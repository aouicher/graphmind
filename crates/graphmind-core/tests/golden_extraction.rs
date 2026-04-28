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

// ── Symbol extraction ────────────────────────────────────────────

#[test]
fn wallet_service_symbols() {
    let (path, source) = fixture("src/services/wallet.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"WalletService"), "missing class WalletService");
    assert!(names.contains(&"createWallet"), "missing method createWallet");
    assert!(names.contains(&"getBalance"), "missing method getBalance");
    assert!(names.contains(&"persistWallet"), "missing method persistWallet");
    assert!(names.contains(&"fetchBalance"), "missing method fetchBalance");
    assert!(names.contains(&"Wallet"), "missing interface Wallet");

    let class = parsed.symbols.iter().find(|s| s.name == "WalletService").unwrap();
    assert!(matches!(class.kind, SymbolKind::Class));

    let method = parsed.symbols.iter().find(|s| s.name == "createWallet").unwrap();
    assert!(matches!(method.kind, SymbolKind::Method));
}

#[test]
fn route_symbols() {
    let (path, source) = fixture("src/routes/wallet.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"handleCreateWallet"), "missing handleCreateWallet");
    assert!(names.contains(&"handleGetBalance"), "missing handleGetBalance");
    assert!(names.contains(&"Request"), "missing interface Request");
    assert!(names.contains(&"Response"), "missing interface Response");
}

#[test]
fn util_symbols() {
    let (path, source) = fixture("src/utils/validator.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"validateAddress"), "missing validateAddress");
    assert!(names.contains(&"sanitizeInput"), "missing sanitizeInput");

    for sym in &parsed.symbols {
        assert!(matches!(sym.kind, SymbolKind::Function));
    }
}

// ── Call site extraction (member expression resolution) ──────────

#[test]
fn member_expression_resolved_to_method_name() {
    let (path, source) = fixture("src/services/wallet.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();

    // this.logger.info() should resolve to "info", not "this.logger.info"
    assert!(callees.contains(&"info"), "member expression not resolved: expected 'info', got {:?}", callees);
    // this.persistWallet() should resolve to "persistWallet"
    assert!(callees.contains(&"persistWallet"), "missing call to persistWallet");
    // validateAddress() is a direct call
    assert!(callees.contains(&"validateAddress"), "missing call to validateAddress");
    // this.fetchBalance()
    assert!(callees.contains(&"fetchBalance"), "missing call to fetchBalance");

    // No callee should contain a dot (member expressions must be resolved)
    for cs in &parsed.call_sites {
        assert!(
            !cs.callee.contains('.'),
            "unresolved member expression in callee: '{}' (caller: {})",
            cs.callee, cs.caller
        );
    }
}

#[test]
fn route_calls_service_methods() {
    let (path, source) = fixture("src/routes/wallet.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let call_pairs: Vec<(&str, &str)> = parsed
        .call_sites
        .iter()
        .map(|c| (c.caller.as_str(), c.callee.as_str()))
        .collect();

    // handleCreateWallet calls walletService.createWallet → resolved to "createWallet"
    assert!(
        call_pairs.contains(&("handleCreateWallet", "createWallet")),
        "missing call handleCreateWallet → createWallet, got {:?}",
        call_pairs
    );
    // handleGetBalance calls walletService.getBalance → resolved to "getBalance"
    assert!(
        call_pairs.contains(&("handleGetBalance", "getBalance")),
        "missing call handleGetBalance → getBalance, got {:?}",
        call_pairs
    );

    // No dots in callees
    for cs in &parsed.call_sites {
        assert!(
            !cs.callee.contains('.'),
            "unresolved member expression: '{}'",
            cs.callee
        );
    }
}

#[test]
fn index_calls() {
    let (path, source) = fixture("src/index.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let call_pairs: Vec<(&str, &str)> = parsed
        .call_sites
        .iter()
        .map(|c| (c.caller.as_str(), c.callee.as_str()))
        .collect();

    assert!(
        call_pairs.contains(&("startServer", "info")),
        "startServer should call logger.info (resolved to 'info')"
    );
    assert!(
        call_pairs.contains(&("startServer", "registerRoutes")),
        "startServer should call registerRoutes"
    );
}

// ── Import extraction ────────────────────────────────────────────

#[test]
fn wallet_service_imports() {
    let (path, source) = fixture("src/services/wallet.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let import_sources: Vec<&str> = parsed.imports.iter().map(|i| i.source.as_str()).collect();
    assert!(
        import_sources.iter().any(|s| s.contains("validator")),
        "missing import from validator, got {:?}",
        import_sources
    );
    assert!(
        import_sources.iter().any(|s| s.contains("logger")),
        "missing import from logger, got {:?}",
        import_sources
    );

    let specifiers: Vec<&str> = parsed
        .imports
        .iter()
        .flat_map(|i| i.specifiers.iter().map(|s| s.as_str()))
        .collect();
    assert!(specifiers.contains(&"validateAddress"), "missing specifier validateAddress");
    assert!(specifiers.contains(&"Logger"), "missing specifier Logger");
}

#[test]
fn route_imports() {
    let (path, source) = fixture("src/routes/wallet.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let specifiers: Vec<&str> = parsed
        .imports
        .iter()
        .flat_map(|i| i.specifiers.iter().map(|s| s.as_str()))
        .collect();
    assert!(specifiers.contains(&"WalletService"), "missing specifier WalletService");
}

// ── Signature extraction ─────────────────────────────────────────

#[test]
fn function_signatures_present() {
    let (path, source) = fixture("src/utils/validator.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    let validate = parsed.symbols.iter().find(|s| s.name == "validateAddress").unwrap();
    let sig = validate.signature.as_deref().unwrap();
    assert!(sig.contains("address"), "signature should contain param name: {sig}");
    assert!(sig.contains("string"), "signature should contain type: {sig}");
}

// ── Line numbers ─────────────────────────────────────────────────

#[test]
fn line_numbers_are_sensible() {
    let (path, source) = fixture("src/services/wallet.ts");
    let parsed = parse_single(&path, &source, "typescript").unwrap();

    for sym in &parsed.symbols {
        assert!(sym.line_start >= 1, "line_start should be >= 1: {}", sym.name);
        assert!(sym.line_end >= sym.line_start, "line_end < line_start: {}", sym.name);
    }

    for cs in &parsed.call_sites {
        assert!(cs.line >= 1, "call site line should be >= 1");
    }
}
