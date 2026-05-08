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

fn first_identifier_child(node: Node, source: &str) -> Option<String> {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "identifier" {
                return Some(node_text(child, source));
            }
        }
    }
    None
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "class_interface" | "class_implementation" => {
            if let Some(name) = first_identifier_child(node, source) {
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
        "protocol_declaration" => {
            if let Some(name) = first_identifier_child(node, source) {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Interface,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
        "method_definition" | "method_declaration" => {
            if let Some(name) = extract_method_selector(node, source) {
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Method,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(name),
                    doc: None,
                });
            }
        }
        "function_definition" => {
            if let Some(decl) = node.child_by_field_name("declarator") {
                symbols.push(Symbol {
                    name: node_text(decl, source),
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

fn extract_method_selector(node: Node, source: &str) -> Option<String> {
    if let Some(sel) = node.child_by_field_name("selector") {
        return Some(node_text(sel, source));
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "identifier" {
                return Some(node_text(child, source));
            }
            if child.kind() == "keyword_selector" {
                return Some(node_text(child, source));
            }
        }
    }
    None
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = match node.kind() {
        "method_definition" => extract_method_selector(node, source),
        "function_definition" => node.child_by_field_name("declarator").map(|n| node_text(n, source)),
        _ => None,
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "message_expression" {
        if let Some(sel) = node.child_by_field_name("selector").or_else(|| {
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    if child.kind() == "identifier" || child.kind() == "keyword_selector" {
                        return Some(child);
                    }
                }
            }
            None
        }) {
            let callee = node_text(sel, source);
            let kind = crate::extractor::classify_call_kind(&callee).to_string();
            if let Some(caller) = active_fn {
                sites.push(CallSite {
                    caller: caller.to_string(),
                    callee,
                    receiver: None,
                    line: node.start_position().row as u32 + 1,
                    kind,
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
        if let Some(path_node) = node.child_by_field_name("path").or_else(|| {
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    if child.kind() == "string_literal" || child.kind() == "system_lib_string" {
                        return Some(child);
                    }
                }
            }
            None
        }) {
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
