use graphmind_core::{parse_single, SymbolKind};

// ══════════════════════════════════════════════════════════════════
// TypeScript edge cases
// ══════════════════════════════════════════════════════════════════

#[test]
fn ts_chained_member_expression() {
    let source = r#"
function doWork() {
    this.a.b.c();
    foo.bar.baz.qux();
}
"#;
    let parsed = parse_single("test.ts", source, "typescript").unwrap();
    for cs in &parsed.call_sites {
        assert!(!cs.callee.contains('.'), "chained member not resolved: '{}'", cs.callee);
    }
    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();
    assert!(callees.contains(&"c"), "this.a.b.c() should resolve to 'c'");
    assert!(callees.contains(&"qux"), "foo.bar.baz.qux() should resolve to 'qux'");
}

#[test]
fn ts_arrow_function_exported() {
    let source = r#"
export const myHandler = (req: Request) => {
    return process(req);
};
"#;
    let parsed = parse_single("test.ts", source, "typescript").unwrap();
    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"myHandler"), "exported arrow function missing");
    let sym = parsed.symbols.iter().find(|s| s.name == "myHandler").unwrap();
    assert!(matches!(sym.kind, SymbolKind::Function));
}

#[test]
fn ts_nested_call_in_argument() {
    let source = r#"
function outer() {
    doSomething(getConfig(), helper.run());
}
"#;
    let parsed = parse_single("test.ts", source, "typescript").unwrap();
    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();
    assert!(callees.contains(&"doSomething"), "missing direct call doSomething");
    assert!(callees.contains(&"getConfig"), "missing nested call getConfig");
    assert!(callees.contains(&"run"), "helper.run() should resolve to 'run'");
}

#[test]
fn ts_constructor_call() {
    let source = r#"
function init() {
    const svc = new MyService();
    const logger = new utils.Logger();
}
"#;
    let parsed = parse_single("test.ts", source, "typescript").unwrap();
    // new expressions are not call_expressions in tree-sitter TS
    // Verify we don't crash or produce garbage
    for cs in &parsed.call_sites {
        assert!(!cs.callee.is_empty(), "empty callee");
    }
}

#[test]
fn ts_empty_file() {
    let parsed = parse_single("empty.ts", "", "typescript").unwrap();
    assert!(parsed.symbols.is_empty());
    assert!(parsed.call_sites.is_empty());
    assert!(parsed.imports.is_empty());
}

#[test]
fn ts_class_with_getters_setters() {
    let source = r#"
class Config {
    get value(): string {
        return this.getValue();
    }
    set value(v: string) {
        this.setValue(v);
    }
    private getValue(): string { return ""; }
    private setValue(v: string) {}
}
"#;
    let parsed = parse_single("test.ts", source, "typescript").unwrap();
    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Config"), "missing class Config");
    assert!(names.contains(&"getValue"), "missing method getValue");
    assert!(names.contains(&"setValue"), "missing method setValue");
}

#[test]
fn ts_multiple_imports() {
    let source = r#"
import { foo, bar } from './module1';
import DefaultExport from './module2';
import * as utils from './utils';
"#;
    let parsed = parse_single("test.ts", source, "typescript").unwrap();
    assert_eq!(parsed.imports.len(), 3, "should have 3 import statements");

    let all_specs: Vec<&str> = parsed.imports.iter()
        .flat_map(|i| i.specifiers.iter().map(|s| s.as_str()))
        .collect();
    assert!(all_specs.contains(&"foo"), "missing specifier foo");
    assert!(all_specs.contains(&"bar"), "missing specifier bar");
    assert!(all_specs.contains(&"DefaultExport"), "missing default export");
}

// ══════════════════════════════════════════════════════════════════
// Python edge cases
// ══════════════════════════════════════════════════════════════════

#[test]
fn python_nested_calls() {
    let source = r#"
def process():
    result = transform(data.get("key"))
    db.session.commit()
"#;
    let parsed = parse_single("test.py", source, "python").unwrap();
    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();
    assert!(callees.contains(&"transform"), "missing transform");
    assert!(callees.contains(&"get"), "data.get() should resolve to 'get'");
    assert!(callees.contains(&"commit"), "db.session.commit() should resolve to 'commit'");
}

#[test]
fn python_decorator_not_a_symbol() {
    let source = r#"
def my_decorator(func):
    def wrapper(*args):
        return func(*args)
    return wrapper

@my_decorator
def decorated_fn():
    pass
"#;
    let parsed = parse_single("test.py", source, "python").unwrap();
    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"my_decorator"));
    assert!(names.contains(&"decorated_fn"));
}

#[test]
fn python_docstring_extracted() {
    let source = r#"
class MyService:
    """A service that does things."""

    def my_method(self):
        """Do the thing."""
        pass
"#;
    let parsed = parse_single("test.py", source, "python").unwrap();
    let cls = parsed.symbols.iter().find(|s| s.name == "MyService").unwrap();
    assert!(cls.doc.is_some(), "class docstring should be extracted");
}

// ══════════════════════════════════════════════════════════════════
// Go edge cases
// ══════════════════════════════════════════════════════════════════

#[test]
fn go_interface_detected() {
    let source = r#"
package main

type Repository interface {
    FindByID(id string) (*Entity, error)
    Save(entity *Entity) error
}
"#;
    let parsed = parse_single("test.go", source, "go").unwrap();
    let repo = parsed.symbols.iter().find(|s| s.name == "Repository").unwrap();
    assert!(matches!(repo.kind, SymbolKind::Interface));
}

#[test]
fn go_multiple_returns() {
    let source = r#"
package main

func divide(a, b float64) (float64, error) {
    if b == 0 {
        return 0, fmt.Errorf("division by zero")
    }
    return a / b, nil
}
"#;
    let parsed = parse_single("test.go", source, "go").unwrap();
    let func = parsed.symbols.iter().find(|s| s.name == "divide").unwrap();
    assert!(func.signature.is_some());
}

// ══════════════════════════════════════════════════════════════════
// Rust edge cases
// ══════════════════════════════════════════════════════════════════

#[test]
fn rust_scoped_call_resolved() {
    let source = r#"
fn process() {
    let result = String::from("hello");
    let v = Vec::new();
    self.logger.info("test");
}
"#;
    let parsed = parse_single("test.rs", source, "rust").unwrap();
    let callees: Vec<&str> = parsed.call_sites.iter().map(|c| c.callee.as_str()).collect();
    assert!(callees.contains(&"from"), "String::from should resolve to 'from'");
    assert!(callees.contains(&"new"), "Vec::new should resolve to 'new'");
    assert!(callees.contains(&"info"), "self.logger.info should resolve to 'info'");
    for cs in &parsed.call_sites {
        assert!(!cs.callee.contains("::"), "scoped identifier not resolved: '{}'", cs.callee);
        assert!(!cs.callee.contains('.'), "field expression not resolved: '{}'", cs.callee);
    }
}

#[test]
fn rust_trait_and_impl() {
    let source = r#"
pub trait Handler {
    fn handle(&self, req: Request) -> Response;
}

pub struct MyHandler;

impl Handler for MyHandler {
    fn handle(&self, req: Request) -> Response {
        self.process(req)
    }
}

impl MyHandler {
    fn process(&self, req: Request) -> Response {
        Response::ok()
    }
}
"#;
    let parsed = parse_single("test.rs", source, "rust").unwrap();
    let names: Vec<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Handler"), "missing trait Handler");
    assert!(names.contains(&"MyHandler"), "missing struct MyHandler");
    assert!(names.contains(&"handle"), "missing impl method handle");
    assert!(names.contains(&"process"), "missing impl method process");

    let trait_sym = parsed.symbols.iter().find(|s| s.name == "Handler").unwrap();
    assert!(matches!(trait_sym.kind, SymbolKind::Interface));

    let methods: Vec<_> = parsed.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Method)).collect();
    assert!(methods.len() >= 2, "should have at least 2 methods (handle, process)");
}

// ══════════════════════════════════════════════════════════════════
// FTS query conversion (unit-level)
// ══════════════════════════════════════════════════════════════════

#[test]
fn fts_query_conversion_logic() {
    // Simulates the MCP handler's query conversion
    let convert = |raw: &str| -> String {
        raw.split(';')
            .flat_map(|part| part.split_whitespace())
            .filter(|w| w.len() > 1)
            .map(|w| format!("{w}*"))
            .collect::<Vec<_>>()
            .join(" ")
    };

    assert_eq!(convert("createWallet"), "createWallet*");
    assert_eq!(convert("create wallet"), "create* wallet*");
    assert_eq!(convert("auth; validate token"), "auth* validate* token*");
    assert_eq!(convert("a b"), "", "single-char words filtered out");
    assert_eq!(convert("handleCreate; getBalance"), "handleCreate* getBalance*");
}

// ══════════════════════════════════════════════════════════════════
// Unsupported language
// ══════════════════════════════════════════════════════════════════

#[test]
fn unsupported_language_returns_error() {
    let result = parse_single("test.xyz", "some content", "cobol");
    assert!(result.is_err());
}
