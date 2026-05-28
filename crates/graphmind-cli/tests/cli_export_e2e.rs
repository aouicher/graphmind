mod common;
use common::QueryTestEnv;

#[test]
fn export_json_valid() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["export", &env.slug, "--format", "json"]);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&out);
    assert!(parsed.is_ok(), "export --format json should produce valid JSON, got: {out}");
    let json = parsed.unwrap();
    assert!(json.get("symbols").is_some(), "JSON export should have 'symbols' key");
}

#[test]
fn export_dot_format() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["export", &env.slug, "--format", "dot"]);
    assert!(out.contains("digraph"), "dot export should contain 'digraph', got: {}", &out[..out.len().min(200)]);
}

#[test]
fn export_mermaid_format() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["export", &env.slug, "--format", "mermaid"]);
    assert!(
        out.contains("graph") || out.contains("flowchart") || out.contains("-->"),
        "mermaid export should contain graph syntax, got: {}",
        &out[..out.len().min(200)]
    );
}
