use crate::extractor::{self, CallSite, Symbol};
use crate::resolver::{self, ResolvedImport};

pub struct ParseResult {
    pub symbols: Vec<Symbol>,
    pub call_sites: Vec<CallSite>,
    pub imports: Vec<ResolvedImport>,
}

pub fn parse(path: &str, source: &str, language: &str) -> Result<ParseResult, String> {
    let mut parser = tree_sitter::Parser::new();

    let ts_language = match language {
        "typescript" | "tsx" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        "javascript" | "jsx" | "mjs" => tree_sitter_javascript::LANGUAGE,
        _ => return Err(format!("Unsupported language: {language}")),
    };

    parser
        .set_language(&ts_language.into())
        .map_err(|e| format!("Failed to set language: {e}"))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "Failed to parse source".to_string())?;

    let root = tree.root_node();
    let symbols = extractor::extract_symbols(root, source);
    let call_sites = extractor::extract_call_sites(root, source);
    let imports = resolver::extract_imports(root, source, path);

    Ok(ParseResult {
        symbols,
        call_sites,
        imports,
    })
}
