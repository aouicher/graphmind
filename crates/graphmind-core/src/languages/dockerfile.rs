use crate::extractor::{CallSite, Symbol, SymbolKind};
use crate::resolver::ResolvedImport;
use tree_sitter::Node;

pub fn extract_symbols(root: Node, source: &str) -> Vec<Symbol> {
    crate::extractor::run_collect(root, source, collect_symbols)
}

pub fn extract_call_sites(_root: Node, _source: &str) -> Vec<CallSite> {
    Vec::new()
}

pub fn extract_imports(root: Node, source: &str, _file_path: &str) -> Vec<ResolvedImport> {
    let mut imports = Vec::new();
    collect_imports(root, source, &mut imports);
    imports
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "from_instruction" => {
            if let Some(image) = node.child_by_field_name("image").or_else(|| node.named_child(0)) {
                let name = node_text(image, source);
                let alias = node.child_by_field_name("as").map(|n| node_text(n, source));
                symbols.push(Symbol {
                    name: alias.unwrap_or(name),
                    kind: SymbolKind::Class,
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                    signature: None,
                    doc: None,
                });
            }
        }
        "label_instruction" => {
            for i in 0..node.named_child_count() {
                if let Some(pair) = node.named_child(i) {
                    if pair.kind() == "label_pair" {
                        if let Some(key) = pair.child_by_field_name("key") {
                            symbols.push(Symbol {
                                name: node_text(key, source),
                                kind: SymbolKind::Type,
                                line_start: node.start_position().row as u32 + 1,
                                line_end: node.end_position().row as u32 + 1,
                                signature: None,
                                doc: None,
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

fn collect_imports(node: Node, source: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "from_instruction" {
        if let Some(image) = node.child_by_field_name("image").or_else(|| node.named_child(0)) {
            imports.push(ResolvedImport {
                source: node_text(image, source),
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
