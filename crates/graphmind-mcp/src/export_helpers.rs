use graphmind_db::queries::{EdgeRow, SymbolRow};

pub(crate) fn render_mermaid(symbols: &[SymbolRow], edges: &[EdgeRow], title: &str) -> String {
    let mut out = format!("flowchart LR\n  %% {title}\n");
    for s in symbols {
        let shape = match s.kind.as_str() {
            "Class" | "Interface" => format!("[{}]", s.name),
            "Function" | "Method" => format!("({})", s.name),
            _ => format!("[/{}\\]", s.name),
        };
        out.push_str(&format!("  s{}{};\n", s.id, shape));
    }
    for e in edges {
        let label = &e.kind;
        out.push_str(&format!("  s{} -->|{}| s{};\n", e.from_id, label, e.to_id));
    }
    out
}

pub(crate) fn render_dot(symbols: &[SymbolRow], edges: &[EdgeRow], title: &str) -> String {
    let mut out = format!("digraph \"{}\" {{\n  rankdir=LR;\n", title);
    for s in symbols {
        let shape = match s.kind.as_str() {
            "Class" | "Interface" => "box",
            "Function" | "Method" => "ellipse",
            _ => "diamond",
        };
        out.push_str(&format!("  s{} [label=\"{}\" shape={}];\n", s.id, s.name, shape));
    }
    for e in edges {
        out.push_str(&format!("  s{} -> s{} [label=\"{}\"];\n", e.from_id, e.to_id, e.kind));
    }
    out.push_str("}\n");
    out
}
