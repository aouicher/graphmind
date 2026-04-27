#[cfg(feature = "napi")]
use napi_derive::napi;

use tree_sitter::Node;

#[cfg_attr(feature = "napi", napi(string_enum))]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SymbolKind {
    Function,
    Class,
    Method,
    Interface,
    Type,
}

#[cfg_attr(feature = "napi", napi(object))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line_start: u32,
    pub line_end: u32,
    pub signature: Option<String>,
    pub doc: Option<String>,
}

#[cfg_attr(feature = "napi", napi(object))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallSite {
    pub caller: String,
    pub callee: String,
    pub line: u32,
}

pub fn extract_symbols(root: Node, source: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    collect_symbols(root, source, &mut symbols);
    symbols
}

pub fn extract_call_sites(root: Node, source: &str) -> Vec<CallSite> {
    let mut call_sites = Vec::new();
    collect_call_sites(root, source, &mut call_sites, None);
    call_sites
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "function_declaration" | "function" => {
            if let Some(sym) = extract_function(node, source) {
                symbols.push(sym);
            }
        }
        "class_declaration" => {
            if let Some(sym) = extract_class(node, source) {
                symbols.push(sym);
            }
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    if child.kind() == "class_body" {
                        collect_class_members(child, source, symbols);
                    }
                }
            }
        }
        "interface_declaration" => {
            if let Some(sym) = extract_interface(node, source) {
                symbols.push(sym);
            }
        }
        "type_alias_declaration" => {
            if let Some(sym) = extract_type_alias(node, source) {
                symbols.push(sym);
            }
        }
        "export_statement" | "lexical_declaration" => {
            if let Some(child) = find_arrow_function(node, source) {
                symbols.push(child);
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

fn extract_function(node: Node, source: &str) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source);
    let sig = build_signature(node, source);
    let doc = extract_doc_comment(node, source);

    Some(Symbol {
        name,
        kind: SymbolKind::Function,
        line_start: node.start_position().row as u32 + 1,
        line_end: node.end_position().row as u32 + 1,
        signature: Some(sig),
        doc,
    })
}

fn extract_class(node: Node, source: &str) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source);

    Some(Symbol {
        name,
        kind: SymbolKind::Class,
        line_start: node.start_position().row as u32 + 1,
        line_end: node.end_position().row as u32 + 1,
        signature: None,
        doc: extract_doc_comment(node, source),
    })
}

fn extract_interface(node: Node, source: &str) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source);

    Some(Symbol {
        name,
        kind: SymbolKind::Interface,
        line_start: node.start_position().row as u32 + 1,
        line_end: node.end_position().row as u32 + 1,
        signature: None,
        doc: extract_doc_comment(node, source),
    })
}

fn extract_type_alias(node: Node, source: &str) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source);

    Some(Symbol {
        name,
        kind: SymbolKind::Type,
        line_start: node.start_position().row as u32 + 1,
        line_end: node.end_position().row as u32 + 1,
        signature: None,
        doc: extract_doc_comment(node, source),
    })
}

fn find_arrow_function(node: Node, source: &str) -> Option<Symbol> {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i)?;
        if child.kind() == "lexical_declaration" {
            return find_arrow_function(child, source);
        }
        if child.kind() == "variable_declarator" {
            let name_node = child.child_by_field_name("name")?;
            let value = child.child_by_field_name("value")?;
            if value.kind() == "arrow_function" {
                let name = node_text(name_node, source);
                return Some(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line_start: child.start_position().row as u32 + 1,
                    line_end: child.end_position().row as u32 + 1,
                    signature: Some(build_signature(value, source)),
                    doc: extract_doc_comment(node, source),
                });
            }
        }
    }
    None
}

fn collect_class_members(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "method_definition" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Method,
                        line_start: child.start_position().row as u32 + 1,
                        line_end: child.end_position().row as u32 + 1,
                        signature: Some(build_signature(child, source)),
                        doc: extract_doc_comment(child, source),
                    });
                }
            }
        }
    }
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = match node.kind() {
        "function_declaration" | "method_definition" => {
            node.child_by_field_name("name").map(|n| node_text(n, source))
        }
        _ => None,
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            let callee = if func.kind() == "member_expression" {
                func.child_by_field_name("property")
                    .map(|p| node_text(p, source))
                    .unwrap_or_else(|| node_text(func, source))
            } else {
                node_text(func, source)
            };
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

fn build_signature(node: Node, source: &str) -> String {
    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = node_text(params, source);
        let ret = node
            .child_by_field_name("return_type")
            .map(|n| format!(": {}", node_text(n, source)))
            .unwrap_or_default();
        format!("{params_text}{ret}")
    } else {
        String::new()
    }
}

fn extract_doc_comment(node: Node, source: &str) -> Option<String> {
    let prev = node.prev_sibling()?;
    if prev.kind() == "comment" {
        let text = node_text(prev, source);
        if text.starts_with("/**") {
            return Some(text);
        }
    }
    None
}

fn node_text(node: Node, source: &str) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    source[start..end].to_string()
}
