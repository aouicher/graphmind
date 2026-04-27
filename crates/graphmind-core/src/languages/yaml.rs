use crate::extractor::{CallSite, Symbol, SymbolKind};
use crate::resolver::ResolvedImport;
use tree_sitter::Node;

pub fn extract_symbols(root: Node, source: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    collect_top_level_keys(root, source, &mut symbols);
    symbols
}

pub fn extract_call_sites(_root: Node, _source: &str) -> Vec<CallSite> {
    Vec::new()
}

pub fn extract_imports(_root: Node, _source: &str, _file_path: &str) -> Vec<ResolvedImport> {
    Vec::new()
}

fn collect_top_level_keys(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    for i in 0..node.named_child_count() {
        let Some(child) = node.named_child(i) else {
            continue;
        };

        if child.kind() == "block_mapping" {
            collect_mapping_entries(child, source, symbols);
        } else if child.kind() == "document" {
            for j in 0..child.named_child_count() {
                if let Some(doc_child) = child.named_child(j) {
                    if doc_child.kind() == "block_node" || doc_child.kind() == "block_mapping" {
                        collect_from_block_node(doc_child, source, symbols);
                    }
                }
            }
        }
    }
}

fn collect_from_block_node(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    if node.kind() == "block_mapping" {
        collect_mapping_entries(node, source, symbols);
        return;
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "block_mapping" {
                collect_mapping_entries(child, source, symbols);
            }
        }
    }
}

fn collect_mapping_entries(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    for i in 0..node.named_child_count() {
        let Some(entry) = node.named_child(i) else {
            continue;
        };
        if entry.kind() != "block_mapping_pair" {
            continue;
        }

        let Some(key_node) = entry.child_by_field_name("key") else {
            continue;
        };

        let key = node_text(key_node, source).trim().to_string();
        if key.is_empty() {
            continue;
        }

        symbols.push(Symbol {
            name: key,
            kind: SymbolKind::Type,
            line_start: entry.start_position().row as u32 + 1,
            line_end: entry.end_position().row as u32 + 1,
            signature: None,
            doc: None,
        });
    }
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
