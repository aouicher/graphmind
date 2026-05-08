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

pub fn extract_imports(root: Node, source: &str, file_path: &str) -> Vec<ResolvedImport> {
    let mut imports = Vec::new();
    collect_imports(root, source, file_path, &mut imports);
    imports
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "method" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let sig = build_ruby_signature(node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(sig),
                    doc: extract_ruby_comment(node, source),
                });
            }
        }
        "singleton_method" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = format!("self.{}", node_text(name_node, source));
                let sig = build_ruby_signature(node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(sig),
                    doc: extract_ruby_comment(node, source),
                });
            }
        }
        "class" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Class,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: superclass_signature(node, source),
                    doc: extract_ruby_comment(node, source),
                });
            }
        }
        "module" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Interface,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: extract_ruby_comment(node, source),
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

fn collect_call_sites(
    node: Node,
    source: &str,
    sites: &mut Vec<CallSite>,
    current_fn: Option<&str>,
) {
    let fn_name = match node.kind() {
        "method" | "singleton_method" => node.child_by_field_name("name").map(|n| node_text(n, source)),
        _ => None,
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "call" {
        if let Some(method_node) = node.child_by_field_name("method") {
            let callee = node_text(method_node, source);
            let receiver = node.child_by_field_name("receiver")
                .map(|r| crate::extractor::extract_receiver_name(r, source));
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

fn collect_imports(node: Node, source: &str, file_path: &str, imports: &mut Vec<ResolvedImport>) {
    // require "foo" / require_relative "foo"
    if node.kind() == "call" {
        if let Some(method) = node.child_by_field_name("method") {
            let method_name = node_text(method, source);
            if method_name == "require" || method_name == "require_relative" {
                if let Some(args) = node.child_by_field_name("arguments") {
                    if let Some(arg) = args.named_child(0) {
                        let raw = node_text(arg, source);
                        let module = raw.trim_matches('"').trim_matches('\'').to_string();
                        imports.push(ResolvedImport {
                            source: module.clone(),
                            specifiers: vec![module],
                            line: node.start_position().row as u32 + 1,
                            is_default: true,
                            from_file: file_path.to_string(),
                        });
                    }
                }
            }
        }
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_imports(child, source, file_path, imports);
        }
    }
}

fn build_ruby_signature(node: Node, source: &str) -> String {
    if let Some(params) = node.child_by_field_name("parameters") {
        node_text(params, source)
    } else {
        String::new()
    }
}

fn superclass_signature(node: Node, source: &str) -> Option<String> {
    node.child_by_field_name("superclass")
        .map(|s| format!("< {}", node_text(s, source)))
}

fn extract_ruby_comment(node: Node, source: &str) -> Option<String> {
    let prev = node.prev_sibling()?;
    if prev.kind() == "comment" {
        let text = node_text(prev, source);
        if text.starts_with('#') {
            return Some(text);
        }
    }
    None
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
