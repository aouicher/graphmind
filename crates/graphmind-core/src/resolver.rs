use napi_derive::napi;
use tree_sitter::Node;

#[napi(object)]
#[derive(Debug, Clone)]
pub struct ResolvedImport {
    pub source: String,
    pub specifiers: Vec<String>,
    pub line: u32,
    pub is_default: bool,
    pub from_file: String,
}

pub fn extract_imports(root: Node, source: &str, file_path: &str) -> Vec<ResolvedImport> {
    let mut imports = Vec::new();
    collect_imports(root, source, file_path, &mut imports);
    imports
}

fn collect_imports(node: Node, source: &str, file_path: &str, imports: &mut Vec<ResolvedImport>) {
    if node.kind() == "import_statement" {
        if let Some(import) = parse_import(node, source, file_path) {
            imports.push(import);
        }
    }

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            collect_imports(child, source, file_path, imports);
        }
    }
}

fn parse_import(node: Node, source: &str, file_path: &str) -> Option<ResolvedImport> {
    let source_node = node.child_by_field_name("source")?;
    let raw_source = node_text(source_node, source);
    let module_source = raw_source.trim_matches(|c| c == '\'' || c == '"');

    let mut specifiers = Vec::new();
    let mut is_default = false;

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            match child.kind() {
                "import_clause" => {
                    for j in 0..child.named_child_count() {
                        if let Some(spec) = child.named_child(j) {
                            match spec.kind() {
                                "identifier" => {
                                    is_default = true;
                                    specifiers.push(node_text(spec, source));
                                }
                                "named_imports" => {
                                    collect_named_specifiers(spec, source, &mut specifiers);
                                }
                                "namespace_import" => {
                                    specifiers.push(format!("* as {}", node_text(spec, source)));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                "import_specifier" => {
                    specifiers.push(node_text(child, source));
                }
                _ => {}
            }
        }
    }

    Some(ResolvedImport {
        source: module_source.to_string(),
        specifiers,
        line: node.start_position().row as u32 + 1,
        is_default,
        from_file: file_path.to_string(),
    })
}

fn collect_named_specifiers(node: Node, source: &str, specifiers: &mut Vec<String>) {
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "import_specifier" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    specifiers.push(node_text(name_node, source));
                } else {
                    specifiers.push(node_text(child, source));
                }
            }
        }
    }
}

fn node_text(node: Node, source: &str) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    source[start..end].to_string()
}
