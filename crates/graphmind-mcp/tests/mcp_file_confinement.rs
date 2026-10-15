//! gm_file serves only indexed files of the project, given relative to the project root, that
//! resolve to a regular, indexed file inside the project directory. Its paging parameters are
//! tested here too.

use graphmind_mcp::handlers::dispatch_tool;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Mutex;

// Tests override HOME, so they run one at a time.
static TEST_LOCK: Mutex<()> = Mutex::new(());

const OUTSIDE_MARKER: &str = "GM_OUTSIDE_MARKER";
const ENV_MARKER: &str = "GM_ENV_MARKER";
const IGNORED_MARKER: &str = "GM_IGNORED_MARKER";
const HIDDEN_MARKER: &str = "GM_HIDDEN_MARKER";

/// A HOME with one registered and built project, `alpha`, at `<home>/alpha`:
/// - `src/a.ts`, `src/lines.ts` (three lines) and `b.ts`, indexed, and `src/alias.ts`, an indexed
///   symlink to `src/a.ts`;
/// - `.env`, the gitignored `secret.ts` and `.hidden/a.ts`, not indexed;
/// - `src/env.ts` and `src/secret_link.ts`, symlinks to `.env` and `secret.ts`;
/// - `src/outside.ts`, a symlink to `<home>/outside.ts`, outside the project.
///
/// `f` gets the canonical project directory and the file outside it.
fn with_built_project(f: impl FnOnce(&Path, &Path)) {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().unwrap();
    let home_path = home.path().canonicalize().unwrap();
    let project = home_path.join("alpha");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/a.ts"), "export function alpha() { return 1; }\n").unwrap();
    std::fs::write(project.join("b.ts"), "export function beta() { return 2; }\n").unwrap();
    std::fs::write(
        project.join("src/lines.ts"),
        "export function one() { return 1; }\nexport function two() { return 2; }\nexport function three() { return 3; }\n",
    )
    .unwrap();
    std::fs::create_dir_all(project.join(".hidden")).unwrap();
    std::fs::write(project.join(".hidden/a.ts"), format!("export const h = '{HIDDEN_MARKER}';\n")).unwrap();
    std::fs::write(project.join(".env"), format!("SECRET={ENV_MARKER}\n")).unwrap();
    std::fs::write(project.join(".gitignore"), "secret.ts\n").unwrap();
    std::fs::write(project.join("secret.ts"), format!("export const s = '{IGNORED_MARKER}';\n")).unwrap();
    let outside = home_path.join("outside.ts");
    std::fs::write(&outside, format!("export const o = '{OUTSIDE_MARKER}';\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(project.join(".env"), project.join("src/env.ts")).unwrap();
        symlink(project.join("secret.ts"), project.join("src/secret_link.ts")).unwrap();
        symlink(&outside, project.join("src/outside.ts")).unwrap();
        symlink("a.ts", project.join("src/alias.ts")).unwrap();
    }

    let graphmind_dir = home_path.join(".graphmind");
    let graph_dir = graphmind_dir.join("graphs/alpha");
    std::fs::create_dir_all(graph_dir.join("cache")).unwrap();
    std::fs::create_dir_all(graphmind_dir.join("memory")).unwrap();
    let config = json!({
        "version": "1",
        "projects": {
            "alpha": {
                "slug": "alpha",
                "path": project.to_str().unwrap(),
                "last_build": null,
                "languages": [],
                "registered": "2024-01-01T00:00:00Z",
                "auto_watch": false,
                "exclude": []
            }
        },
        "global_exclude": [],
        "defaults": { "embedding_model": "minilm", "watch_debounce": 2000, "max_depth": 5, "exclude_tests": true },
        "mcp": { "transport": "stdio", "http_port": 37378, "restrict_to_projects": null }
    });
    std::fs::write(graphmind_dir.join("config.json"), config.to_string()).unwrap();
    unsafe { std::env::set_var("HOME", &home_path) };

    let mut builder = graphmind_db::builder::GraphBuilder::new(
        graph_dir.join("graph.db").to_str().unwrap(),
        graph_dir.join("cache").to_str().unwrap(),
    );
    builder.build(
        project.to_str().unwrap(),
        &graphmind_db::builder::BuildOptions { full: true, ..Default::default() },
    );
    f(&project, &outside);
}

fn gm_file(file: &str) -> Value {
    dispatch_tool("gm_file", &json!({ "file": file, "project": "alpha" }))
}

fn text(v: &Value) -> &str {
    v["content"][0]["text"].as_str().unwrap_or("")
}

fn is_error(v: &Value) -> bool {
    v.get("isError").and_then(Value::as_bool).unwrap_or(false)
}

/// `file` is refused and the answer contains none of the markers of the files it must not read.
fn assert_refused(file: &str) -> String {
    let resp = gm_file(file);
    let text = text(&resp).to_string();
    assert!(is_error(&resp), "{file} must be refused, got: {text}");
    for marker in [OUTSIDE_MARKER, ENV_MARKER, IGNORED_MARKER, HIDDEN_MARKER] {
        assert!(!text.contains(marker), "{file} leaked {marker}: {text}");
    }
    text
}

/// gm_file's paging wiring: `offset`, `limit` and `column` reach the pager, a partial page ends
/// with the note after the closing fence, and invalid values are errors. The paging rules
/// themselves are unit-tested in `handlers/file_page.rs`.
#[test]
fn gm_file_pages_with_offset_limit_and_column() {
    with_built_project(|project, _| {
        let file = "src/lines.ts";
        let content = std::fs::read_to_string(project.join(file)).unwrap();
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let header = format!("# {file} [alpha]\n\n```\n");
        let read = |extra: Value| {
            let mut args = json!({ "file": file, "project": "alpha" });
            args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            dispatch_tool("gm_file", &args)
        };

        // A file under the cap comes back whole, without a note.
        assert_eq!(text(&read(json!({}))), format!("{header}{content}\n```"));

        assert_eq!(
            text(&read(json!({ "offset": 2, "limit": 1 }))),
            format!(
                "{header}{}\n```\n\n[graphmind: showing lines 2-2 of 3; call gm_file again with offset=3 to continue]",
                lines[1]
            )
        );

        // Column 8 of line 1 is byte 7: the page starts after "export ".
        assert_eq!(
            text(&read(json!({ "column": 8, "limit": 1 }))),
            format!(
                "{header}{}\n```\n\n[graphmind: showing lines 1-1 of 3; call gm_file again with offset=2 to continue]",
                &lines[0][7..]
            )
        );

        for (name, bad) in [("offset", json!("4")), ("column", json!(0)), ("limit", json!(-1))] {
            let resp = read(json!({ name: bad }));
            assert!(is_error(&resp), "{name}={bad} must be an error");
            assert!(text(&resp).starts_with(&format!("Invalid parameter {name}:")), "{}", text(&resp));
        }
    });
}

#[test]
fn indexed_file_is_served_with_or_without_leading_dot_slash() {
    with_built_project(|_, _| {
        for file in ["src/a.ts", "./src/a.ts"] {
            let resp = gm_file(file);
            assert!(!is_error(&resp), "{file}: {}", text(&resp));
            assert!(text(&resp).contains("export function alpha()"), "{file}: {}", text(&resp));
        }
    });
}

#[test]
fn absolute_path_is_refused() {
    with_built_project(|project, outside| {
        assert_refused(outside.to_str().unwrap());
        assert_refused(project.join("src/a.ts").to_str().unwrap());
    });
}

#[test]
fn parent_dir_path_is_refused() {
    with_built_project(|_, _| {
        assert_refused("../outside.ts");
        assert_refused("src/../../outside.ts");
    });
}

#[cfg(unix)]
#[test]
fn symlink_leading_out_of_the_project_is_refused() {
    with_built_project(|_, _| {
        assert_refused("src/outside.ts");
    });
}

#[cfg(unix)]
#[test]
fn symlink_to_a_dotfile_or_gitignored_file_is_refused() {
    with_built_project(|_, _| {
        assert_refused("src/env.ts");
        assert_refused("src/secret_link.ts");
    });
}

#[test]
fn unindexed_file_inside_the_project_is_refused_with_a_hint() {
    with_built_project(|_, _| {
        for file in [".env", "secret.ts"] {
            let text = assert_refused(file);
            assert!(text.contains("relative to the project root"), "{file}: {text}");
            assert!(text.contains("gm_outline and gm_search report indexed paths"), "{file}: {text}");
        }
    });
}

#[test]
fn unknown_project_gets_the_shared_message() {
    with_built_project(|_, _| {
        let resp = dispatch_tool("gm_file", &json!({ "file": "src/a.ts", "project": "no-such-project" }));
        assert!(is_error(&resp), "{resp}");
        assert_eq!(text(&resp), "Project 'no-such-project' is not registered (see `graphmind list`) Registered: alpha");
    });
}

/// The server still answers: an indexed file untouched by the test is served.
fn assert_still_serving() {
    let resp = gm_file("b.ts");
    assert!(!is_error(&resp) && text(&resp).contains("export function beta()"), "{}", text(&resp));
}

#[cfg(unix)]
#[test]
fn symlink_to_an_indexed_file_is_served() {
    with_built_project(|_, _| {
        let resp = gm_file("src/alias.ts");
        assert!(!is_error(&resp), "{}", text(&resp));
        assert!(text(&resp).contains("export function alpha()"), "{}", text(&resp));
    });
}

#[cfg(unix)]
#[test]
fn indexed_file_replaced_by_a_symlink_to_an_unindexed_file_is_refused() {
    use std::os::unix::fs::symlink;
    with_built_project(|project, _| {
        for target in [".env", "secret.ts"] {
            std::fs::remove_file(project.join("src/a.ts")).unwrap();
            symlink(project.join(target), project.join("src/a.ts")).unwrap();
            assert_refused("src/a.ts");
            assert_still_serving();
        }
    });
}

#[cfg(unix)]
#[test]
fn indexed_directory_replaced_by_a_symlink_to_a_dot_directory_is_refused() {
    with_built_project(|project, _| {
        std::fs::rename(project.join("src"), project.join("src_before")).unwrap();
        std::os::unix::fs::symlink(".hidden", project.join("src")).unwrap();
        assert_refused("src/a.ts");
        assert_still_serving();
    });
}

#[cfg(unix)]
#[test]
fn indexed_file_replaced_by_a_fifo_is_refused_without_blocking() {
    with_built_project(|project, _| {
        let fifo = project.join("src/a.ts");
        std::fs::remove_file(&fifo).unwrap();
        let status = std::process::Command::new("mkfifo").arg(&fifo).status().unwrap();
        assert!(status.success(), "mkfifo failed");

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(gm_file("src/a.ts")).ok());
        let resp = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(resp) => resp,
            Err(_) => {
                // Opening the write end gives the blocked reader an EOF so its thread ends.
                drop(std::fs::OpenOptions::new().write(true).open(&fifo));
                panic!("gm_file blocked on a FIFO");
            }
        };
        assert!(is_error(&resp), "a FIFO must be refused, got: {}", text(&resp));
        assert_still_serving();
    });
}
