//! gm_file: reading a file of a registered project through the MCP handlers.

mod common;

use common::setup;
use serde_json::json;

#[test]
fn gm_file_returns_content() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_file",
        json!({ "file": "src/services/wallet.ts", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(
        text.contains("wallet"),
        "expected 'wallet' in: {text}"
    );
}

#[test]
fn gm_file_not_found_returns_error() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_file",
        json!({ "file": "src/nonexistent.ts", "project": ctx.slug }),
    );
    // handler returns err_text when file cannot be read
    assert_eq!(
        resp.get("isError"),
        Some(&json!(true)),
        "expected isError=true for missing file"
    );
    let text = ctx.text(&resp);
    assert!(text.contains("gm_outline and gm_search"), "an unindexed path names the tools that list indexed ones: {text}");
}

/// gm_file serves only indexed files of the project: an absolute path or `..` must not read a
/// file outside it, even one that exists.
#[test]
fn gm_file_refuses_paths_outside_the_project() {
    let ctx = setup();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), "GM_OUTSIDE_MARKER").unwrap();
    // tests/fixtures/sample-project/../../../Cargo.toml is the workspace manifest.
    for file in [outside.path().to_str().unwrap(), "../../../Cargo.toml", "src/../../../../Cargo.toml"] {
        let resp = ctx.dispatch("gm_file", json!({ "file": file, "project": ctx.slug }));
        let text = ctx.text(&resp);
        assert!(ctx.is_error(&resp), "{file} must be refused, got: {text}");
        assert!(!text.contains("GM_OUTSIDE_MARKER") && !text.contains("[workspace]"), "{file} leaked: {text}");
    }
}
