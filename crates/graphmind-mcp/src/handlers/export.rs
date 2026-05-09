use serde_json::Value;

use crate::export_helpers::{render_dot, render_mermaid};
use crate::formatting::{err_text, text_content};
use crate::graph_helpers::with_graph;

pub(crate) fn handle_export(args: &Value) -> Value {
    let file = args.get("file").and_then(|v| v.as_str());
    let symbol = args.get("symbol").and_then(|v| v.as_str());
    let format = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("mermaid");
    let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;

    if file.is_none() && symbol.is_none() {
        return err_text("At least one of 'file' or 'symbol' is required");
    }

    with_graph(args, |gq, proj| {
        let (symbols, edges) = if let Some(f) = file {
            gq.file_subgraph(f)
        } else {
            let sym_name = symbol.unwrap();
            let found = gq.find_symbol(sym_name);
            if found.is_empty() {
                return err_text(&format!("Symbol '{}' not found", sym_name));
            }
            gq.neighborhood(found[0].id, depth)
        };

        let output = match format {
            "dot" => render_dot(&symbols, &edges, &proj.slug),
            _ => render_mermaid(&symbols, &edges, &proj.slug),
        };

        text_content(&output)
    })
}
