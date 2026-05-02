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
        "class_declaration" => {
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
        "interface_declaration" => {
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
        "struct_declaration" | "enum_declaration" | "record_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Type,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
        "method_declaration" | "constructor_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let sig = node.child_by_field_name("parameters").map(|p| node_text(p, source));
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Method,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: sig,
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
        "method_declaration" | "constructor_declaration" => {
            node.child_by_field_name("name").map(|n| node_text(n, source))
        }
        _ => None,
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "invocation_expression" {
        if let Some(func) = node.child_by_field_name("function").or_else(|| node.named_child(0)) {
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

fn collect_imports(node: Node, source: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "using_directive" {
        let text = node_text(node, source);
        let ns = text.trim_start_matches("using ")
            .trim_start_matches("static ")
            .trim_end_matches(';')
            .trim()
            .to_string();
        let spec = ns.rsplit('.').next().unwrap_or(&ns).to_string();
        imports.push(ResolvedImport {
            source: ns,
            specifiers: vec![spec],
            line: node.start_position().row as u32 + 1,
            is_default: false,
            from_file: String::new(),
        });
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
