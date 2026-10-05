//! Embedding during `graphmind build` against a mock OpenAI-compatible server: a failing input
//! loses only itself and is retried later, an outage doesn't hammer the server, and edited files
//! are re-embedded by a normal build.

mod common;
use common::CliTestEnv;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default)]
struct Mock {
    /// Fail any request containing this marker.
    poison: AtomicBool,
    /// Fail every request.
    down: AtomicBool,
    requests: AtomicUsize,
}

fn start_mock() -> (Arc<Mock>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let mock = Arc::new(Mock::default());
    let m = mock.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).ok();
            let body = String::from_utf8_lossy(&body).to_string();
            m.requests.fetch_add(1, Ordering::SeqCst);

            let fail = m.down.load(Ordering::SeqCst) || (m.poison.load(Ordering::SeqCst) && body.contains("POISON"));
            let (status, payload) = if fail {
                ("500 Internal Server Error", r#"{"error":"mock failure"}"#.to_string())
            } else {
                let req: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                let n = req["input"].as_array().map(|a| a.len()).unwrap_or(0);
                let data: Vec<_> = (0..n).map(|i| serde_json::json!({"embedding": [i as f32 + 1.0, 1.0, 0.5]})).collect();
                ("200 OK", serde_json::json!({ "data": data }).to_string())
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            stream.write_all(resp.as_bytes()).ok();
        }
    });
    (mock, url)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), to).unwrap();
        }
    }
}

/// Copy the fixture (so tests can edit it), register it, and point embeddings at the mock.
fn setup(env: &CliTestEnv, mock_url: &str) -> std::path::PathBuf {
    let project = env.home.path().join("project");
    copy_dir(&env.fixture_path(), &project);
    std::fs::write(
        project.join("src/poison.ts"),
        "export function poisonPill(): string {\n  return \"POISON\";\n}\n",
    )
    .unwrap();
    env.run_ok(&["register", project.to_str().unwrap(), "--slug", "emb"]);

    let config_path = env.home.path().join(".graphmind/config.json");
    let mut config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    config["embedding"] = serde_json::json!({
        "mode": "openai",
        "model": "mock-embed",
        "openai_base_url": mock_url,
        "api_keys": { "openai": "test-key" },
        "batch_size": 4,
        "request_timeout_secs": 5,
    });
    std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    project
}

fn embedded_count(env: &CliTestEnv) -> usize {
    let out = env.run_ok(&["embed", "emb"]);
    let line = out.lines().find(|l| l.contains("Symbols:")).unwrap_or_else(|| panic!("no count in: {out}"));
    let digits: String = line.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap()
}

#[test]
fn failing_symbol_loses_only_itself_and_is_retried() {
    let env = CliTestEnv::new();
    let (mock, url) = start_mock();
    setup(&env, &url);
    mock.poison.store(true, Ordering::SeqCst);

    // The graph is fine, so the build still succeeds; the gap is reported.
    let out = env.run(&["build", "emb"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "build should succeed: {stderr}");
    assert!(stderr.contains("1 of"), "should report exactly one missing symbol: {stderr}");
    assert!(stderr.contains("mock failure"), "should show the provider error: {stderr}");
    let partial = embedded_count(&env);
    assert!(partial > 0);

    // `embed --run` makes the failure visible to scripts.
    let out = env.run(&["embed", "--run", "emb"]);
    assert!(!out.status.success(), "embed --run should exit non-zero while symbols fail");

    // Once the provider recovers, the next run fills the gap without redoing the rest.
    mock.poison.store(false, Ordering::SeqCst);
    let out = env.run_ok(&["embed", "--run", "emb"]);
    assert!(out.contains("Embedding 1 symbols"), "only the missing symbol: {out}");
    assert_eq!(embedded_count(&env), partial + 1);
}

#[test]
fn provider_outage_does_not_hammer_the_server() {
    let env = CliTestEnv::new();
    let (mock, url) = start_mock();
    setup(&env, &url);
    mock.down.store(true, Ordering::SeqCst);

    let out = env.run(&["build", "emb"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "build should succeed: {stderr}");
    assert!(stderr.contains("were not embedded"), "{stderr}");
    let calls = mock.requests.load(Ordering::SeqCst);
    assert!(calls <= 6, "outage made {calls} requests with batch_size 4");
}

#[test]
fn normal_build_re_embeds_edited_files() {
    let env = CliTestEnv::new();
    let (_mock, url) = start_mock();
    let project = setup(&env, &url);
    env.run_ok(&["build", "emb"]);
    let before = embedded_count(&env);

    let logger = project.join("src/utils/logger.ts");
    let src = std::fs::read_to_string(&logger).unwrap().replace("[INFO]", "[INFO-EDITED]");
    std::fs::write(&logger, src).unwrap();

    let out = env.run_ok(&["build", "emb"]);
    assert!(out.contains("across 1 files"), "only the edited file is re-embedded: {out}");
    assert_eq!(embedded_count(&env), before, "old rows are replaced, not kept alongside");

    let out = env.run_ok(&["build", "emb"]);
    assert!(!out.contains("Embedding "), "nothing left to embed: {out}");
}
