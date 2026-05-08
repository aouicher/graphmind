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

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    if node.kind() == "left_assignment" || node.kind() == "equals_assignment" || node.kind() == "binary_operator" {
        let lhs = node.named_child(0);
        let rhs = node.named_child(1).or_else(|| node.named_child(2));
        if let (Some(name_node), Some(value)) = (lhs, rhs) {
            if value.kind() == "function_definition" && name_node.kind() == "identifier" {
                symbols.push(Symbol {
                    name: node_text(name_node, source),
                    kind: SymbolKind::Function,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, symbols);
        }
    }
}

fn collect_call_sites(node: Node, source: &str, sites: &mut Vec<CallSite>, current_fn: Option<&str>) {
    let fn_name = if node.kind() == "left_assignment" || node.kind() == "equals_assignment" || node.kind() == "binary_operator" {
        let lhs = node.named_child(0);
        let rhs = node.named_child(1).or_else(|| node.named_child(2));
        if let (Some(name_node), Some(value)) = (lhs, rhs) {
            if value.kind() == "function_definition" && name_node.kind() == "identifier" {
                Some(node_text(name_node, source))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    let active_fn = fn_name.as_deref().or(current_fn);

    if node.kind() == "call" {
        if let Some(func) = node.child_by_field_name("function").or_else(|| node.named_child(0)) {
            let callee = node_text(func, source);
            if let Some(caller) = active_fn {
                sites.push(CallSite {
                    caller: caller.to_string(),
                    callee,
                    receiver: None,
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

fn collect_imports(node: Node, source: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "call" {
        if let Some(func) = node.child_by_field_name("function").or_else(|| node.named_child(0)) {
            let name = node_text(func, source);
            if name == "library" || name == "require" {
                if let Some(args) = node.child_by_field_name("arguments") {
                    if let Some(arg) = args.named_child(0) {
                        let pkg = node_text(arg, source)
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        imports.push(ResolvedImport {
                            source: pkg,
                            specifiers: Vec::new(),
                            line: node.start_position().row as u32 + 1,
                            is_default: false,
                            from_file: String::new(),
                        });
                    }
                }
            }
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
