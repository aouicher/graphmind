//! Scripts named without an extension (`bin/deploy`, a repo's CLI) are indexed by their `#!`
//! line; other extensionless files are left out, and reading the line never leaves the project
//! or blocks on a special file.
#![cfg(unix)]

use graphmind_db::builder::{BuildOptions, GraphBuilder};
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// Builds `project` into a fresh database under `work` (on another thread, so a build that
/// blocks fails the test instead of hanging it) and returns the indexed (file, symbol) pairs.
fn build_and_list(work: &Path, project: &Path) -> Vec<(String, String)> {
    let db_path = work.join("graph.db");
    let cache_dir = work.join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let (tx, rx) = mpsc::channel();
    let (db, cache, proj) = (
        db_path.to_string_lossy().to_string(),
        cache_dir.to_string_lossy().to_string(),
        project.to_string_lossy().to_string(),
    );
    std::thread::spawn(move || {
        let mut builder = GraphBuilder::new(&db, &cache);
        builder.build(&proj, &BuildOptions { full: true, ..Default::default() });
        tx.send(()).ok();
    });
    rx.recv_timeout(Duration::from_secs(60)).expect("build did not finish");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut stmt = conn.prepare("SELECT file, name FROM symbols ORDER BY file, name").unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn has(rows: &[(String, String)], file: &str, name: &str) -> bool {
    rows.iter().any(|(f, n)| f == file && n == name)
}

#[test]
fn extensionless_shell_scripts_are_indexed_by_their_shebang() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("bin")).unwrap();
    std::fs::write(project.join("mycli"), "#!/bin/bash\nmain_cmd() {\n  echo hi\n}\n").unwrap();
    std::fs::write(project.join("bin/deploy"), "#!/usr/bin/env sh\ndeploy_step() { :; }\n").unwrap();
    std::fs::write(project.join("bin/crlf"), "#!/usr/bin/env -S zsh -e\r\ncrlf_fn() { :; }\r\n").unwrap();
    // Not shell scripts: no `#!`, or another interpreter.
    std::fs::write(project.join("NOTES"), "plain_fn() { :; }\n").unwrap();
    std::fs::write(project.join("bin/tool"), "#!/usr/bin/env python3\ndef py_fn():\n    pass\n").unwrap();

    let rows = build_and_list(&dir.path().join("work"), &project);
    assert!(has(&rows, "mycli", "main_cmd"), "{rows:?}");
    assert!(has(&rows, "bin/deploy", "deploy_step"), "{rows:?}");
    assert!(has(&rows, "bin/crlf", "crlf_fn"), "a CRLF first line still reads: {rows:?}");
    assert!(rows.iter().all(|(f, _)| f != "NOTES" && f != "bin/tool"), "only shell scripts: {rows:?}");
}

#[test]
fn a_linked_script_is_read_only_when_it_resolves_inside_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(dir.path().join("outside"), "#!/bin/sh\noutside_secret_fn() { :; }\n").unwrap();
    symlink(dir.path().join("outside"), project.join("leak")).unwrap();

    let rows = build_and_list(&dir.path().join("work"), &project);
    assert!(rows.iter().all(|(f, n)| f != "leak" && n != "outside_secret_fn"), "content outside the project leaked: {rows:?}");
}

#[test]
fn an_extensionless_fifo_neither_blocks_nor_is_indexed() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("run"), "#!/bin/sh\nrun_fn() { :; }\n").unwrap();
    let fifo = project.join("pipe");
    let status = std::process::Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(status.success());

    let rows = build_and_list(&dir.path().join("work"), &project);
    assert!(has(&rows, "run", "run_fn"), "{rows:?}");
    assert!(rows.iter().all(|(f, _)| f != "pipe"), "{rows:?}");
}
