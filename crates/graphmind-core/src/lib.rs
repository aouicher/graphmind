use napi_derive::napi;

mod extractor;
mod languages;
mod parser;
mod resolver;

pub use extractor::{CallSite, Symbol, SymbolKind};
pub use parser::ParseResult;
pub use resolver::ResolvedImport;

#[napi(object)]
pub struct ParsedFile {
    pub path: String,
    pub language: String,
    pub symbols: Vec<Symbol>,
    pub call_sites: Vec<CallSite>,
    pub imports: Vec<ResolvedImport>,
}

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

#[napi(object)]
pub struct FileInput {
    pub path: String,
    pub source: String,
    pub language: String,
}

#[napi]
pub fn supported_languages() -> Vec<String> {
    vec![
        "typescript".to_string(),
        "javascript".to_string(),
        "python".to_string(),
        "go".to_string(),
        "rust".to_string(),
        "ruby".to_string(),
    ]
}
