use crate::extractor::{CallSite, Symbol, SymbolKind};
use crate::resolver::ResolvedImport;
use tree_sitter::Node;

pub fn extract_symbols(root: Node, source: &str) -> Vec<Symbol> {
    crate::extractor::run_collect(root, source, collect_symbols)
}

pub fn extract_call_sites(root: Node, source: &str) -> Vec<CallSite> {
    let mut sites = Vec::new();
    collect_call_sites(root, source, &mut sites, None);
    sites
}

pub fn extract_imports(root: Node, source: &str, _file_path: &str) -> Vec<ResolvedImport> {
    let mut imports = Vec::new();
    collect_imports(root, source, &mut imports);
    imports
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "function_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let sig = node.child_by_field_name("parameters")
                    .map(|p| node_text(p, source))
                    .unwrap_or_default();
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Function,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(sig),
                    doc: extract_phpdoc(node, source),
                });
            }
        }
        "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Class,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: extract_phpdoc(node, source),
                });
            }
        }
        "interface_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Interface,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: extract_phpdoc(node, source),
                });
            }
        }
        "method_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let sig = node.child_by_field_name("parameters")
                    .map(|p| node_text(p, source))
                    .unwrap_or_default();
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Method,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(sig),
                    doc: extract_phpdoc(node, source),
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

fn extract_phpdoc(node: Node, source: &str) -> Option<String> {
    let prev = node.prev_sibling()?;
    if prev.kind() == "comment" {
        let text = node_text(prev, source);
        if text.starts_with("/**") {
            return Some(text);
        }
    }
    None
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = match node.kind() {
        "function_definition" | "method_declaration" => {
            node.child_by_field_name("name").map(|n| node_text(n, source))
        }
        _ => None,
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "function_call_expression" || node.kind() == "member_call_expression" {
        if let Some(func) = node.child_by_field_name("function").or_else(|| node.child_by_field_name("name")) {
            let callee = node_text(func, source);
            let receiver = if node.kind() == "member_call_expression" {
                node.child_by_field_name("object")
                    .map(|o| crate::extractor::extract_receiver_name(o, source))
            } else {
                None
            };
            if let Some(caller) = active_fn {
                sites.push(CallSite {
                    caller: caller.to_string(),
                    callee,
                    receiver,
                    line: node.start_position().row as u32 + 1,
                    kind: "calls".to_string(),
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

fn collect_imports(node: Node, source: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "namespace_use_declaration" {
        let text = node_text(node, source);
        let path = text.trim_start_matches("use ")
            .trim_end_matches(';')
            .trim()
            .to_string();
        let spec = path.rsplit('\\').next().unwrap_or(&path).to_string();
        imports.push(ResolvedImport {
            source: path,
            specifiers: vec![spec],
            line: node.start_position().row as u32 + 1,
            is_default: false,
            from_file: String::new(),
        });
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_imports(child, source, imports);
        }
    }
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
