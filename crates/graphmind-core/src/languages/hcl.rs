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

pub fn extract_imports(_root: Node, _source: &str, _file_path: &str) -> Vec<ResolvedImport> {
    Vec::new()
}

fn collect_symbols(node: Node, source: &str, symbols: &mut Vec<Symbol>) {
    if node.kind() == "block" {
        if let Some(block_type) = node.child(0) {
            let type_name = node_text(block_type, source);
            match type_name.as_str() {
                "resource" | "data" => {
                    let resource_type = node.child(1).map(|n| unquote(&node_text(n, source)));
                    let resource_name = node.child(2).map(|n| unquote(&node_text(n, source)));
                    if let (Some(rt), Some(rn)) = (resource_type, resource_name) {
                        symbols.push(Symbol {
                            name: format!("{}.{}", rt, rn),
                            kind: SymbolKind::Class,
                            line_start: node.start_position().row as u32 + 1,
                            line_end: node.end_position().row as u32 + 1,
                            signature: Some(type_name),
                            doc: None,
                        });
                    }
                }
                "variable" => {
                    if let Some(name_node) = node.child(1) {
                        let name = unquote(&node_text(name_node, source));
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Type,
                            line_start: node.start_position().row as u32 + 1,
                            line_end: node.end_position().row as u32 + 1,
                            signature: Some("variable".to_string()),
                            doc: extract_description(node, source),
                        });
                    }
                }
                "output" => {
                    if let Some(name_node) = node.child(1) {
                        let name = unquote(&node_text(name_node, source));
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Type,
                            line_start: node.start_position().row as u32 + 1,
                            line_end: node.end_position().row as u32 + 1,
                            signature: Some("output".to_string()),
                            doc: extract_description(node, source),
                        });
                    }
                }
                "module" => {
                    if let Some(name_node) = node.child(1) {
                        let name = unquote(&node_text(name_node, source));
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Function,
                            line_start: node.start_position().row as u32 + 1,
                            line_end: node.end_position().row as u32 + 1,
                            signature: Some("module".to_string()),
                            doc: None,
                        });
                    }
                }
                "locals" => {
                    if let Some(body) = node.child_by_field_name("body") {
                        collect_locals(body, source, symbols, node.start_position().row as u32 + 1, node.end_position().row as u32 + 1);
                    }
                }
                "provider" => {
                    if let Some(name_node) = node.child(1) {
                        let name = unquote(&node_text(name_node, source));
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Interface,
                            line_start: node.start_position().row as u32 + 1,
                            line_end: node.end_position().row as u32 + 1,
                            signature: Some("provider".to_string()),
                            doc: None,
                        });
                    }
                }
                "terraform" => {
                    symbols.push(Symbol {
                        name: "terraform".to_string(),
                        kind: SymbolKind::Interface,
                        line_start: node.start_position().row as u32 + 1,
                        line_end: node.end_position().row as u32 + 1,
                        signature: Some("terraform".to_string()),
                        doc: None,
                    });
                }
                _ => {}
            }
        }
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_symbols(child, source, symbols);
        }
    }
}

fn collect_locals(node: Node, source: &str, symbols: &mut Vec<Symbol>, line_start: u32, line_end: u32) {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "attribute" {
                if let Some(key) = child.child(0) {
                    symbols.push(Symbol {
                        name: node_text(key, source),
                        kind: SymbolKind::Type,
                        line_start,
                        line_end,
                        signature: Some("local".to_string()),
                        doc: None,
                    });
                }
            }
        }
    }
}

fn extract_description(block: Node, source: &str) -> Option<String> {
    let body = block.child_by_field_name("body")?;
    for i in 0..body.named_child_count() {
        if let Some(attr) = body.named_child(i) {
            if attr.kind() == "attribute" {
                if let Some(key) = attr.child(0) {
                    if node_text(key, source) == "description" {
                        if let Some(val) = attr.child(2) {
                            return Some(unquote(&node_text(val, source)));
                        }
                    }
                }
            }
        }
    }
    None
}

fn unquote(s: &str) -> String {
    s.trim_matches('"').to_string()
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

#[cfg(test)]
mod tests {
    use crate::parser;

    #[test]
    fn extracts_hcl_symbols() {
        let source = r#"resource "aws_s3_bucket" "my_bucket" {
  bucket = "my-bucket"
}

variable "region" {
  description = "AWS region"
  default     = "eu-west-1"
}

module "vpc" {
  source = "./modules/vpc"
}

output "bucket_arn" {
  description = "The ARN of the bucket"
  value       = aws_s3_bucket.my_bucket.arn
}
"#;
        let result = parser::parse("main.tf", source, "hcl").unwrap();
        let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"aws_s3_bucket.my_bucket"), "missing resource, got: {:?}", names);
        assert!(names.contains(&"region"), "missing variable, got: {:?}", names);
        assert!(names.contains(&"vpc"), "missing module, got: {:?}", names);
        assert!(names.contains(&"bucket_arn"), "missing output, got: {:?}", names);
    }
}
