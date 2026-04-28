use crate::extractor::{CallSite, Symbol, SymbolKind};
use crate::resolver::ResolvedImport;
use tree_sitter::Node;

pub fn extract_symbols(root: Node, source: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    collect_symbols(root, source, &mut symbols);
    symbols
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
        "rule_set" => {
            if let Some(sel) = node.child_by_field_name("selectors").or_else(|| node.named_child(0)) {
                if sel.kind() == "selectors" || sel.kind() == "selector" || sel.kind() == "class_selector" || sel.kind() == "id_selector" {
                    symbols.push(Symbol {
                        name: node_text(sel, source),
                        kind: SymbolKind::Type,
                        line_start: node.start_position().row as u32 + 1,
                        line_end: node.end_position().row as u32 + 1,
                        signature: None,
                        doc: None,
                    });
                }
            }
        }
        "keyframes_statement" => {
            if let Some(name_node) = node.child_by_field_name("name").or_else(|| node.named_child(0)) {
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
        _ => {}
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, symbols);
        }
    }
}

fn collect_imports(node: Node, source: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "import_statement" {
        let text = node_text(node, source);
        let path = text
            .trim_start_matches("@import ")
            .trim_end_matches(';')
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        imports.push(ResolvedImport {
            source: path,
            specifiers: Vec::new(),
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
