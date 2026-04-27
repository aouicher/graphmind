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

pub fn extract_imports(root: Node, source: &str, file_path: &str) -> Vec<ResolvedImport> {
    let mut imports = Vec::new();
    collect_imports(root, source, file_path, &mut imports);
    imports
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let sig = build_go_signature(node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(sig),
                    doc: extract_go_doc(node, source),
                });
            }
        }
        "method_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let sig = build_go_signature(node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Method,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(sig),
                    doc: extract_go_doc(node, source),
                });
            }
        }
        "type_declaration" => {
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    if child.kind() == "type_spec" {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = node_text(name_node, source);
                            let type_node = child.child_by_field_name("type");
                            let kind = match type_node.map(|n| n.kind()) {
                                Some("struct_type") => SymbolKind::Class,
                                Some("interface_type") => SymbolKind::Interface,
                                _ => SymbolKind::Type,
                            };
                            symbols.push(Symbol {
                                name,
                                kind,
                                line_start: child.start_position().row as u32 + 1,
                                line_end: child.end_position().row as u32 + 1,
                                signature: None,
                                doc: extract_go_doc(node, source),
                            });
                        }
                    }
                }
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
        "function_declaration" | "method_declaration" => {
            node.child_by_field_name("name").map(|n| node_text(n, source))
        }
        _ => None,
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            let callee = if func.kind() == "selector_expression" {
                func.child_by_field_name("field")
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

fn collect_imports(node: Node, source: &str, file_path: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "import_declaration" {
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                match child.kind() {
                    "import_spec" => {
                        if let Some(path_node) = child.child_by_field_name("path") {
                            let raw = node_text(path_node, source);
                            let module = raw.trim_matches('"').to_string();
                            let name = child
                                .child_by_field_name("name")
                                .map(|n| node_text(n, source))
                                .unwrap_or_else(|| {
                                    module.rsplit('/').next().unwrap_or(&module).to_string()
                                });
                            imports.push(ResolvedImport {
                                source: module,
                                specifiers: vec![name],
                                line: child.start_position().row as u32 + 1,
                                is_default: true,
                                from_file: file_path.to_string(),
                            });
                        }
                    }
                    "import_spec_list" => {
                        for j in 0..child.named_child_count() {
                            if let Some(spec) = child.named_child(j) {
                                if spec.kind() == "import_spec" {
                                    if let Some(path_node) = spec.child_by_field_name("path") {
                                        let raw = node_text(path_node, source);
                                        let module = raw.trim_matches('"').to_string();
                                        let name = spec
                                            .child_by_field_name("name")
                                            .map(|n| node_text(n, source))
                                            .unwrap_or_else(|| {
                                                module.rsplit('/').next().unwrap_or(&module).to_string()
                                            });
                                        imports.push(ResolvedImport {
                                            source: module,
                                            specifiers: vec![name],
                                            line: spec.start_position().row as u32 + 1,
                                            is_default: true,
                                            from_file: file_path.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
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

fn build_go_signature(node: Node, source: &str) -> String {
    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = node_text(params, source);
        if let Some(ret) = node.child_by_field_name("result") {
            format!("{} {}", params_text, node_text(ret, source))
        } else {
            params_text
        }
    } else {
        String::new()
    }
}

fn extract_go_doc(node: Node, source: &str) -> Option<String> {
    let prev = node.prev_sibling()?;
    if prev.kind() == "comment" {
        let text = node_text(prev, source);
        if text.starts_with("//") {
            return Some(text);
        }
    }
    None
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
