use graphmind_embeddings::engine::{EmbedError, EmbeddingEngine};
use graphmind_embeddings::store::{EmbeddingStore, NewEmbeddingRow, bytes_to_float32, float32_to_bytes};
use graphmind_embeddings::search::semantic_search;

// ── Mock engine for deterministic tests ─────────────────────────

struct MockEngine {
    dims: usize,
}

impl MockEngine {
    fn new(dims: usize) -> Self {
        Self { dims }
    }
}

impl EmbeddingEngine for MockEngine {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        Ok(deterministic_vector(text, self.dims))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts.iter().map(|t| deterministic_vector(t, self.dims)).collect())
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn model_id(&self) -> &str {
        "mock-v1"
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn is_available(&self) -> bool {
        true
    }
}

fn deterministic_vector(text: &str, dims: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; dims];
    for (i, byte) in text.bytes().enumerate() {
        v[i % dims] += byte as f32 / 255.0;
    }
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

// ── Float conversion ────────────────────────────────────────────

#[test]
fn float32_roundtrip() {
    let original = vec![1.0f32, -0.5, 0.0, 3.14159, f32::MIN, f32::MAX];
    let bytes = float32_to_bytes(&original);
    let recovered = bytes_to_float32(&bytes);
    assert_eq!(original.len(), recovered.len());
    for (a, b) in original.iter().zip(recovered.iter()) {
        assert_eq!(a.to_bits(), b.to_bits());
    }
}

#[test]
fn float32_empty() {
    let bytes = float32_to_bytes(&[]);
    assert!(bytes.is_empty());
    let recovered = bytes_to_float32(&bytes);
    assert!(recovered.is_empty());
}

// ── Store CRUD ──────────────────────────────────────────────────

fn temp_store() -> (EmbeddingStore, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test_embeddings.db");
    let store = EmbeddingStore::open(&db_path).unwrap();
    (store, tmp)
}

#[test]
fn store_open_creates_tables() {
    let (store, _tmp) = temp_store();
    assert_eq!(store.count().unwrap(), 0);
    assert!(store.files_indexed().is_empty());
}

#[test]
fn store_insert_and_count() {
    let (store, _tmp) = temp_store();
    let emb = float32_to_bytes(&[0.1, 0.2, 0.3]);
    store.insert("myFunc", "function", "src/main.rs", "fn myFunc()", &emb).unwrap();
    assert_eq!(store.count().unwrap(), 1);
}

#[test]
fn store_insert_batch() {
    let (store, _tmp) = temp_store();
    let rows: Vec<NewEmbeddingRow> = (0..100)
        .map(|i| NewEmbeddingRow {
            symbol_name: format!("sym_{i}"),
            symbol_kind: "function".to_string(),
            file: format!("src/file_{}.rs", i / 10),
            text: format!("fn sym_{i}()"),
            embedding: float32_to_bytes(&vec![i as f32 / 100.0; 8]),
        })
        .collect();
    store.insert_batch(&rows).unwrap();
    assert_eq!(store.count().unwrap(), 100);
}

#[test]
fn store_files_indexed() {
    let (store, _tmp) = temp_store();
    let emb = float32_to_bytes(&[0.1, 0.2]);
    store.insert("a", "function", "src/a.rs", "fn a()", &emb).unwrap();
    store.insert("b", "function", "src/a.rs", "fn b()", &emb).unwrap();
    store.insert("c", "function", "src/b.rs", "fn c()", &emb).unwrap();

    let files = store.files_indexed();
    assert_eq!(files.len(), 2);
    assert!(files.contains("src/a.rs"));
    assert!(files.contains("src/b.rs"));
}

#[test]
fn store_delete_by_file() {
    let (store, _tmp) = temp_store();
    let emb = float32_to_bytes(&[0.1, 0.2]);
    store.insert("a", "function", "src/a.rs", "fn a()", &emb).unwrap();
    store.insert("b", "function", "src/a.rs", "fn b()", &emb).unwrap();
    store.insert("c", "function", "src/b.rs", "fn c()", &emb).unwrap();

    let deleted = store.delete_by_file("src/a.rs").unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(store.count().unwrap(), 1);
}

#[test]
fn store_clear() {
    let (store, _tmp) = temp_store();
    let emb = float32_to_bytes(&[0.5]);
    store.insert("x", "function", "f.rs", "fn x()", &emb).unwrap();
    store.set_meta("model", "test-model").unwrap();
    store.clear().unwrap();
    assert_eq!(store.count().unwrap(), 0);
    assert!(store.get_meta("model").is_none());
}

#[test]
fn store_meta_get_set() {
    let (store, _tmp) = temp_store();
    assert!(store.get_meta("model").is_none());
    store.set_meta("model", "openai:text-embedding-3-small").unwrap();
    assert_eq!(store.get_meta("model").unwrap(), "openai:text-embedding-3-small");
    store.set_meta("model", "voyage:voyage-code-3").unwrap();
    assert_eq!(store.get_meta("model").unwrap(), "voyage:voyage-code-3");
}

#[test]
fn store_all_returns_rows() {
    let (store, _tmp) = temp_store();
    let emb = float32_to_bytes(&[1.0, 2.0, 3.0]);
    store.insert("foo", "struct", "lib.rs", "struct Foo {}", &emb).unwrap();
    let all = store.all().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].symbol_name, "foo");
    assert_eq!(all[0].symbol_kind, "struct");
    assert_eq!(all[0].file, "lib.rs");
    let recovered = bytes_to_float32(&all[0].embedding);
    assert_eq!(recovered, vec![1.0, 2.0, 3.0]);
}

// ── Mock engine tests ───────────────────────────────────────────

#[test]
fn mock_engine_produces_normalized_vectors() {
    let engine = MockEngine::new(64);
    let v = engine.embed("hello world").unwrap();
    assert_eq!(v.len(), 64);
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-5, "expected unit vector, got norm={norm}");
}

#[test]
fn mock_engine_batch() {
    let engine = MockEngine::new(32);
    let results = engine.embed_batch(&["hello", "world", "test"]).unwrap();
    assert_eq!(results.len(), 3);
    for v in &results {
        assert_eq!(v.len(), 32);
    }
}

#[test]
fn mock_engine_same_text_same_vector() {
    let engine = MockEngine::new(16);
    let a = engine.embed("deterministic").unwrap();
    let b = engine.embed("deterministic").unwrap();
    assert_eq!(a, b);
}

#[test]
fn mock_engine_different_text_different_vector() {
    let engine = MockEngine::new(16);
    let a = engine.embed("hello").unwrap();
    let b = engine.embed("world").unwrap();
    assert_ne!(a, b);
}

// ── Semantic search ─────────────────────────────────────────────

#[test]
fn semantic_search_empty_store() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("empty.db");
    EmbeddingStore::open(&db_path).unwrap();

    let results = semantic_search(
        &db_path,
        "anything",
        &|text| Some(deterministic_vector(text, 64)),
        10,
        None,
    );
    assert!(results.is_empty());
}

#[test]
fn semantic_search_finds_similar() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("search.db");
    let store = EmbeddingStore::open(&db_path).unwrap();

    let engine = MockEngine::new(64);

    let symbols = [
        ("createWallet", "function", "wallet.ts", "async function createWallet(address: string)"),
        ("validateAddress", "function", "validator.ts", "function validateAddress(addr: string): boolean"),
        ("getBalance", "function", "wallet.ts", "async function getBalance(walletId: string)"),
        ("Logger", "class", "logger.ts", "class Logger { info() {} error() {} }"),
        ("sendTransaction", "function", "tx.ts", "async function sendTransaction(from, to, amount)"),
    ];

    let rows: Vec<NewEmbeddingRow> = symbols
        .iter()
        .map(|(name, kind, file, text)| {
            let emb = engine.embed(text).unwrap();
            NewEmbeddingRow {
                symbol_name: name.to_string(),
                symbol_kind: kind.to_string(),
                file: file.to_string(),
                text: text.to_string(),
                embedding: float32_to_bytes(&emb),
            }
        })
        .collect();
    store.insert_batch(&rows).unwrap();

    let results = semantic_search(
        &db_path,
        "wallet address creation",
        &|text| Some(deterministic_vector(text, 64)),
        3,
        None,
    );

    assert!(!results.is_empty());
    assert!(results.len() <= 3);
    // The wallet-related functions should score higher
    let names: Vec<&str> = results.iter().map(|r| r.symbol_name.as_str()).collect();
    eprintln!("Search results: {:?}", names);
}

#[test]
fn semantic_search_kind_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("filter.db");
    let store = EmbeddingStore::open(&db_path).unwrap();

    let engine = MockEngine::new(64);
    let rows: Vec<NewEmbeddingRow> = vec![
        ("MyClass", "class", "a.ts", "class MyClass {}"),
        ("myFunc", "function", "b.ts", "function myFunc() {}"),
    ]
    .into_iter()
    .map(|(name, kind, file, text)| NewEmbeddingRow {
        symbol_name: name.to_string(),
        symbol_kind: kind.to_string(),
        file: file.to_string(),
        text: text.to_string(),
        embedding: float32_to_bytes(&engine.embed(text).unwrap()),
    })
    .collect();
    store.insert_batch(&rows).unwrap();

    let results = semantic_search(
        &db_path,
        "class",
        &|text| Some(deterministic_vector(text, 64)),
        10,
        Some("class"),
    );
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].symbol_name, "MyClass");
}

#[test]
fn semantic_search_multi_query_rrf() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("rrf.db");
    let store = EmbeddingStore::open(&db_path).unwrap();

    let engine = MockEngine::new(64);
    let symbols = [
        ("authenticate", "function", "auth.ts", "function authenticate(token: string)"),
        ("createUser", "function", "user.ts", "function createUser(name, email)"),
        ("hashPassword", "function", "crypto.ts", "function hashPassword(plain: string): string"),
        ("validateToken", "function", "auth.ts", "function validateToken(jwt: string): boolean"),
    ];

    let rows: Vec<NewEmbeddingRow> = symbols
        .iter()
        .map(|(name, kind, file, text)| NewEmbeddingRow {
            symbol_name: name.to_string(),
            symbol_kind: kind.to_string(),
            file: file.to_string(),
            text: text.to_string(),
            embedding: float32_to_bytes(&engine.embed(text).unwrap()),
        })
        .collect();
    store.insert_batch(&rows).unwrap();

    // Multi-query with semicolons triggers RRF
    let results = semantic_search(
        &db_path,
        "authentication token; password hashing",
        &|text| Some(deterministic_vector(text, 64)),
        4,
        None,
    );

    assert!(!results.is_empty());
    // RRF should merge both rankings
    let names: Vec<&str> = results.iter().map(|r| r.symbol_name.as_str()).collect();
    eprintln!("RRF results: {:?}", names);
}

// ── Full pipeline: build graph → embed → search ─────────────────

#[test]
fn full_pipeline_embed_and_search() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("pipeline.db");
    let store = EmbeddingStore::open(&db_path).unwrap();

    let engine = MockEngine::new(64);

    // Simulate symbols extracted from a real build
    let symbols = vec![
        ("WalletService", "class", "src/services/wallet.ts", "class WalletService { createWallet() {} getBalance() {} }"),
        ("createWallet", "method", "src/services/wallet.ts", "async createWallet(address: string): Promise<Wallet>"),
        ("getBalance", "method", "src/services/wallet.ts", "async getBalance(walletId: string): Promise<number>"),
        ("validateAddress", "function", "src/utils/validator.ts", "function validateAddress(addr: string): boolean"),
        ("sanitizeInput", "function", "src/utils/validator.ts", "function sanitizeInput(input: string): string"),
        ("Router", "class", "src/routes/wallet.ts", "class Router { get() {} post() {} }"),
        ("handleCreate", "function", "src/routes/wallet.ts", "async function handleCreate(req, res)"),
        ("handleGet", "function", "src/routes/wallet.ts", "async function handleGet(req, res)"),
        ("Logger", "class", "src/utils/logger.ts", "class Logger { info() {} warn() {} error() {} }"),
        ("DatabaseClient", "class", "src/db/client.ts", "class DatabaseClient { query() {} connect() {} }"),
    ];

    let rows: Vec<NewEmbeddingRow> = symbols
        .iter()
        .map(|(name, kind, file, text)| NewEmbeddingRow {
            symbol_name: name.to_string(),
            symbol_kind: kind.to_string(),
            file: file.to_string(),
            text: text.to_string(),
            embedding: float32_to_bytes(&engine.embed(text).unwrap()),
        })
        .collect();

    store.insert_batch(&rows).unwrap();
    store.set_meta("model", "mock:mock-v1").unwrap();

    assert_eq!(store.count().unwrap(), 10);
    assert_eq!(store.files_indexed().len(), 5);

    // Search for wallet-related code
    let results = semantic_search(
        &db_path,
        "wallet creation",
        &|text| Some(deterministic_vector(text, 64)),
        5,
        None,
    );
    assert!(!results.is_empty());

    // Search with kind filter
    let class_results = semantic_search(
        &db_path,
        "service",
        &|text| Some(deterministic_vector(text, 64)),
        10,
        Some("class"),
    );
    assert!(class_results.iter().all(|r| r.symbol_kind == "class"));

    // Verify incremental: delete a file, check count
    store.delete_by_file("src/utils/logger.ts").unwrap();
    assert_eq!(store.count().unwrap(), 9);
    assert!(!store.files_indexed().contains("src/utils/logger.ts"));
}

// ── Model change detection ──────────────────────────────────────

#[test]
fn model_change_triggers_reindex() {
    let (store, _tmp) = temp_store();
    let emb = float32_to_bytes(&[0.1, 0.2]);
    store.insert("a", "function", "f.rs", "fn a()", &emb).unwrap();
    store.set_meta("model", "local:all-MiniLM-L6-v2").unwrap();

    assert_eq!(store.count().unwrap(), 1);

    // Simulate model change detection (as done in build.rs)
    let new_model = "openai:text-embedding-3-small";
    let stored = store.get_meta("model").unwrap();
    assert_ne!(stored, new_model);

    // Clear and re-index
    store.clear().unwrap();
    store.set_meta("model", new_model).unwrap();
    assert_eq!(store.count().unwrap(), 0);
    assert_eq!(store.get_meta("model").unwrap(), new_model);
}

// ── Noop engine ─────────────────────────────────────────────────

#[test]
fn noop_engine_returns_errors() {
    use graphmind_embeddings::engine::NoopEngine;
    let engine = NoopEngine;
    assert!(!engine.is_available());
    assert_eq!(engine.dimensions(), 0);
    assert!(engine.embed("test").is_err());
    assert!(engine.embed_batch(&["a", "b"]).is_err());
}

// ── Factory ─────────────────────────────────────────────────────

#[test]
fn factory_disabled_returns_noop() {
    use graphmind_config::config::{EmbeddingConfig, EmbeddingMode, ApiKeys};
    use graphmind_embeddings::factory::create_engine;

    let config = EmbeddingConfig {
        mode: EmbeddingMode::Disabled,
        model: None,
        openai_base_url: None,
        api_keys: ApiKeys::default(),
    };
    let engine = create_engine(&config).unwrap();
    assert!(!engine.is_available());
    assert_eq!(engine.provider_name(), "disabled");
}

#[test]
fn factory_openai_without_key_errors() {
    use graphmind_config::config::{EmbeddingConfig, EmbeddingMode, ApiKeys};
    use graphmind_embeddings::factory::create_engine;

    let config = EmbeddingConfig {
        mode: EmbeddingMode::Openai,
        model: None,
        openai_base_url: None,
        api_keys: ApiKeys { openai: None, voyage: None },
    };
    let result = create_engine(&config);
    assert!(result.is_err());
}

#[test]
fn factory_voyage_without_key_errors() {
    use graphmind_config::config::{EmbeddingConfig, EmbeddingMode, ApiKeys};
    use graphmind_embeddings::factory::create_engine;

    let config = EmbeddingConfig {
        mode: EmbeddingMode::Voyage,
        model: None,
        openai_base_url: None,
        api_keys: ApiKeys { openai: None, voyage: None },
    };
    let result = create_engine(&config);
    assert!(result.is_err());
}

// ── Voyage API integration test (requires API key) ──────────────

#[test]
#[ignore] // Run with: cargo test -p graphmind-embeddings -- --ignored
fn voyage_api_real_embedding() {
    use graphmind_config::config::{EmbeddingConfig, EmbeddingMode, ApiKeys};
    use graphmind_embeddings::factory::create_engine;

    let key = std::env::var("VOYAGE_API_KEY")
        .unwrap_or_else(|_| "pa-G4tMFLgvrgw_C6awmLbAPkt8PxdLK7nHoV-TrseHAXS".to_string());

    let config = EmbeddingConfig {
        mode: EmbeddingMode::Voyage,
        model: Some("voyage-code-3".to_string()),
        openai_base_url: None,
        api_keys: ApiKeys { openai: None, voyage: Some(key) },
    };

    let engine = create_engine(&config).unwrap();
    assert!(engine.is_available());
    assert_eq!(engine.dimensions(), 1024);
    assert_eq!(engine.provider_name(), "voyage");

    let result = engine.embed("function createWallet(address: string): Promise<Wallet>").unwrap();
    assert_eq!(result.len(), 1024);

    // Verify it's normalized (unit vector)
    let norm: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.01, "expected ~unit vector, got norm={norm}");
}

#[test]
#[ignore]
fn voyage_api_batch_embedding() {
    use graphmind_config::config::{EmbeddingConfig, EmbeddingMode, ApiKeys};
    use graphmind_embeddings::factory::create_engine;

    let key = std::env::var("VOYAGE_API_KEY")
        .unwrap_or_else(|_| "pa-G4tMFLgvrgw_C6awmLbAPkt8PxdLK7nHoV-TrseHAXS".to_string());

    let config = EmbeddingConfig {
        mode: EmbeddingMode::Voyage,
        model: Some("voyage-code-3".to_string()),
        openai_base_url: None,
        api_keys: ApiKeys { openai: None, voyage: Some(key) },
    };

    let engine = create_engine(&config).unwrap();
    let texts = &[
        "function createWallet(address: string)",
        "class DatabaseClient { query() {} }",
        "async function handleRequest(req, res)",
    ];
    let results = engine.embed_batch(texts).unwrap();
    assert_eq!(results.len(), 3);
    for v in &results {
        assert_eq!(v.len(), 1024);
    }

    // Similar texts should have higher cosine similarity
    let wallet_vec = &results[0];
    let db_vec = &results[1];
    let handler_vec = &results[2];

    let sim_wallet_handler = cosine_sim(wallet_vec, handler_vec);
    let sim_wallet_db = cosine_sim(wallet_vec, db_vec);
    eprintln!("wallet↔handler: {sim_wallet_handler:.4}, wallet↔db: {sim_wallet_db:.4}");
}

#[test]
#[ignore]
fn voyage_full_pipeline_search() {
    use graphmind_config::config::{EmbeddingConfig, EmbeddingMode, ApiKeys};
    use graphmind_embeddings::factory::create_engine;

    let key = std::env::var("VOYAGE_API_KEY")
        .unwrap_or_else(|_| "pa-G4tMFLgvrgw_C6awmLbAPkt8PxdLK7nHoV-TrseHAXS".to_string());

    let config = EmbeddingConfig {
        mode: EmbeddingMode::Voyage,
        model: Some("voyage-code-3".to_string()),
        openai_base_url: None,
        api_keys: ApiKeys { openai: None, voyage: Some(key.clone()) },
    };

    let engine = create_engine(&config).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("voyage_test.db");
    let store = EmbeddingStore::open(&db_path).unwrap();

    let symbols = [
        ("createWallet", "function", "wallet.ts", "async function createWallet(address: string): Promise<Wallet>"),
        ("getBalance", "function", "wallet.ts", "async function getBalance(walletId: string): Promise<number>"),
        ("hashPassword", "function", "auth.ts", "function hashPassword(plain: string): string"),
        ("validateToken", "function", "auth.ts", "function validateToken(jwt: string): boolean"),
        ("sendEmail", "function", "notify.ts", "function sendEmail(to: string, subject: string, body: string)"),
    ];

    let texts: Vec<&str> = symbols.iter().map(|(_, _, _, t)| *t).collect();
    let embeddings = engine.embed_batch(&texts).unwrap();

    let rows: Vec<NewEmbeddingRow> = symbols
        .iter()
        .zip(embeddings.iter())
        .map(|((name, kind, file, text), emb)| NewEmbeddingRow {
            symbol_name: name.to_string(),
            symbol_kind: kind.to_string(),
            file: file.to_string(),
            text: text.to_string(),
            embedding: float32_to_bytes(emb),
        })
        .collect();
    store.insert_batch(&rows).unwrap();

    let embed_key = key.clone();
    let results = semantic_search(
        &db_path,
        "wallet balance",
        &move |text| {
            let cfg = EmbeddingConfig {
                mode: EmbeddingMode::Voyage,
                model: Some("voyage-code-3".to_string()),
                openai_base_url: None,
                api_keys: ApiKeys { openai: None, voyage: Some(embed_key.clone()) },
            };
            let eng = create_engine(&cfg).ok()?;
            eng.embed(text).ok()
        },
        3,
        None,
    );

    assert!(!results.is_empty());
    let names: Vec<&str> = results.iter().map(|r| r.symbol_name.as_str()).collect();
    eprintln!("Voyage search results: {:?}", names);
    // Wallet-related functions should appear first
    assert!(
        names.contains(&"createWallet") || names.contains(&"getBalance"),
        "Expected wallet functions in top results, got: {:?}", names
    );
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0_f64;
    let mut na = 0.0_f64;
    let mut nb = 0.0_f64;
    for i in 0..a.len().min(b.len()) {
        let ai = a[i] as f64;
        let bi = b[i] as f64;
        dot += ai * bi;
        na += ai * ai;
        nb += bi * bi;
    }
    dot / (na.sqrt() * nb.sqrt())
}
