#[cfg(feature = "napi")]
use napi_derive::napi;

mod extractor;
mod languages;
mod parser;
mod registry;
mod resolver;

pub use extractor::{CallSite, Symbol, SymbolKind};
pub use parser::{parse, ParseResult};
pub use registry::LanguageParser;
pub use resolver::ResolvedImport;

#[cfg_attr(feature = "napi", napi(object))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedFile {
    pub path: String,
    pub language: String,
    pub symbols: Vec<Symbol>,
    pub call_sites: Vec<CallSite>,
    pub imports: Vec<ResolvedImport>,
}

#[cfg(feature = "napi")]
#[napi]
pub fn parse_file(path: String, source: String, language: String) -> napi::Result<ParsedFile> {
    let result = parser::parse(&path, &source, &language)
        .map_err(|e| napi::Error::from_reason(format!("Parse error: {e}")))?;

    Ok(ParsedFile {
        path,
        language,
        symbols: result.symbols,
        call_sites: result.call_sites,
        imports: result.imports,
    })
}

#[cfg(feature = "napi")]
#[napi]
pub fn parse_files(files: Vec<FileInput>) -> napi::Result<Vec<ParsedFile>> {
    use rayon::prelude::*;

    let results: Vec<Result<ParsedFile, String>> = files
        .into_par_iter()
        .map(|file| {
            let result = parser::parse(&file.path, &file.source, &file.language)
                .map_err(|e| format!("Error parsing {}: {e}", file.path))?;
            Ok(ParsedFile {
                path: file.path,
                language: file.language,
                symbols: result.symbols,
                call_sites: result.call_sites,
                imports: result.imports,
            })
        })
        .collect();

    let mut parsed = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(p) => parsed.push(p),
            Err(e) => return Err(napi::Error::from_reason(e)),
        }
    }
    Ok(parsed)
}

#[cfg_attr(feature = "napi", napi(object))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileInput {
    pub path: String,
    pub source: String,
    pub language: String,
}

pub fn parse_single(path: &str, source: &str, language: &str) -> Result<ParsedFile, String> {
    let result = parser::parse(path, source, language)?;
    Ok(ParsedFile {
        path: path.to_string(),
        language: language.to_string(),
        symbols: result.symbols,
        call_sites: result.call_sites,
        imports: result.imports,
    })
}

pub fn parse_batch(files: Vec<FileInput>) -> Vec<Result<ParsedFile, String>> {
    use rayon::prelude::*;

    files
        .into_par_iter()
        .map(|file| {
            let result = parser::parse(&file.path, &file.source, &file.language)
                .map_err(|e| format!("Error parsing {}: {e}", file.path))?;
            Ok(ParsedFile {
                path: file.path,
                language: file.language,
                symbols: result.symbols,
                call_sites: result.call_sites,
                imports: result.imports,
            })
        })
        .collect()
}

#[cfg(feature = "napi")]
#[napi]
pub fn supported_languages() -> Vec<String> {
    languages_list()
}

pub fn languages_list() -> Vec<String> {
    vec![
        "typescript".to_string(),
        "javascript".to_string(),
        "python".to_string(),
        "go".to_string(),
        "rust".to_string(),
        "ruby".to_string(),
        "hcl".to_string(),
        "yaml".to_string(),
        "c".to_string(),
        "objc".to_string(),
        "java".to_string(),
        "php".to_string(),
        "swift".to_string(),
        "bash".to_string(),
        "perl".to_string(),
        "css".to_string(),
        "scss".to_string(),
        "html".to_string(),
        "toml".to_string(),
        "dockerfile".to_string(),
        "sql".to_string(),
        "cpp".to_string(),
        "csharp".to_string(),
        "kotlin".to_string(),
        "dart".to_string(),
        "scala".to_string(),
        "r".to_string(),
        "graphql".to_string(),
        "powershell".to_string(),
    ]
}
