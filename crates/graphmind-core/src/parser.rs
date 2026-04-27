use crate::extractor::{self, CallSite, Symbol};
use crate::languages;
use crate::resolver::{self, ResolvedImport};

pub struct ParseResult {
    pub symbols: Vec<Symbol>,
    pub call_sites: Vec<CallSite>,
    pub imports: Vec<ResolvedImport>,
}

pub fn parse(path: &str, source: &str, language: &str) -> Result<ParseResult, String> {
    let mut parser = tree_sitter::Parser::new();

    let ts_language: tree_sitter::Language = match language {
        "typescript" | "tsx" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "javascript" | "jsx" | "mjs" => tree_sitter_javascript::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "ruby" => tree_sitter_ruby::LANGUAGE.into(),
        "hcl" | "terraform" => tree_sitter_hcl::LANGUAGE.into(),
        "yaml" => tree_sitter_yaml::LANGUAGE.into(),
        _ => return Err(format!("Unsupported language: {language}")),
    };

    parser
        .set_language(&ts_language)
        .map_err(|e| format!("Failed to set language: {e}"))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "Failed to parse source".to_string())?;

    let root = tree.root_node();

    let (symbols, call_sites, imports) = match language {
        "typescript" | "tsx" | "javascript" | "jsx" | "mjs" => (
            extractor::extract_symbols(root, source),
            extractor::extract_call_sites(root, source),
            resolver::extract_imports(root, source, path),
        ),
        "python" => (
            languages::python::extract_symbols(root, source),
            languages::python::extract_call_sites(root, source),
            languages::python::extract_imports(root, source, path),
        ),
        "go" => (
            languages::go::extract_symbols(root, source),
            languages::go::extract_call_sites(root, source),
            languages::go::extract_imports(root, source, path),
        ),
        "rust" => (
            languages::rust::extract_symbols(root, source),
            languages::rust::extract_call_sites(root, source),
            languages::rust::extract_imports(root, source, path),
        ),
        "ruby" => (
            languages::ruby::extract_symbols(root, source),
            languages::ruby::extract_call_sites(root, source),
            languages::ruby::extract_imports(root, source, path),
        ),
        "hcl" | "terraform" => (
            languages::hcl::extract_symbols(root, source),
            languages::hcl::extract_call_sites(root, source),
            languages::hcl::extract_imports(root, source, path),
        ),
        "yaml" => (
            languages::yaml::extract_symbols(root, source),
            languages::yaml::extract_call_sites(root, source),
            languages::yaml::extract_imports(root, source, path),
        ),
        _ => unreachable!(),
    };

    Ok(ParseResult {
        symbols,
        call_sites,
        imports,
    })
}
