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
    pub receiver: Option<String>,
    pub line: u32,
    pub kind: String,
}

pub fn run_collect<F>(root: Node, source: &str, collect: F) -> Vec<Symbol>
where
    F: Fn(Node, &str, &mut Vec<Symbol>),
{
    let mut symbols = Vec::new();
    collect(root, source, &mut symbols);
    symbols
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
        "export_statement" => {
            if let Some(child) = find_arrow_function(node, source) {
                symbols.push(child);
                // Recurse into the arrow function body directly, skipping the
                // lexical_declaration wrapper to avoid duplicate detection
                for i in 0..node.named_child_count() {
                    if let Some(c) = node.named_child(i) {
                        if c.kind() == "lexical_declaration" {
                            // Find the arrow body and recurse into it
                            for j in 0..c.named_child_count() {
                                if let Some(decl) = c.named_child(j) {
                                    if decl.kind() == "variable_declarator" {
                                        if let Some(val) = decl.child_by_field_name("value") {
                                            if val.kind() == "arrow_function" {
                                                if let Some(body) = val.child_by_field_name("body") {
                                                    for k in 0..body.named_child_count() {
                                                        if let Some(stmt) = body.named_child(k) {
                                                            collect_symbols(stmt, source, symbols);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                return;
            }
            // export const obj = { method() {} } — extract object literal methods
            let before = symbols.len();
            collect_object_literal_methods(node, source, symbols, None);
            if symbols.len() > before {
                return;
            }
        }
        "lexical_declaration" => {
            if let Some(child) = find_arrow_function(node, source) {
                symbols.push(child);
                return;
            }
            // const obj = { method() {} } — extract object literal methods
            let before = symbols.len();
            collect_object_literal_methods(node, source, symbols, None);
            if symbols.len() > before {
                return;
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

/// Extract methods from `export const obj = { method() {}, arrow: () => {} }`
/// container_name: if provided, names symbols as "container.method"
fn collect_object_literal_methods(node: Node, source: &str, symbols: &mut Vec<Symbol>, container_name: Option<&str>) {
    for i in 0..node.named_child_count() {
        let child = match node.named_child(i) {
            Some(c) => c,
            None => continue,
        };
        match child.kind() {
            // Traverse wrappers to reach variable_declarator
            "lexical_declaration" | "export_statement" => {
                collect_object_literal_methods(child, source, symbols, container_name);
            }
            "variable_declarator" => {
                // Determine container name from the variable name
                let var_name = child.child_by_field_name("name")
                    .map(|n| node_text(n, source));
                let value = child.child_by_field_name("value");
                if let Some(val) = value {
                    if val.kind() == "object" {
                        let cname = var_name.as_deref().or(container_name);
                        collect_object_literal_methods(val, source, symbols, cname);
                    }
                }
            }
            "pair" => {
                // { key: function() {} } or { key: () => {} }
                let key = child.child_by_field_name("key")
                    .map(|n| node_text(n, source));
                let val = child.child_by_field_name("value");
                if let (Some(key_name), Some(val_node)) = (key, val) {
                    let sym_name = if let Some(c) = container_name {
                        format!("{c}.{key_name}")
                    } else {
                        key_name.clone()
                    };
                    match val_node.kind() {
                        "arrow_function" | "function" | "function_expression" => {
                            symbols.push(Symbol {
                                name: sym_name,
                                kind: SymbolKind::Method,
                                line_start: child.start_position().row as u32 + 1,
                                line_end: child.end_position().row as u32 + 1,
                                signature: Some(build_signature(val_node, source)),
                                doc: extract_doc_comment(child, source),
                            });
                        }
                        _ => {}
                    }
                }
            }
            "method_definition" => {
                // { async method() {} }
                if let Some(name_node) = child.child_by_field_name("name") {
                    let method_name = node_text(name_node, source);
                    let sym_name = if let Some(c) = container_name {
                        format!("{c}.{method_name}")
                    } else {
                        method_name
                    };
                    symbols.push(Symbol {
                        name: sym_name,
                        kind: SymbolKind::Method,
                        line_start: child.start_position().row as u32 + 1,
                        line_end: child.end_position().row as u32 + 1,
                        signature: Some(build_signature(child, source)),
                        doc: extract_doc_comment(child, source),
                    });
                }
            }
            _ => {}
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
            let (callee, receiver) = if func.kind() == "member_expression" {
                let method = func.child_by_field_name("property")
                    .map(|p| node_text(p, source))
                    .unwrap_or_else(|| node_text(func, source));
                let recv = func.child_by_field_name("object")
                    .map(|o| extract_receiver_name(o, source));
                (method, recv)
            } else {
                (node_text(func, source), None)
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

pub fn extract_receiver_name(node: Node, source: &str) -> String {
    match node.kind() {
        "identifier" | "constant" | "scope_resolution" => node_text(node, source),
        "member_expression" | "attribute" | "selector_expression" | "field_expression" => {
            node.child_by_field_name("property")
                .or_else(|| node.child_by_field_name("attribute"))
                .or_else(|| node.child_by_field_name("field"))
                .map(|n| node_text(n, source))
                .unwrap_or_else(|| node_text(node, source))
        }
        "call_expression" | "call" => {
            node.child_by_field_name("function")
                .or_else(|| node.child_by_field_name("method"))
                .map(|n| node_text(n, source))
                .unwrap_or_else(|| node_text(node, source))
        }
        _ => {
            let text = node_text(node, source);
            if text.len() > 40 { text[..40].to_string() } else { text }
        }
    }
}

fn node_text(node: Node, source: &str) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    source[start..end].to_string()
}

#[cfg(test)]
mod object_literal_tests {
    use crate::parser::parse;

    #[test]
    fn extracts_object_literal_methods() {
        let source = r#"export const authService = {
  async refreshToken(token: string): Promise<void> {
    return;
  },
  async authenticate(email: string): Promise<void> {
    return;
  },
};"#;
        let result = parse("test.ts", source, "typescript").unwrap();
        let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("refreshToken")), "refreshToken not found in {:?}", names);
        assert!(names.iter().any(|n| n.contains("authenticate")), "authenticate not found in {:?}", names);
        assert_eq!(names.iter().filter(|n| n.contains("refreshToken")).count(), 1, "duplicate refreshToken");
    }
}
