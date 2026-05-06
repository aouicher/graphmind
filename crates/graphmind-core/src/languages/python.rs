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
        "function_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let sig = build_python_signature(node, source);
                let doc = extract_python_docstring(node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: Some(sig),
                    doc,
                });
            }
        }
        "class_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let doc = extract_python_docstring(node, source);
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Class,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc,
                });
            }
            // Collect methods inside class body
            if let Some(body) = node.child_by_field_name("body") {
                collect_class_methods(body, source, symbols);
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

fn collect_class_methods(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "function_definition" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(name_node, source);
                    let sig = build_python_signature(child, source);
                    let doc = extract_python_docstring(child, source);
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Method,
                        line_start: child.start_position().row as u32 + 1,
                        line_end: child.end_position().row as u32 + 1,
                        signature: Some(sig),
                        doc,
                    });
                }
            }
        }
    }
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = if node.kind() == "function_definition" {
        node.child_by_field_name("name").map(|n| node_text(n, source))
    } else {
        None
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "call" {
        if let Some(func) = node.child_by_field_name("function") {
            let (callee, receiver) = if func.kind() == "attribute" {
                let method = func.child_by_field_name("attribute")
                    .map(|p| node_text(p, source))
                    .unwrap_or_else(|| node_text(func, source));
                let recv = func.child_by_field_name("object")
                    .map(|o| crate::extractor::extract_receiver_name(o, source));
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
    match node.kind() {
        "import_statement" => {
            // import foo, import foo.bar
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    if child.kind() == "dotted_name" {
                        let module = node_text(child, source);
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
        "import_from_statement" => {
            // from foo import bar, baz
            let mut module_name = String::new();
            let mut specifiers = Vec::new();

            if let Some(module_node) = node.child_by_field_name("module_name") {
                module_name = node_text(module_node, source);
            }

            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i) {
                    match child.kind() {
                        "dotted_name" if module_name.is_empty() => {
                            module_name = node_text(child, source);
                        }
                        "import_from_names" | "import_prefix" => {
                            for j in 0..child.named_child_count() {
                                if let Some(spec) = child.named_child(j) {
                                    if spec.kind() == "dotted_name" || spec.kind() == "identifier" {
                                        specifiers.push(node_text(spec, source));
                                    }
                                }
                            }
                        }
                        "dotted_name" | "identifier" => {
                            specifiers.push(node_text(child, source));
                        }
                        _ => {}
                    }
                }
            }

            if !module_name.is_empty() {
                imports.push(ResolvedImport {
                    source: module_name,
                    specifiers,
                    line: node.start_position().row as u32 + 1,
                    is_default: false,
                    from_file: file_path.to_string(),
                });
            }
        }
        _ => {}
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_imports(child, source, file_path, imports);
        }
    }
}

fn build_python_signature(node: Node, source: &str) -> String {
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

fn extract_python_docstring(node: Node, source: &str) -> Option<String> {
    let body = node.child_by_field_name("body")?;
    let first = body.named_child(0)?;
    if first.kind() == "expression_statement" {
        let expr = first.named_child(0)?;
        if expr.kind() == "string" {
            return Some(node_text(expr, source));
        }
    }
    None
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
