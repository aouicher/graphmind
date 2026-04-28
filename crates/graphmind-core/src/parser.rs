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
        "c" => tree_sitter_c::LANGUAGE.into(),
        "objc" => tree_sitter_objc::LANGUAGE.into(),
        "java" => tree_sitter_java::LANGUAGE.into(),
        "php" => tree_sitter_php::LANGUAGE_PHP.into(),
        "swift" => tree_sitter_swift::LANGUAGE.into(),
        "bash" => tree_sitter_bash::LANGUAGE.into(),
        "perl" => tree_sitter_perl_next::LANGUAGE.into(),
        "css" => tree_sitter_css::LANGUAGE.into(),
        "scss" => tree_sitter_scss::language(),
        "html" => tree_sitter_html::LANGUAGE.into(),
        "toml" => tree_sitter_toml_ng::language(),
        "dockerfile" => tree_sitter_containerfile::LANGUAGE.into(),
        "sql" => tree_sitter_sql::LANGUAGE.into(),
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
        "c" => (
            languages::c::extract_symbols(root, source),
            languages::c::extract_call_sites(root, source),
            languages::c::extract_imports(root, source, path),
        ),
        "objc" => (
            languages::objc::extract_symbols(root, source),
            languages::objc::extract_call_sites(root, source),
            languages::objc::extract_imports(root, source, path),
        ),
        "java" => (
            languages::java::extract_symbols(root, source),
            languages::java::extract_call_sites(root, source),
            languages::java::extract_imports(root, source, path),
        ),
        "php" => (
            languages::php::extract_symbols(root, source),
            languages::php::extract_call_sites(root, source),
            languages::php::extract_imports(root, source, path),
        ),
        "swift" => (
            languages::swift::extract_symbols(root, source),
            languages::swift::extract_call_sites(root, source),
            languages::swift::extract_imports(root, source, path),
        ),
        "bash" => (
            languages::bash::extract_symbols(root, source),
            languages::bash::extract_call_sites(root, source),
            languages::bash::extract_imports(root, source, path),
        ),
        "perl" => (
            languages::perl::extract_symbols(root, source),
            languages::perl::extract_call_sites(root, source),
            languages::perl::extract_imports(root, source, path),
        ),
        "css" => (
            languages::css::extract_symbols(root, source),
            languages::css::extract_call_sites(root, source),
            languages::css::extract_imports(root, source, path),
        ),
        "scss" => (
            languages::scss::extract_symbols(root, source),
            languages::scss::extract_call_sites(root, source),
            languages::scss::extract_imports(root, source, path),
        ),
        "html" => (
            languages::html::extract_symbols(root, source),
            languages::html::extract_call_sites(root, source),
            languages::html::extract_imports(root, source, path),
        ),
        "toml" => (
            languages::toml::extract_symbols(root, source),
            languages::toml::extract_call_sites(root, source),
            languages::toml::extract_imports(root, source, path),
        ),
        "dockerfile" => (
            languages::dockerfile::extract_symbols(root, source),
            languages::dockerfile::extract_call_sites(root, source),
            languages::dockerfile::extract_imports(root, source, path),
        ),
        "sql" => (
            languages::sql::extract_symbols(root, source),
            languages::sql::extract_call_sites(root, source),
            languages::sql::extract_imports(root, source, path),
        ),
        _ => unreachable!(),
    };

    Ok(ParseResult {
        symbols,
        call_sites,
        imports,
    })
}
