//! Giving a new worktree a graph without a full build.
//!
//! Every path in a graph is relative to the project root and the build cache is
//! keyed by file content, so a copy of another worktree's graph is a valid
//! starting point: the next incremental build re-parses only the files that
//! differ between the two checkouts.

use std::fs::{self, File};
use std::path::Path;
use std::sync::OnceLock;

use crate::config::ProjectConfig;
use crate::paths;

fn open_build_lock(slug: &str) -> Option<File> {
    let dir = paths::graph_dir(slug);
    fs::create_dir_all(&dir).ok();
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join("build.lock"))
        .ok()
}

/// Exclusive per-project build lock, held until the returned file is dropped. Git hooks start
/// builds in the background (commit, merge, rebase, checkout can fire in quick succession);
/// two builds writing the same graph at once could insert duplicate symbols. A second build
/// waits for the first, then has little left to do.
pub fn lock_build(slug: &str) -> Option<File> {
    use fs4::FileExt;
    let file = open_build_lock(slug)?;
    if FileExt::try_lock(&file).is_err() {
        eprintln!("Waiting for another build of {slug} to finish...");
        FileExt::lock(&file).ok()?;
    }
    Some(file)
}

/// [`lock_build`] without waiting: `None` while another process builds `slug`.
pub fn try_lock_build(slug: &str) -> Option<File> {
    use fs4::FileExt;
    let file = open_build_lock(slug)?;
    FileExt::try_lock(&file).ok()?;
    Some(file)
}

/// Copies `src`'s graph, build cache and embeddings to `dst`, which must have no graph
/// yet. The caller holds `dst`'s build lock. Skipped (returns false) when `src` has no
/// graph or is being built right now — a graph caught mid-build would not match its
/// cache — in which case `dst` simply gets a full build.
pub fn seed_graph(dst: &str, src: &str) -> bool {
    use fs4::FileExt;
    let src_db = paths::graph_db_path(src);
    let dst_db = paths::graph_db_path(dst);
    if dst == src || !src_db.exists() || dst_db.exists() {
        return false;
    }
    let Some(src_lock) = open_build_lock(src) else {
        return false;
    };
    if FileExt::try_lock(&src_lock).is_err() {
        return false;
    }

    fs::create_dir_all(paths::cache_dir_path(dst)).ok();
    let src_hashes = paths::cache_dir_path(src).join("hashes.json");
    let dst_hashes = paths::cache_dir_path(dst).join("hashes.json");
    let src_embeddings = paths::embedding_db_path(src);

    // The graph goes last: its presence is what marks `dst` as seeded.
    let ok = (!src_hashes.exists() || copy_atomic(&src_hashes, &dst_hashes))
        && (!src_embeddings.exists() || snapshot(&src_embeddings, &paths::embedding_db_path(dst)))
        && snapshot(&src_db, &dst_db);
    if !ok {
        for leftover in [dst_db, dst_hashes, paths::embedding_db_path(dst)] {
            fs::remove_file(leftover).ok();
        }
    }
    let _ = src_lock.unlock();
    ok
}

/// A consistent single-file copy of a (WAL-mode) SQLite database, moved into place in one rename.
fn snapshot(src: &Path, dst: &Path) -> bool {
    let tmp = dst.with_extension("db.seed");
    fs::remove_file(&tmp).ok();
    let copied = rusqlite::Connection::open(src)
        .and_then(|conn| conn.execute("VACUUM INTO ?1", [tmp.to_string_lossy().as_ref()]))
        .is_ok();
    if copied && fs::rename(&tmp, dst).is_ok() {
        return true;
    }
    fs::remove_file(&tmp).ok();
    false
}

fn copy_atomic(src: &Path, dst: &Path) -> bool {
    let tmp = dst.with_extension("json.seed");
    if fs::copy(src, &tmp).is_ok() && fs::rename(&tmp, dst).is_ok() {
        return true;
    }
    fs::remove_file(&tmp).ok();
    false
}

static ON_LINK: OnceLock<fn(&ProjectConfig)> = OnceLock::new();

/// Sets what runs after a worktree is registered automatically (see
/// [`crate::Registry::link_worktree`]): the CLI and MCP server start a background
/// build so the seeded graph catches up with the worktree's branch.
pub fn on_worktree_linked(f: fn(&ProjectConfig)) {
    let _ = ON_LINK.set(f);
}

pub(crate) fn worktree_linked(project: &ProjectConfig) {
    if let Some(f) = ON_LINK.get() {
        f(project);
    }
}

/// Starts `graphmind build <slug>` detached from this process, using this process's
/// own executable. For hosts whose executable *is* the graphmind CLI. Disabled by
/// `GRAPHMIND_NO_AUTO_BUILD`.
pub fn spawn_background_build(project: &ProjectConfig) {
    if std::env::var_os("GRAPHMIND_NO_AUTO_BUILD").is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["build", &project.slug])
        .current_dir(&project.path)
        .env("GRAPHMIND_SKIP_NOTICES", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    // Reaped from a thread so a long-running MCP server collects no zombies; a
    // short-lived CLI exits first and the build carries on without it.
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || child.wait());
    }
}
