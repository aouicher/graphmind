//! Regression tests for issue #109: a client probe sent before `initialize`
//! must not kill the MCP server.

use graphmind_mcp::handshake::{answer_preinit_probes, prepend};
use tokio::io::{AsyncReadExt, BufReader};

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;

async fn run(input: &str) -> (String, String) {
    let mut reader = BufReader::new(input.as_bytes());
    let mut out: Vec<u8> = Vec::new();
    let carry = answer_preinit_probes(&mut reader, &mut out).await;
    (carry, String::from_utf8(out).unwrap())
}

/// The exact sequence Copilot CLI 1.0.81 sends.
#[tokio::test]
async fn server_discover_probe_is_answered_and_initialize_survives() {
    let input = format!(
        "{}\n{}\n",
        r#"{"jsonrpc":"2.0","id":0,"method":"server/discover","params":{}}"#, INIT
    );
    let (carry, out) = run(&input).await;

    // The probe got a -32601 reply on the same id.
    let reply: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(reply["id"], 0);
    assert_eq!(reply["error"]["code"], -32601);

    // initialize is handed back untouched for rmcp.
    assert_eq!(carry.trim(), INIT);
}

/// With no probe, the filter must be a pure passthrough and write nothing.
#[tokio::test]
async fn initialize_first_is_untouched() {
    let (carry, out) = run(&format!("{INIT}\n")).await;
    assert_eq!(carry.trim(), INIT);
    assert!(out.is_empty(), "must not write when there is no probe");
}

/// rmcp handles pre-init `ping` itself, so it must reach rmcp, not us.
#[tokio::test]
async fn pre_init_ping_is_forwarded_not_answered() {
    let ping = r#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#;
    let (carry, out) = run(&format!("{ping}\n{INIT}\n")).await;
    assert_eq!(carry.trim(), ping);
    assert!(out.is_empty(), "rmcp must answer ping, not the filter");
}

/// Several probes in a row each get their own reply.
#[tokio::test]
async fn multiple_probes_each_get_a_reply() {
    let input = format!(
        "{}\n{}\n{}\n",
        r#"{"jsonrpc":"2.0","id":0,"method":"server/discover"}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":"vendor/hello"}"#,
        INIT
    );
    let (carry, out) = run(&input).await;
    assert_eq!(out.lines().count(), 2);
    assert_eq!(carry.trim(), INIT);
}

/// A notification has no id, so JSON-RPC forbids a response: swallow it.
#[tokio::test]
async fn pre_init_notification_gets_no_response() {
    let input = format!("{}\n{}\n", r#"{"jsonrpc":"2.0","method":"vendor/event"}"#, INIT);
    let (carry, out) = run(&input).await;
    assert!(out.is_empty(), "notifications take no reply");
    assert_eq!(carry.trim(), INIT);
}

/// Standard lifecycle notifications belong to rmcp.
#[tokio::test]
async fn lifecycle_notification_is_forwarded() {
    let note = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let (carry, out) = run(&format!("{note}\n")).await;
    assert_eq!(carry.trim(), note);
    assert!(out.is_empty());
}

/// Garbage must go to rmcp so it reports the protocol error, not be eaten here.
#[tokio::test]
async fn malformed_line_is_forwarded() {
    let (carry, out) = run("not json\n").await;
    assert_eq!(carry.trim(), "not json");
    assert!(out.is_empty());
}

#[tokio::test]
async fn eof_returns_empty_carry() {
    let (carry, out) = run("").await;
    assert!(carry.is_empty());
    assert!(out.is_empty());
}

/// `prepend` must reconstruct the exact byte stream rmcp would have seen.
#[tokio::test]
async fn prepend_replays_carry_then_rest() {
    let rest = b"second\nthird\n";
    let mut stream = prepend("first\n".to_string(), &rest[..]);
    let mut all = String::new();
    stream.read_to_string(&mut all).await.unwrap();
    assert_eq!(all, "first\nsecond\nthird\n");
}
