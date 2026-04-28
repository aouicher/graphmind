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

pub fn extract_imports(root: Node, source: &str, _file_path: &str) -> Vec<ResolvedImport> {
    let mut imports = Vec::new();
    collect_imports(root, source, &mut imports);
    imports
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "class_interface" | "class_implementation" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Class,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
        "protocol_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Interface,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
        "method_definition" | "method_declaration" => {
            if let Some(sel) = node.child_by_field_name("selector") {
                symbols.push(Symbol {
                    name: node_text(sel, source),
                    kind: SymbolKind::Method,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(node_text(sel, source)),
                    doc: None,
                });
            }
        }
        "function_definition" => {
            if let Some(decl) = node.child_by_field_name("declarator") {
                let name = node_text(decl, source);
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
        _ => {}
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, symbols);
        }
    }
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = match node.kind() {
        "method_definition" => node.child_by_field_name("selector").map(|n| node_text(n, source)),
        "function_definition" => node.child_by_field_name("declarator").map(|n| node_text(n, source)),
        _ => None,
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "message_expression" {
        if let Some(sel) = node.child_by_field_name("selector") {
            let callee = node_text(sel, source);
            if let Some(caller) = active_fn {
                sites.push(CallSite {
                    caller: caller.to_string(),
                    callee,
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

fn collect_imports(node: Node, source: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "preproc_import" || node.kind() == "preproc_include" {
        if let Some(path_node) = node.child_by_field_name("path") {
            let path = node_text(path_node, source)
                .trim_matches('"')
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
            imports.push(ResolvedImport {
                source: path,
                specifiers: Vec::new(),
                line: node.start_position().row as u32 + 1,
                is_default: false,
                from_file: String::new(),
            });
        }
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
