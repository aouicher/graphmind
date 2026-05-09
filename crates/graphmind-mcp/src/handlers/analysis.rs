use serde_json::Value;

use crate::formatting::{compact_symbol_line, err_text, text_content};
use crate::graph_helpers::{qualify_definitions, with_graph};

pub(crate) fn handle_outline(args: &Value) -> Value {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return err_text("Missing required parameter: file"),
    };
    with_graph(args, |gq, _proj| {
        let outline = gq.outline(file);
        let mut lines = vec![format!(">> outline: {}\n", file)];
        fn render_tree(
            nodes: &[graphmind_db::queries::OutlineNode],
            lines: &mut Vec<String>,
            depth: usize,
        ) {
            for node in nodes {
                let indent = "  ".repeat(depth);
                let sig = node.signature.as_deref().unwrap_or("");
                let sig_part = if sig.is_empty() {
                    String::new()
                } else {
                    format!("({})", sig)
                };
                lines.push(format!(
                    "{}{} {}{} :{}",
                    indent, node.kind, node.name, sig_part, node.line_start
                ));
                if !node.children.is_empty() {
                    render_tree(&node.children, lines, depth + 1);
                }
            }
        }
        render_tree(&outline, &mut lines, 1);
        text_content(&lines.join("\n"))
    })
}

pub(crate) fn handle_who_calls_chain(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    let depth = args
        .get("depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as usize;

    with_graph(args, |gq, _proj| {
        let (chain, max_reached) = gq.who_calls_chain(symbol, depth);
        let mut lines = vec![format!(
            ">> who calls {} (depth {}, {} callers):\n",
            symbol,
            depth,
            chain.len()
        )];
        for node in &chain {
            let indent = "  ".repeat(node.depth + 1);
            lines.push(format!(
                "{}← {} [{}] {}:{}",
                indent, node.name, node.kind, node.file, node.line_start
            ));
        }
        if max_reached {
            lines.push(format!(
                "\n  (max depth {} reached — increase depth for more)",
                depth
            ));
        }
        text_content(&lines.join("\n"))
    })
}

pub(crate) fn handle_dead_code(args: &Value) -> Value {
    let kind = args.get("kind").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);

    with_graph(args, |gq, proj| {
        let dead = gq.dead_code(kind, limit);
        let qualified = qualify_definitions(gq, &dead, false);
        let kind_label = kind.unwrap_or("all");
        let mut lines = vec![format!(
            ">> {} dead symbols [{}] (kind: {}):\n",
            qualified.len(),
            proj.slug,
            kind_label
        )];
        for s in &qualified {
            lines.push(compact_symbol_line(s));
        }
        text_content(&lines.join("\n"))
    })
}

pub(crate) fn handle_similar(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);

    with_graph(args, |gq, proj| {
        let found = gq.find_symbol(symbol);
        if found.is_empty() {
            return err_text(&format!("Symbol '{}' not found", symbol));
        }
        let results = gq.similar_symbols(found[0].id, limit);
        let mut lines = vec![format!(
            ">> {} similar to \"{}\" [{}]:\n",
            results.len(),
            symbol,
            proj.slug
        )];
        for s in &results {
            lines.push(format!(
                "  {} [{}] {}:{} ({:.2})",
                s.name, s.kind, s.file, s.line_start, s.score
            ));
        }
        text_content(&lines.join("\n"))
    })
}
