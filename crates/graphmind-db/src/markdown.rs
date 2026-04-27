use regex::Regex;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct MdSymbol {
    pub name: String,
    pub kind: String,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MdImport {
    pub source: String,
    pub specifiers: Vec<String>,
    pub line: usize,
    pub is_default: bool,
    pub from_file: String,
}

#[derive(Debug, Clone)]
pub struct MarkdownParseResult {
    pub path: String,
    pub language: String,
    pub symbols: Vec<MdSymbol>,
    pub imports: Vec<MdImport>,
}

pub fn parse_markdown(path: &str, source: &str) -> MarkdownParseResult {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut seen_links = HashSet::new();

    let mut in_code_block = false;
    let mut code_block_start = 0usize;
    let mut code_block_lang = String::new();
    let mut code_block_name = String::new();

    let header_re = Regex::new(r"^(#{1,6})\s+(.+)").unwrap();
    let link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    let wikilink_re = Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;

        if let Some(after_fence) = line.strip_prefix("```") {
            if !in_code_block {
                in_code_block = true;
                code_block_start = line_num;
                code_block_lang = after_fence.split_whitespace().next().unwrap_or("").to_string();
                let prev = if i > 0 { lines[i - 1] } else { "" };
                code_block_name = header_re
                    .captures(prev)
                    .and_then(|c| c.get(2))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| format!("code-block-L{line_num}"));
            } else {
                in_code_block = false;
                if !code_block_lang.is_empty() {
                    symbols.push(MdSymbol {
                        name: code_block_name.clone(),
                        kind: "function".to_string(),
                        line_start: code_block_start,
                        line_end: line_num,
                        signature: Some(code_block_lang.clone()),
                        doc: None,
                    });
                }
            }
            continue;
        }

        if in_code_block {
            continue;
        }

        if let Some(caps) = header_re.captures(line) {
            let level = caps.get(1).map(|m| m.as_str().len()).unwrap_or(1);
            let title = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("").to_string();
            let mut end = line_num;
            for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
                if let Some(next_caps) = header_re.captures(next_line) {
                    let next_level = next_caps.get(1).map(|m| m.as_str().len()).unwrap_or(1);
                    if next_level <= level {
                        break;
                    }
                }
                end = j + 1;
            }
            symbols.push(MdSymbol {
                name: title,
                kind: if level <= 2 { "class" } else { "type" }.to_string(),
                line_start: line_num,
                line_end: end,
                signature: Some(format!("h{level}")),
                doc: None,
            });
        }

        for caps in link_re.captures_iter(line) {
            let label = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let target = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if seen_links.contains(target) {
                continue;
            }
            seen_links.insert(target.to_string());
            if target.starts_with("http://") || target.starts_with("https://") {
                continue;
            }
            imports.push(MdImport {
                source: target.to_string(),
                specifiers: vec![label.to_string()],
                line: line_num,
                is_default: true,
                from_file: path.to_string(),
            });
        }

        for caps in wikilink_re.captures_iter(line) {
            let target = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if seen_links.contains(target) {
                continue;
            }
            seen_links.insert(target.to_string());
            imports.push(MdImport {
                source: target.to_string(),
                specifiers: vec![target.to_string()],
                line: line_num,
                is_default: true,
                from_file: path.to_string(),
            });
        }
    }

    MarkdownParseResult {
        path: path.to_string(),
        language: "markdown".to_string(),
        symbols,
        imports,
    }
}
