#![allow(dead_code)]

use crate::extractor::{CallSite, Symbol};
use tree_sitter::Node;

pub trait LanguageParser: Send + Sync {
    fn extract_symbols(&self, root: Node, source: &str) -> Vec<Symbol>;
    fn extract_call_sites(&self, root: Node, source: &str) -> Vec<CallSite>;
    fn tree_sitter_language(&self) -> tree_sitter::Language;
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

pub struct RustParser;

impl LanguageParser for RustParser {
    fn extract_symbols(&self, root: Node, source: &str) -> Vec<Symbol> {
        crate::languages::rust::extract_symbols(root, source)
    }
    fn extract_call_sites(&self, root: Node, source: &str) -> Vec<CallSite> {
        crate::languages::rust::extract_call_sites(root, source)
    }
    fn tree_sitter_language(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

pub struct PythonParser;

impl LanguageParser for PythonParser {
    fn extract_symbols(&self, root: Node, source: &str) -> Vec<Symbol> {
        crate::languages::python::extract_symbols(root, source)
    }
    fn extract_call_sites(&self, root: Node, source: &str) -> Vec<CallSite> {
        crate::languages::python::extract_call_sites(root, source)
    }
    fn tree_sitter_language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }
}

// ---------------------------------------------------------------------------
// TypeScript
// ---------------------------------------------------------------------------

pub struct TypeScriptParser;

impl LanguageParser for TypeScriptParser {
    fn extract_symbols(&self, root: Node, source: &str) -> Vec<Symbol> {
        crate::extractor::extract_symbols(root, source)
    }
    fn extract_call_sites(&self, root: Node, source: &str) -> Vec<CallSite> {
        crate::extractor::extract_call_sites(root, source)
    }
    fn tree_sitter_language(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

pub struct GoParser;

impl LanguageParser for GoParser {
    fn extract_symbols(&self, root: Node, source: &str) -> Vec<Symbol> {
        crate::languages::go::extract_symbols(root, source)
    }
    fn extract_call_sites(&self, root: Node, source: &str) -> Vec<CallSite> {
        crate::languages::go::extract_call_sites(root, source)
    }
    fn tree_sitter_language(&self) -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()
    }
}

// ---------------------------------------------------------------------------
// Dart
// ---------------------------------------------------------------------------

pub struct DartParser;

impl LanguageParser for DartParser {
    fn extract_symbols(&self, root: Node, source: &str) -> Vec<Symbol> {
        crate::languages::dart::extract_symbols(root, source)
    }
    fn extract_call_sites(&self, root: Node, source: &str) -> Vec<CallSite> {
        crate::languages::dart::extract_call_sites(root, source)
    }
    fn tree_sitter_language(&self) -> tree_sitter::Language {
        tree_sitter_dart::LANGUAGE.into()
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub fn get_parser(language: &str) -> Option<&'static dyn LanguageParser> {
    static RUST: RustParser = RustParser;
    static PYTHON: PythonParser = PythonParser;
    static TYPESCRIPT: TypeScriptParser = TypeScriptParser;
    static GO: GoParser = GoParser;
    static DART: DartParser = DartParser;

    match language {
        "rust" => Some(&RUST),
        "python" => Some(&PYTHON),
        "typescript" | "tsx" => Some(&TYPESCRIPT),
        "go" => Some(&GO),
        "dart" => Some(&DART),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_with_registry(language: &str, source: &str) -> Vec<Symbol> {
        let parser_impl = get_parser(language).expect("parser not registered");
        let ts_lang = parser_impl.tree_sitter_language();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&ts_lang).expect("set language");
        let tree = parser.parse(source, None).expect("parse");
        parser_impl.extract_symbols(tree.root_node(), source)
    }

    #[test]
    fn rust_parser_registered_and_extracts_symbols() {
        let source = "fn hello() -> u32 { 42 }";
        let symbols = parse_with_registry("rust", source);
        assert!(!symbols.is_empty(), "rust parser returned no symbols");
        assert!(symbols.iter().any(|s| s.name == "hello"), "expected 'hello' symbol");
    }

    #[test]
    fn python_parser_registered_and_extracts_symbols() {
        let source = "def greet(name):\n    return name\n";
        let symbols = parse_with_registry("python", source);
        assert!(!symbols.is_empty(), "python parser returned no symbols");
        assert!(symbols.iter().any(|s| s.name == "greet"), "expected 'greet' symbol");
    }
}
