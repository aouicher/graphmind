#[cfg(feature = "napi")]
use napi_derive::napi;

use std::collections::HashMap;

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

/// Symbols in source order, and the nodes whose calls they own, keyed by node id.
#[derive(Default)]
struct Collected {
    symbols: Vec<Symbol>,
    scopes: HashMap<usize, String>,
}

impl Collected {
    fn push(&mut self, symbol: Symbol) {
        self.symbols.push(symbol);
    }

    /// A symbol that owns the calls made inside `node`.
    fn push_scope(&mut self, node: Node, symbol: Symbol) {
        self.scopes.insert(node.id(), symbol.name.clone());
        self.symbols.push(symbol);
    }

    /// A function held by a variable or an object key owns its calls only at module
    /// level. Elsewhere they stay with the scope around it: a local callback passed
    /// around keeps its function in blast radius, and same-named locals (two
    /// components' `onClick`, two `describe` blocks' `setup`) cannot be mixed up by the
    /// builder, which resolves callers by name. Such a local is a symbol with no callees.
    fn push_held(&mut self, function: Node, symbol: Symbol) {
        if is_module_level(function) {
            self.push_scope(function, symbol);
        } else {
            self.push(symbol);
        }
    }
}

/// True when only declaration wrappers separate `node` from the program root.
fn is_module_level(node: Node) -> bool {
    let mut parent = node.parent();
    while let Some(p) = parent {
        match p.kind() {
            "program" => return true,
            "pair" | "object" | "variable_declarator" | "lexical_declaration" | "export_statement" => {
                parent = p.parent();
            }
            _ => return false,
        }
    }
    false
}

pub fn extract_symbols(root: Node, source: &str) -> Vec<Symbol> {
    extract_symbols_and_call_sites(root, source).0
}

pub fn extract_call_sites(root: Node, source: &str) -> Vec<CallSite> {
    extract_symbols_and_call_sites(root, source).1
}

/// Symbols, and call sites attributed to the symbol that owns the scope they sit in.
pub fn extract_symbols_and_call_sites(root: Node, source: &str) -> (Vec<Symbol>, Vec<CallSite>) {
    let mut collected = Collected::default();
    collect_symbols(root, source, &mut collected);
    let mut call_sites = Vec::new();
    collect_call_sites(root, source, &mut call_sites, None, &collected.scopes);
    (collected.symbols, call_sites)
}

fn collect_symbols(node: Node, source: &str, out: &mut Collected) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(sym) = extract_function(node, source) {
                out.push_scope(node, sym);
            }
        }
        "class_declaration" => {
            if let Some(sym) = extract_class(node, source) {
                out.push(sym);
            }
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    if child.kind() == "class_body" {
                        collect_class_members(child, source, out);
                    }
                }
            }
        }
        "interface_declaration" => {
            if let Some(sym) = extract_interface(node, source) {
                out.push(sym);
            }
        }
        "type_alias_declaration" => {
            if let Some(sym) = extract_type_alias(node, source) {
                out.push(sym);
            }
        }
        "export_statement" | "lexical_declaration" => {
            if let Some((sym, function)) = find_assigned_function(node, source) {
                out.push_held(function, sym);
                // Recurse into the body only, so the declaration is not found twice.
                if let Some(body) = function.child_by_field_name("body") {
                    collect_symbols(body, source, out);
                }
                return;
            }
            // const obj = { method() {} } — extract object literal methods
            let before = out.symbols.len();
            collect_object_literal_methods(node, source, out, None);
            if out.symbols.len() > before {
                return;
            }
        }
        _ => {}
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, out);
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

/// Node kinds of a function used as a value: `() => {}`, `function () {}`, `function* () {}`.
fn is_function_value(node: Node) -> bool {
    matches!(
        node.kind(),
        "arrow_function" | "function_expression" | "generator_function"
    )
}

/// `const name = <function>`: the first such declarator, as a Function symbol, with its function node.
fn find_assigned_function<'t>(node: Node<'t>, source: &str) -> Option<(Symbol, Node<'t>)> {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i)?;
        if child.kind() == "lexical_declaration" {
            return find_assigned_function(child, source);
        }
        if child.kind() == "variable_declarator" {
            let name_node = child.child_by_field_name("name")?;
            let value = child.child_by_field_name("value")?;
            if is_function_value(value) {
                let symbol = Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Function,
                    line_start: child.start_position().row as u32 + 1,
                    line_end: child.end_position().row as u32 + 1,
                    signature: Some(build_signature(value, source)),
                    doc: extract_doc_comment(node, source),
                };
                return Some((symbol, value));
            }
        }
    }
    None
}

fn collect_class_members(node: Node, source: &str, out: &mut Collected) {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "method_definition" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    out.push_scope(child, Symbol {
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
fn collect_object_literal_methods(node: Node, source: &str, out: &mut Collected, container_name: Option<&str>) {
    for i in 0..node.named_child_count() {
        let child = match node.named_child(i) {
            Some(c) => c,
            None => continue,
        };
        match child.kind() {
            // Traverse wrappers to reach variable_declarator
            "lexical_declaration" | "export_statement" => {
                collect_object_literal_methods(child, source, out, container_name);
            }
            "variable_declarator" => {
                // Determine container name from the variable name
                let var_name = child.child_by_field_name("name")
                    .map(|n| node_text(n, source));
                let value = child.child_by_field_name("value");
                if let Some(val) = value {
                    if val.kind() == "object" {
                        let cname = var_name.as_deref().or(container_name);
                        collect_object_literal_methods(val, source, out, cname);
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
                    if is_function_value(val_node) {
                        out.push_held(val_node, Symbol {
                            name: sym_name,
                            kind: SymbolKind::Method,
                            line_start: child.start_position().row as u32 + 1,
                            line_end: child.end_position().row as u32 + 1,
                            signature: Some(build_signature(val_node, source)),
                            doc: extract_doc_comment(child, source),
                        });
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
                    out.push_held(child, Symbol {
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

fn collect_call_sites<'a>(
    node: Node,
    source: &str,
    sites: &mut Vec<CallSite>,
    current_fn: Option<&'a str>,
    scopes: &'a HashMap<usize, String>,
) {
    let active_fn = scopes.get(&node.id()).map(String::as_str).or(current_fn);

    if node.kind() == "new_expression" {
        if let Some(constructor) = node.child_by_field_name("constructor") {
            let callee = node_text(constructor, source);
            if let Some(caller) = active_fn {
                sites.push(CallSite {
                    caller: caller.to_string(),
                    callee,
                    receiver: None,
                    line: node.start_position().row as u32 + 1,
                    kind: "instantiates".to_string(),
                });
            }
        }
    }

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
            let kind = classify_call_kind(&callee);
            if let Some(caller) = active_fn {
                sites.push(CallSite {
                    caller: caller.to_string(),
                    callee,
                    receiver,
                    line: node.start_position().row as u32 + 1,
                    kind: kind.to_string(),
                });
            }
        }
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_call_sites(child, source, sites, active_fn, scopes);
        }
    }
}

pub fn classify_call_kind(callee: &str) -> &'static str {
    match callee {
        "addEventListener" | "on" | "once" | "addListener" | "subscribe" | "observe" => "listens",
        "emit" | "dispatch" | "trigger" | "fire" | "publish" | "send" | "broadcast" => "emits",
        "spawn" | "fork" => "spawns",
        _ => "calls",
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
            if text.len() > 40 { text.chars().take(40).collect() } else { text }
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

#[cfg(test)]
mod generator_tests {
    use crate::extractor::SymbolKind;
    use crate::parser::parse;

    const SOURCE: &str = r#"export function* genFn(n: number): Generator<number> {
  for (let i = 0; i < n; i++) yield helper(i);
}

function* plainGen() {
  yield* genFn(2);
}

export function main(): number {
  return [...genFn(3)].length;
}
"#;

    #[test]
    fn generator_functions_are_symbols_and_callers() {
        for language in ["typescript", "javascript"] {
            let source = if language == "javascript" {
                SOURCE
                    .replace("(n: number): Generator<number>", "(n)")
                    .replace("(): number", "()")
            } else {
                SOURCE.to_string()
            };
            let result = parse("gen.ts", &source, language).unwrap();

            let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
            let gen = result.symbols.iter().find(|s| s.name == "genFn");
            let gen = gen.unwrap_or_else(|| panic!("{language}: genFn missing from {names:?}"));
            assert_eq!(gen.kind, SymbolKind::Function, "{language}");
            assert_eq!((gen.line_start, gen.line_end), (1, 3), "{language}");
            assert!(
                names.contains(&"plainGen"),
                "{language}: plainGen missing from {names:?}"
            );

            let calls: Vec<(&str, &str)> = result
                .call_sites
                .iter()
                .map(|c| (c.caller.as_str(), c.callee.as_str()))
                .collect();
            for expected in [
                ("genFn", "helper"),
                ("plainGen", "genFn"),
                ("main", "genFn"),
            ] {
                assert!(
                    calls.contains(&expected),
                    "{language}: {expected:?} not in {calls:?}"
                );
            }
        }
    }
}
