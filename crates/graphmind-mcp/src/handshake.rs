//! Tolerate non-standard client probes sent before the MCP `initialize` call.
//!
//! Some MCP clients send a vendor-specific request before the standard
//! `initialize` handshake. GitHub Copilot CLI >= 1.0.79 sends `server/discover`
//! (see aouicher/graphmind#109, github/copilot-cli#4370).
//!
//! rmcp's server handshake accepts only `ping` before `initialize`; anything
//! else aborts with `ExpectedInitializeRequest` and the process exits, so the
//! client never gets to send `initialize` at all.
//!
//! Copilot tolerates a `-32601 Method not found` reply to its probe and then
//! proceeds with the normal lifecycle, so that is what we answer. This runs
//! once, before rmcp is handed the stream; the first message it cannot be a
//! probe is returned untouched for rmcp to consume.

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Methods that must reach rmcp untouched: rmcp owns the whole lifecycle,
/// including its own pre-`initialize` `ping` handling.
fn is_lifecycle_method(method: &str) -> bool {
    method == "initialize" || method == "ping" || method.starts_with("notifications/")
}

/// A JSON-RPC request (has an `id`) for a method rmcp cannot accept before
/// `initialize`. Returns the `id` to answer with, or `None` if this line is
/// not such a probe and must be handed to rmcp as-is.
fn probe_id(line: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let method = value.get("method")?.as_str()?;
    if is_lifecycle_method(method) {
        return None;
    }
    // A notification (no id) needs no reply, but forwarding it would also
    // abort rmcp's handshake, so treat it as a probe with a null id and
    // swallow it without responding.
    Some(value.get("id").cloned().unwrap_or(serde_json::Value::Null))
}

fn method_not_found(id: &serde_json::Value, line: &str) -> String {
    let method = serde_json::from_str::<serde_json::Value>(line.trim())
        .ok()
        .and_then(|v| v.get("method").and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_default();
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32601,
            "message": format!("Method not found: {method}"),
        }
    })
    .to_string()
}

/// Read and answer pre-`initialize` probes on `input`, writing `-32601`
/// replies to `output`.
///
/// Returns the first line that is *not* a probe (the client's `initialize`,
/// `ping`, or anything else rmcp should decide about), which the caller must
/// replay to rmcp. Returns an empty string at EOF.
pub async fn answer_preinit_probes<R, W>(input: &mut R, output: &mut W) -> String
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line).await {
            Ok(0) | Err(_) => return String::new(),
            Ok(_) => {}
        }

        // Blank keep-alive lines carry no message; skip them.
        if line.trim().is_empty() {
            continue;
        }

        let Some(id) = probe_id(&line) else {
            return line;
        };

        // Notifications (null id) get no response, per JSON-RPC.
        if id.is_null() {
            continue;
        }

        let reply = method_not_found(&id, &line);
        if output.write_all(reply.as_bytes()).await.is_err()
            || output.write_all(b"\n").await.is_err()
            || output.flush().await.is_err()
        {
            return String::new();
        }
    }
}

/// Chain an already-read line back in front of the rest of the stream, so rmcp
/// sees the byte stream it would have seen with no filter at all.
pub fn prepend<R: AsyncRead + Unpin>(carry: String, rest: R) -> impl AsyncRead + Unpin {
    std::io::Cursor::new(carry.into_bytes()).chain(rest)
}
