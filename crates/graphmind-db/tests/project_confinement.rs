//! The builder indexes only regular files inside the project, and links only to files it
//! indexes itself.
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

#[test]
fn symlinked_files_are_indexed_only_when_they_resolve_inside_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/real.rs"), "pub fn shared_fn() {}\n").unwrap();
    std::fs::write(dir.path().join("outside.rs"), "pub fn outside_secret_fn() {}\n").unwrap();
    symlink(project.join("src/real.rs"), project.join("src/alias.rs")).unwrap();
    symlink(dir.path().join("outside.rs"), project.join("src/leak.rs")).unwrap();

    let rows = build_and_list(&dir.path().join("work"), &project);
    assert!(rows.contains(&("src/real.rs".to_string(), "shared_fn".to_string())), "{rows:?}");
    assert!(rows.contains(&("src/alias.rs".to_string(), "shared_fn".to_string())), "a link inside the project is indexed: {rows:?}");
    assert!(rows.iter().all(|(f, n)| f != "src/leak.rs" && n != "outside_secret_fn"), "content outside the project leaked: {rows:?}");
}

#[test]
fn fifo_with_an_indexed_extension_neither_blocks_nor_is_indexed() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn inside_fn() {}\n").unwrap();
    let status = std::process::Command::new("mkfifo").arg(project.join("src/pipe.rs")).status().unwrap();
    assert!(status.success(), "mkfifo failed");

    let rows = build_and_list(&dir.path().join("work"), &project);
    assert!(rows.iter().any(|(_, n)| n == "inside_fn"), "{rows:?}");
    assert!(rows.iter().all(|(f, _)| f != "src/pipe.rs"), "{rows:?}");
}

/// Paths of the files recorded in the graph at `work/graph.db`, sorted.
fn indexed_files(work: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(work.join("graph.db")).unwrap();
    let mut stmt = conn.prepare("SELECT path FROM files ORDER BY path").unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();
    rows
}

#[test]
fn symlinks_to_dotfiles_or_ignored_files_are_not_indexed() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("secrets")).unwrap();
    std::fs::create_dir_all(project.join("vendor")).unwrap();
    std::fs::write(project.join(".gitignore"), "secrets/\n").unwrap();
    std::fs::write(project.join(".env"), "# Env\nDB_PASSWORD=env_secret\n").unwrap();
    std::fs::write(project.join("secrets/prod.toml"), "[prod]\napi_key = \"prod_secret\"\n").unwrap();
    std::fs::write(project.join("vendor/dep.rs"), "pub fn vendored_fn() {}\n").unwrap();
    std::fs::write(project.join("src/real.rs"), "pub fn shared_fn() {}\n").unwrap();
    symlink(".env", project.join("notes.md")).unwrap();
    symlink("secrets/prod.toml", project.join("conf.toml")).unwrap();
    symlink("../vendor/dep.rs", project.join("src/dep.rs")).unwrap();
    symlink("real.rs", project.join("src/alias.rs")).unwrap();

    let work = dir.path().join("work");
    let rows = build_and_list(&work, &project);
    assert_eq!(indexed_files(&work), ["src/alias.rs", "src/real.rs"]);
    assert!(rows.contains(&("src/alias.rs".to_string(), "shared_fn".to_string())), "{rows:?}");
    assert!(rows.iter().all(|(_, n)| n != "vendored_fn"), "an excluded file came in through a link: {rows:?}");
}

#[test]
fn build_fails_and_keeps_the_graph_when_the_project_directory_cannot_be_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn kept_fn() {}\n").unwrap();
    let work = dir.path().join("work");
    assert!(build_and_list(&work, &project).iter().any(|(_, n)| n == "kept_fn"));

    std::fs::remove_dir_all(&project).unwrap();
    let mut builder = GraphBuilder::new(
        &work.join("graph.db").to_string_lossy(),
        &work.join("cache").to_string_lossy(),
    );
    let result = builder.build(&project.to_string_lossy(), &BuildOptions::default());
    drop(builder);

    assert!(result.errors.iter().any(|e| e.contains("cannot read the project directory")), "{:?}", result.errors);
    assert_eq!(result.deleted, 0);
    assert_eq!(indexed_files(&work), ["src/lib.rs"], "the graph is left as it was");
}

/// Removes every permission on `dir` until dropped. None when permissions don't stop this
/// process from reading it (running as root), so the caller can skip its test.
struct Unreadable(std::path::PathBuf);

impl Unreadable {
    fn new(dir: &Path) -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let guard = Unreadable(dir.to_path_buf());
        std::fs::read_dir(dir).is_err().then_some(guard)
    }
}

impl Drop for Unreadable {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755)).ok();
    }
}

fn rebuild(work: &Path, project: &Path) -> graphmind_db::builder::BuildResult {
    let mut builder = GraphBuilder::new(
        &work.join("graph.db").to_string_lossy(),
        &work.join("cache").to_string_lossy(),
    );
    builder.build(&project.to_string_lossy(), &BuildOptions::default())
}

#[test]
fn build_fails_and_keeps_the_graph_when_the_project_directory_cannot_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn kept_fn() {}\n").unwrap();
    let work = dir.path().join("work");
    assert!(build_and_list(&work, &project).iter().any(|(_, n)| n == "kept_fn"));

    let Some(_guard) = Unreadable::new(&project) else { return };
    let result = rebuild(&work, &project);

    assert!(result.errors.iter().any(|e| e.contains("cannot read the project directory")), "{:?}", result.errors);
    assert_eq!(result.deleted, 0);
    assert_eq!(indexed_files(&work), ["src/lib.rs"], "the graph is left as it was");
}

#[test]
fn files_of_a_folder_that_cannot_be_read_stay_in_the_graph() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("data/nested")).unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn lib_fn() {}\n").unwrap();
    std::fs::write(project.join("src/gone.rs"), "pub fn gone_fn() {}\n").unwrap();
    std::fs::write(project.join("data/nested/db.rs"), "pub fn db_fn() {}\n").unwrap();
    let work = dir.path().join("work");
    build_and_list(&work, &project);
    assert_eq!(indexed_files(&work), ["data/nested/db.rs", "src/gone.rs", "src/lib.rs"]);

    std::fs::remove_file(project.join("src/gone.rs")).unwrap();
    std::fs::write(project.join("src/new.rs"), "pub fn new_fn() {}\n").unwrap();
    let Some(_guard) = Unreadable::new(&project.join("data")) else { return };
    let result = rebuild(&work, &project);

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.deleted, 1, "only the deleted file leaves the graph");
    assert_eq!(indexed_files(&work), ["data/nested/db.rs", "src/lib.rs", "src/new.rs"]);
}
