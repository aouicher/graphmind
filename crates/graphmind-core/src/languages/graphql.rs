use crate::extractor::{CallSite, Symbol, SymbolKind};
use crate::resolver::ResolvedImport;
use tree_sitter::Node;

pub fn extract_symbols(root: Node, source: &str) -> Vec<Symbol> {
    crate::extractor::run_collect(root, source, collect_symbols)
}

pub fn extract_call_sites(_root: Node, _source: &str) -> Vec<CallSite> {
    Vec::new()
}

pub fn extract_imports(_root: Node, _source: &str, _file_path: &str) -> Vec<ResolvedImport> {
    Vec::new()
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "type_system_definition" | "executable_definition" => {
            extract_graphql_definition(node, source, symbols);
        }
        _ => {}
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, symbols);
        }
    }
}

fn extract_graphql_definition(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    let text = node_text(node, source);
    let kind = if text.starts_with("type ") || text.starts_with("input ") {
        Some(SymbolKind::Type)
    } else if text.starts_with("interface ") {
        Some(SymbolKind::Interface)
    } else if text.starts_with("enum ") || text.starts_with("scalar ") || text.starts_with("union ") {
        Some(SymbolKind::Type)
    } else if text.starts_with("query ") || text.starts_with("mutation ") || text.starts_with("subscription ") || text.starts_with("fragment ") {
        Some(SymbolKind::Function)
    } else {
        None
    };

    if let Some(sym_kind) = kind {
        let name = text.split_whitespace().nth(1)
            .map(|n| n.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_'))
            .unwrap_or("")
            .to_string();
        if !name.is_empty() {
            symbols.push(Symbol {
                name,
                kind: sym_kind,
                line_start: node.start_position().row as u32 + 1,
                line_end: node.end_position().row as u32 + 1,
                signature: None,
                doc: None,
            });
        }
    }
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
