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
    if node.kind() == "element" {
        if let Some(start_tag) = node.child_by_field_name("start_tag").or_else(|| {
            if node.named_child_count() > 0 {
                let c = node.named_child(0)?;
                if c.kind() == "start_tag" { Some(c) } else { None }
            } else {
                None
            }
        }) {
            for i in 0..start_tag.named_child_count() {
                if let Some(attr) = start_tag.named_child(i) {
                    if attr.kind() == "attribute" {
                        if let Some(name) = attr.child_by_field_name("name") {
                            if node_text(name, source) == "id" {
                                if let Some(val) = attr.child_by_field_name("value") {
                                    let id = node_text(val, source)
                                        .trim_matches('"')
                                        .trim_matches('\'')
                                        .to_string();
                                    symbols.push(Symbol {
                                        name: id,
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
            }
        }
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, symbols);
        }
    }
}

fn collect_imports(node: Node, source: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "script_element" || node.kind() == "style_element" {
        if let Some(start_tag) = node.named_child(0) {
            if start_tag.kind() == "start_tag" {
                for i in 0..start_tag.named_child_count() {
                    if let Some(attr) = start_tag.named_child(i) {
                        if attr.kind() == "attribute" {
                            if let Some(name) = attr.child_by_field_name("name") {
                                let attr_name = node_text(name, source);
                                if attr_name == "src" || attr_name == "href" {
                                    if let Some(val) = attr.child_by_field_name("value") {
                                        let path = node_text(val, source)
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
                                }
                            }
                        }
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
