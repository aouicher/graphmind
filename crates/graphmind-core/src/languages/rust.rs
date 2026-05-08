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
        "function_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let sig = build_rust_signature(node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(sig),
                    doc: extract_rust_doc(node, source),
                });
            }
        }
        "struct_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Class,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: extract_rust_doc(node, source),
                });
            }
        }
        "enum_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Type,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: extract_rust_doc(node, source),
                });
            }
        }
        "trait_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Interface,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: extract_rust_doc(node, source),
                });
            }
        }
        "impl_item" => {
            collect_impl_methods(node, source, symbols);
        }
        _ => {}
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, symbols);
        }
    }
}

fn collect_impl_methods(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.named_child_count() {
            if let Some(child) = body.named_child(i) {
                if child.kind() == "function_item" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = node_text(name_node, source);
                        let sig = build_rust_signature(child, source);
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Method,
                            line_start: child.start_position().row as u32 + 1,
                            line_end: child.end_position().row as u32 + 1,
                            signature: Some(sig),
                            doc: extract_rust_doc(child, source),
                        });
                    }
                }
            }
        }
    }
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = if node.kind() == "function_item" {
        node.child_by_field_name("name").map(|n| node_text(n, source))
    } else {
        None
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            let (callee, receiver) = if func.kind() == "field_expression" {
                let method = func.child_by_field_name("field")
                    .map(|p| node_text(p, source))
                    .unwrap_or_else(|| node_text(func, source));
                let recv = func.child_by_field_name("value")
                    .map(|o| crate::extractor::extract_receiver_name(o, source));
                (method, recv)
            } else if func.kind() == "scoped_identifier" {
                let method = func.child_by_field_name("name")
                    .map(|p| node_text(p, source))
                    .unwrap_or_else(|| node_text(func, source));
                let recv = func.child_by_field_name("path")
                    .map(|o| node_text(o, source));
                // tokio::spawn / thread::spawn / rayon::spawn → spawns
                let full = node_text(func, source);
                if method == "spawn" && (full.contains("tokio") || full.contains("thread") || full.contains("rayon") || full.contains("task")) {
                    let kind = "spawns".to_string();
                    if let Some(caller) = active_fn {
                        sites.push(CallSite {
                            caller: caller.to_string(),
                            callee: full,
                            receiver: recv,
                            line: node.start_position().row as u32 + 1,
                            kind,
                        });
                    }
                    for i in 0..node.named_child_count() {
                        if let Some(child) = node.named_child(i) {
                            collect_call_sites(child, source, sites, active_fn);
                        }
                    }
                    return;
                }
                (method, recv)
            } else {
                (node_text(func, source), None)
            };
            let kind = crate::extractor::classify_call_kind(&callee).to_string();
            if let Some(caller) = active_fn {
                sites.push(CallSite {
                    caller: caller.to_string(),
                    callee,
                    receiver,
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

fn collect_imports(node: Node, source: &str, file_path: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "use_declaration" {
        let mut specifiers = Vec::new();
        collect_use_paths(node, source, &mut specifiers);

        if !specifiers.is_empty() {
            let full_text = node_text(node, source);
            let module = full_text
                .trim_start_matches("use ")
                .trim_end_matches(';')
                .to_string();
            imports.push(ResolvedImport {
                source: module,
                specifiers,
                line: node.start_position().row as u32 + 1,
                is_default: false,
                from_file: file_path.to_string(),
            });
        }
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_imports(child, source, file_path, imports);
        }
    }
}

fn collect_use_paths(node: Node, source: &str, specifiers: &mut Vec<String>) {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            match child.kind() {
                "identifier" | "scoped_identifier" => {
                    specifiers.push(node_text(child, source));
                }
                "use_list" | "use_wildcard" | "use_as_clause" | "scoped_use_list" => {
                    collect_use_paths(child, source, specifiers);
                }
                _ => {
                    collect_use_paths(child, source, specifiers);
                }
            }
        }
    }
}

fn build_rust_signature(node: Node, source: &str) -> String {
    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = node_text(params, source);
        if let Some(ret) = node.child_by_field_name("return_type") {
            format!("{} -> {}", params_text, node_text(ret, source))
        } else {
            params_text
        }
    } else {
        String::new()
    }
}

fn extract_rust_doc(node: Node, source: &str) -> Option<String> {
    let prev = node.prev_sibling()?;
    if prev.kind() == "line_comment" {
        let text = node_text(prev, source);
        if text.starts_with("///") || text.starts_with("//!") {
            return Some(text);
        }
    }
    None
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
