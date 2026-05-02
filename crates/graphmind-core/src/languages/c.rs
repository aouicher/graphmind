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
    collect_includes(root, source, &mut imports);
    imports
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "function_definition" => {
            if let Some(decl) = node.child_by_field_name("declarator") {
                if let Some(name) = extract_declarator_name(decl, source) {
                    let sig = build_c_signature(node, source);
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        line_start: node.start_position().row as u32 + 1,
                        line_end: node.end_position().row as u32 + 1,
                        signature: Some(sig),
                        doc: None,
                    });
                }
            }
        }
        "struct_specifier" | "enum_specifier" | "union_specifier" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
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

fn extract_declarator_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_declarator" => {
            node.child_by_field_name("declarator")
                .and_then(|d| extract_declarator_name(d, source))
        }
        "pointer_declarator" => {
            node.child_by_field_name("declarator")
                .and_then(|d| extract_declarator_name(d, source))
        }
        "identifier" => Some(node_text(node, source)),
        _ => None,
    }
}

fn build_c_signature(node: Node, source: &str) -> String {
    if let Some(decl) = node.child_by_field_name("declarator") {
        let text = node_text(decl, source);
        if text.len() < 200 { text } else { text[..200].to_string() }
    } else {
        String::new()
    }
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = if node.kind() == "function_definition" {
        node.child_by_field_name("declarator")
            .and_then(|d| extract_declarator_name(d, source))
    } else {
        None
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            let callee = node_text(func, source);
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

fn collect_includes(node: Node, source: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "preproc_include" {
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
            collect_includes(child, source, imports);
        }
    }
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
