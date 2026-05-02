use crate::extractor::{CallSite, Symbol, SymbolKind};
use crate::resolver::ResolvedImport;
use tree_sitter::Node;

pub fn extract_symbols(root: Node, source: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    collect_symbols(root, source, &mut symbols);
    symbols
}

pub fn extract_call_sites(root: Node, source: &str) -> Vec<CallSite> {
    let mut sites = Vec::new();
    collect_call_sites(root, source, &mut sites, None);
    sites
}

pub fn extract_imports(_root: Node, _source: &str, _file_path: &str) -> Vec<ResolvedImport> {
    Vec::new()
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "function_statement" => {
            if let Some(name) = find_child_by_kind(node, "function_name", source) {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
        "class_statement" => {
            if let Some(name) = find_child_by_kind(node, "class_name", source)
                .or_else(|| node.child_by_field_name("name").map(|n| node_text(n, source)))
            {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Class,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
        "enum_statement" => {
            if let Some(name) = find_child_by_kind(node, "enum_name", source)
                .or_else(|| node.child_by_field_name("name").map(|n| node_text(n, source)))
            {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Type,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
        _ => {}
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, symbols);
        }
    }
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = if node.kind() == "function_statement" {
        find_child_by_kind(node, "function_name", source)
    } else {
        None
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "command" {
        if let Some(name_node) = node.child_by_field_name("name").or_else(|| node.named_child(0)) {
            let callee = node_text(name_node, source);
            if let Some(caller) = active_fn {
                sites.push(CallSite {
                    caller: caller.to_string(),
                    callee,
                    receiver: None,
                    line: node.start_position().row as u32 + 1,
                });
            }
        }
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_call_sites(child, source, sites, active_fn);
        }
    }
}

fn find_child_by_kind(node: Node, kind: &str, source: &str) -> Option<String> {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == kind {
                return Some(node_text(child, source));
            }
        }
    }
    None
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
