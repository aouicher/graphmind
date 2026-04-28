use graphmind_core::{parse_single, SymbolKind};

fn fixture(name: &str) -> (String, String) {
    let path = format!(
        "{}/tests/fixtures/sample-project/{}",
        env!("CARGO_MANIFEST_DIR").replace("/crates/graphmind-core", ""),
        name
    );
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"));
    (path, source)
}

// ══════════════════════════════════════════════════════════════════
// Python
// ══════════════════════════════════════════════════════════════════

#[test]
fn python_symbols() {
    let (path, source) = fixture("src/services/wallet.py");
    let parsed = parse_single(&path, &source, "python").unwrap();
    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"WalletService"), "missing class WalletService: {:?}", names);
    assert!(names.contains(&"create_wallet"), "missing method create_wallet: {:?}", names);
    assert!(names.contains(&"get_balance"), "missing method get_balance: {:?}", names);
    assert!(names.contains(&"_persist_wallet"), "missing method _persist_wallet: {:?}", names);
    assert!(names.contains(&"_fetch_balance"), "missing method _fetch_balance: {:?}", names);
    assert!(names.contains(&"__init__"), "missing __init__: {:?}", names);

    let cls = parsed.symbols.iter().find(|s| s.name == "WalletService").unwrap();
    assert!(matches!(cls.kind, SymbolKind::Class));
}

#[test]
fn python_member_expression_resolved() {
    let (path, source) = fixture("src/services/wallet.py");
    let parsed = parse_single(&path, &source, "python").unwrap();

    for cs in &parsed.call_sites {
        assert!(
            !cs.callee.contains('.'),
            "Python: unresolved member expression '{}' (caller: {})",
            cs.callee, cs.caller
        );
    }

    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();
    assert!(callees.contains(&"info"), "missing call to self.logger.info → 'info': {:?}", callees);
    assert!(callees.contains(&"validate_address"), "missing call to validate_address: {:?}", callees);
}

#[test]
fn python_call_graph() {
    let (path, source) = fixture("src/services/wallet.py");
    let parsed = parse_single(&path, &source, "python").unwrap();

    let pairs: Vec<(&str, &str)> = parsed.call_sites.iter()
        .map(|c| (c.caller.as_str(), c.callee.as_str()))
        .collect();

    assert!(pairs.contains(&("create_wallet", "validate_address")),
        "create_wallet should call validate_address: {:?}", pairs);
    assert!(pairs.contains(&("create_wallet", "_persist_wallet")),
        "create_wallet should call _persist_wallet: {:?}", pairs);
    assert!(pairs.contains(&("get_balance", "_fetch_balance")),
        "get_balance should call _fetch_balance: {:?}", pairs);
}

#[test]
fn python_imports() {
    let (path, source) = fixture("src/services/wallet.py");
    let parsed = parse_single(&path, &source, "python").unwrap();

    let specifiers: Vec<&str> = parsed.imports.iter()
        .flat_map(|i| i.specifiers.iter().map(|s| s.as_str()))
        .collect();

    assert!(specifiers.contains(&"validate_address"), "missing import validate_address: {:?}", specifiers);
    assert!(specifiers.contains(&"Logger"), "missing import Logger: {:?}", specifiers);
}

// ══════════════════════════════════════════════════════════════════
// Go
// ══════════════════════════════════════════════════════════════════

#[test]
fn go_symbols() {
    let (path, source) = fixture("src/services/wallet.go");
    let parsed = parse_single(&path, &source, "go").unwrap();
    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"Wallet"), "missing struct Wallet: {:?}", names);
    assert!(names.contains(&"WalletService"), "missing struct WalletService: {:?}", names);
    assert!(names.contains(&"NewWalletService"), "missing function NewWalletService: {:?}", names);
    assert!(names.contains(&"CreateWallet"), "missing method CreateWallet: {:?}", names);
    assert!(names.contains(&"GetBalance"), "missing method GetBalance: {:?}", names);
    assert!(names.contains(&"persistWallet"), "missing method persistWallet: {:?}", names);
    assert!(names.contains(&"fetchBalance"), "missing method fetchBalance: {:?}", names);

    let wallet = parsed.symbols.iter().find(|s| s.name == "Wallet").unwrap();
    assert!(matches!(wallet.kind, SymbolKind::Class), "Wallet should be Class (struct)");

    let new_fn = parsed.symbols.iter().find(|s| s.name == "NewWalletService").unwrap();
    assert!(matches!(new_fn.kind, SymbolKind::Function));

    let method = parsed.symbols.iter().find(|s| s.name == "CreateWallet").unwrap();
    assert!(matches!(method.kind, SymbolKind::Method));
}

#[test]
fn go_member_expression_resolved() {
    let (path, source) = fixture("src/services/wallet.go");
    let parsed = parse_single(&path, &source, "go").unwrap();

    for cs in &parsed.call_sites {
        assert!(
            !cs.callee.contains('.'),
            "Go: unresolved selector expression '{}' (caller: {})",
            cs.callee, cs.caller
        );
    }

    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();
    assert!(callees.contains(&"Info"), "missing call to s.logger.Info → 'Info': {:?}", callees);
    assert!(callees.contains(&"ValidateAddress"), "missing call to utils.ValidateAddress: {:?}", callees);
}

#[test]
fn go_call_graph() {
    let (path, source) = fixture("src/services/wallet.go");
    let parsed = parse_single(&path, &source, "go").unwrap();

    let pairs: Vec<(&str, &str)> = parsed.call_sites.iter()
        .map(|c| (c.caller.as_str(), c.callee.as_str()))
        .collect();

    assert!(pairs.contains(&("CreateWallet", "persistWallet")),
        "CreateWallet should call persistWallet: {:?}", pairs);
    assert!(pairs.contains(&("GetBalance", "fetchBalance")),
        "GetBalance should call fetchBalance: {:?}", pairs);
}

#[test]
fn go_imports() {
    let (path, source) = fixture("src/services/wallet.go");
    let parsed = parse_single(&path, &source, "go").unwrap();

    let sources: Vec<&str> = parsed.imports.iter().map(|i| i.source.as_str()).collect();
    assert!(sources.contains(&"fmt"), "missing import fmt: {:?}", sources);
    assert!(sources.contains(&"myapp/utils"), "missing import myapp/utils: {:?}", sources);
}

// ══════════════════════════════════════════════════════════════════
// Rust
// ══════════════════════════════════════════════════════════════════

#[test]
fn rust_symbols() {
    let (path, source) = fixture("src/services/wallet.rs");
    let parsed = parse_single(&path, &source, "rust").unwrap();
    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"Wallet"), "missing struct Wallet: {:?}", names);
    assert!(names.contains(&"WalletError"), "missing enum WalletError: {:?}", names);
    assert!(names.contains(&"WalletOps"), "missing trait WalletOps: {:?}", names);
    assert!(names.contains(&"WalletService"), "missing struct WalletService: {:?}", names);
    assert!(names.contains(&"new"), "missing method new: {:?}", names);
    assert!(names.contains(&"create_wallet"), "missing method create_wallet: {:?}", names);
    assert!(names.contains(&"get_balance"), "missing method get_balance: {:?}", names);
    assert!(names.contains(&"persist_wallet"), "missing method persist_wallet: {:?}", names);
    assert!(names.contains(&"fetch_balance"), "missing method fetch_balance: {:?}", names);

    let wallet = parsed.symbols.iter().find(|s| s.name == "Wallet").unwrap();
    assert!(matches!(wallet.kind, SymbolKind::Class), "struct → Class");

    let error = parsed.symbols.iter().find(|s| s.name == "WalletError").unwrap();
    assert!(matches!(error.kind, SymbolKind::Type), "enum → Type");

    let ops = parsed.symbols.iter().find(|s| s.name == "WalletOps").unwrap();
    assert!(matches!(ops.kind, SymbolKind::Interface), "trait → Interface");

    let method = parsed.symbols.iter().find(|s| s.name == "create_wallet").unwrap();
    assert!(matches!(method.kind, SymbolKind::Method), "impl fn → Method");
}

#[test]
fn rust_member_expression_resolved() {
    let (path, source) = fixture("src/services/wallet.rs");
    let parsed = parse_single(&path, &source, "rust").unwrap();

    for cs in &parsed.call_sites {
        assert!(
            !cs.callee.contains('.') && !cs.callee.contains("::"),
            "Rust: unresolved expression '{}' (caller: {})",
            cs.callee, cs.caller
        );
    }

    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();
    assert!(callees.contains(&"info"), "missing call to self.logger.info → 'info': {:?}", callees);
    assert!(callees.contains(&"validate_address"), "missing call to validate_address: {:?}", callees);
}

#[test]
fn rust_call_graph() {
    let (path, source) = fixture("src/services/wallet.rs");
    let parsed = parse_single(&path, &source, "rust").unwrap();

    let pairs: Vec<(&str, &str)> = parsed.call_sites.iter()
        .map(|c| (c.caller.as_str(), c.callee.as_str()))
        .collect();

    assert!(pairs.contains(&("create_wallet", "persist_wallet")),
        "create_wallet should call persist_wallet: {:?}", pairs);
    assert!(pairs.contains(&("get_balance", "fetch_balance")),
        "get_balance should call fetch_balance: {:?}", pairs);
}

#[test]
fn rust_imports() {
    let (path, source) = fixture("src/services/wallet.rs");
    let parsed = parse_single(&path, &source, "rust").unwrap();

    assert!(!parsed.imports.is_empty(), "should have use declarations");

    let all_specs: Vec<&str> = parsed.imports.iter()
        .flat_map(|i| i.specifiers.iter().map(|s| s.as_str()))
        .collect();
    assert!(
        all_specs.iter().any(|s| s.contains("validate_address") || s.contains("validator")),
        "should import from validator: {:?}", all_specs
    );
}

// ══════════════════════════════════════════════════════════════════
// Ruby
// ══════════════════════════════════════════════════════════════════

#[test]
fn ruby_symbols() {
    let (path, source) = fixture("src/services/wallet.rb");
    let parsed = parse_single(&path, &source, "ruby").unwrap();
    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"WalletService"), "missing class WalletService: {:?}", names);
    assert!(names.contains(&"initialize"), "missing initialize: {:?}", names);
    assert!(names.contains(&"create_wallet"), "missing create_wallet: {:?}", names);
    assert!(names.contains(&"get_balance"), "missing get_balance: {:?}", names);
    assert!(names.contains(&"persist_wallet"), "missing persist_wallet: {:?}", names);
    assert!(names.contains(&"fetch_balance"), "missing fetch_balance: {:?}", names);

    let cls = parsed.symbols.iter().find(|s| s.name == "WalletService").unwrap();
    assert!(matches!(cls.kind, SymbolKind::Class));
}

#[test]
fn ruby_call_sites() {
    let (path, source) = fixture("src/services/wallet.rb");
    let parsed = parse_single(&path, &source, "ruby").unwrap();

    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();
    assert!(callees.contains(&"info"), "missing call to @logger.info → 'info': {:?}", callees);
    assert!(callees.contains(&"validate_address"), "missing call to validate_address: {:?}", callees);
    assert!(callees.contains(&"persist_wallet"), "missing call to persist_wallet: {:?}", callees);
}

#[test]
fn ruby_imports() {
    let (path, source) = fixture("src/services/wallet.rb");
    let parsed = parse_single(&path, &source, "ruby").unwrap();

    assert!(!parsed.imports.is_empty(), "should have require_relative imports");

    let sources: Vec<&str> = parsed.imports.iter().map(|i| i.source.as_str()).collect();
    assert!(
        sources.iter().any(|s| s.contains("validator")),
        "should require validator: {:?}", sources
    );
    assert!(
        sources.iter().any(|s| s.contains("logger")),
        "should require logger: {:?}", sources
    );
}

// ══════════════════════════════════════════════════════════════════
// Cross-language consistency
// ══════════════════════════════════════════════════════════════════

#[test]
fn all_languages_find_wallet_service() {
    let langs = vec![
        ("src/services/wallet.ts", "typescript"),
        ("src/services/wallet.py", "python"),
        ("src/services/wallet.go", "go"),
        ("src/services/wallet.rs", "rust"),
        ("src/services/wallet.rb", "ruby"),
    ];

    for (file, lang) in langs {
        let (path, source) = fixture(file);
        let parsed = parse_single(&path, &source, lang).unwrap();
        let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"WalletService"),
            "{lang}: missing WalletService in {file}"
        );
    }
}

#[test]
fn all_languages_resolve_member_expressions() {
    let langs = vec![
        ("src/services/wallet.ts", "typescript"),
        ("src/services/wallet.py", "python"),
        ("src/services/wallet.go", "go"),
        ("src/services/wallet.rs", "rust"),
    ];

    for (file, lang) in langs {
        let (path, source) = fixture(file);
        let parsed = parse_single(&path, &source, lang).unwrap();
        for cs in &parsed.call_sites {
            assert!(
                !cs.callee.contains('.'),
                "{lang} ({file}): unresolved member expression '{}'",
                cs.callee
            );
        }
    }
}

#[test]
fn all_languages_have_call_sites() {
    let langs = vec![
        ("src/services/wallet.ts", "typescript"),
        ("src/services/wallet.py", "python"),
        ("src/services/wallet.go", "go"),
        ("src/services/wallet.rs", "rust"),
        ("src/services/wallet.rb", "ruby"),
    ];

    for (file, lang) in langs {
        let (path, source) = fixture(file);
        let parsed = parse_single(&path, &source, lang).unwrap();
        assert!(
            !parsed.call_sites.is_empty(),
            "{lang}: no call sites extracted from {file}"
        );
        assert!(
            parsed.call_sites.len() >= 4,
            "{lang}: expected >= 4 call sites, got {} in {file}",
            parsed.call_sites.len()
        );
    }
}

#[test]
fn all_languages_have_imports() {
    let langs = vec![
        ("src/services/wallet.ts", "typescript"),
        ("src/services/wallet.py", "python"),
        ("src/services/wallet.go", "go"),
        ("src/services/wallet.rs", "rust"),
        ("src/services/wallet.rb", "ruby"),
    ];

    for (file, lang) in langs {
        let (path, source) = fixture(file);
        let parsed = parse_single(&path, &source, lang).unwrap();
        assert!(
            !parsed.imports.is_empty(),
            "{lang}: no imports extracted from {file}"
        );
    }
}
