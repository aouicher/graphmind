#![allow(dead_code)]

use graphmind_cli::commands::setup::{install_mcp_client, repair_claude_settings, McpClient, Outcome};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, OnceLock};

pub static TEST_LOCK: Mutex<()> = Mutex::new(());

/// `graphmind setup` run as a command: `exe` in `home`, with `shell` as $SHELL and only the system
/// directories on PATH, so no `claude` or `opencode` from the machine running the tests is used.
/// The caller checks the status and reads stdout.
pub fn run_setup_as(exe: &Path, home: &Path, shell: &str) -> Output {
    Command::new(exe)
        .arg("setup")
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("SHELL", shell)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("NO_COLOR", "1")
        .env("GRAPHMIND_SKIP_NOTICES", "1")
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .output()
        .expect("failed to run graphmind setup")
}

/// True when running as root, where file permissions don't stop writes. A test that needs them
/// returns early when this is true, and the skip is printed so it can't pass unnoticed.
pub fn skip_as_root(test: &str) -> bool {
    let root = Command::new("id").arg("-u").output().is_ok_and(|out| out.stdout == b"0\n");
    if root {
        eprintln!("{test} skipped: running as root, so file permissions don't apply");
    }
    root
}

/// Setup's Claude Code step: register at user scope, then repair settings.json.
pub fn claude_code_step() -> Outcome {
    let outcome = install_mcp_client(&McpClient::ClaudeCodeUser);
    repair_claude_settings(outcome.is_in_place());
    outcome
}

/// A fake `claude` CLI in `<home>/bin`: logs its arguments to `<home>/claude-calls.log`, then runs
/// `script_body`.
#[cfg(unix)]
pub fn fake_claude(home: &Path, script_body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("claude");
    std::fs::write(&script, format!("#!/bin/sh\necho \"$*\" >> \"$HOME/claude-calls.log\"\n{script_body}\n")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Writes ~/.claude.json (in `$CLAUDE_CONFIG_DIR` when that is set) the way `claude mcp add -s
/// user` does (with an empty `env`).
#[cfg(unix)]
pub const FAKE_CLAUDE_OK: &str = r#"file="${CLAUDE_CONFIG_DIR:-$HOME}/.claude.json"
case "$2" in
  add) printf '{"mcpServers":{"graphmind":{"type":"stdio","command":"%s","args":["mcp"],"env":{}}}}' "$7" > "$file" ;;
  remove) printf '{"mcpServers":{}}' > "$file" ;;
esac"#;

/// For suites that call setup functions in-process: create a fresh temp HOME, point `HOME` at it,
/// run `f`, then drop. PATH holds the system directories plus `<home>/bin` (empty unless a test
/// puts a fake CLI there), so the `claude` and `opencode` installed on the machine running the
/// tests are never used.
pub fn with_home<F: FnOnce(&std::path::Path)>(f: F) {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let path = std::env::join_paths([dir.path().join("bin"), "/usr/bin".into(), "/bin".into()]).unwrap();
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("PATH", path);
        // Claude Code's config would move there, out of the temp HOME.
        std::env::remove_var("CLAUDE_CONFIG_DIR");
    }
    f(dir.path());
    // dir dropped here — content cleaned up, HOME still points at it but that's harmless.
}

/// `.bak` files next to `file`.
pub fn backups_of(file: &std::path::Path) -> usize {
    let name = file.file_name().unwrap().to_string_lossy().to_string();
    let stem = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem).to_string();
    std::fs::read_dir(file.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with(&stem) && n.ends_with(".bak")
        })
        .count()
}

pub struct CliTestEnv {
    pub home: tempfile::TempDir,
    pub binary: PathBuf,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl CliTestEnv {
    pub fn new() -> Self {
        let lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("HOME", home.path()) };
        // A `claude` subprocess would write to CLAUDE_CONFIG_DIR instead of the temp HOME.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

        let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap() // crates/
            .parent()
            .unwrap() // workspace root
            .join("target/debug/graphmind");

        let bin_dir = home.path().join(".graphmind").join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&binary, bin_dir.join("graphmind")).ok();

        Self {
            home,
            binary,
            _lock: lock,
        }
    }

    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(&self.binary)
            .args(args)
            .env("HOME", self.home.path())
            .env_remove("CLAUDE_CONFIG_DIR")
            .env("GRAPHMIND_SKIP_NOTICES", "1")
            // A background build outliving the command would race the test's assertions.
            .env("GRAPHMIND_NO_AUTO_BUILD", "1")
            .output()
            .expect("failed to run graphmind")
    }

    /// Like `run`, but executes with the given directory as cwd — needed
    /// for tests that exercise cwd-based resolution (e.g. an unregistered
    /// worktree), since `resolve_project_slug` reads `current_dir()`.
    pub fn run_in(&self, dir: &std::path::Path, args: &[&str]) -> Output {
        Command::new(&self.binary)
            .args(args)
            .current_dir(dir)
            .env("HOME", self.home.path())
            .env_remove("CLAUDE_CONFIG_DIR")
            .env("GRAPHMIND_SKIP_NOTICES", "1")
            // A background build outliving the command would race the test's assertions.
            .env("GRAPHMIND_NO_AUTO_BUILD", "1")
            .output()
            .expect("failed to run graphmind")
    }

    pub fn run_ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            out.status.success(),
            "command {:?} failed\nstdout: {stdout}\nstderr: {stderr}",
            args
        );
        stdout
    }

    pub fn run_ok_in(&self, dir: &std::path::Path, args: &[&str]) -> String {
        let out = self.run_in(dir, args);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            out.status.success(),
            "command {:?} in {:?} failed\nstdout: {stdout}\nstderr: {stderr}",
            args, dir
        );
        stdout
    }

    pub fn fixture_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures/sample-project")
    }

    /// Register fixture project and build graph, returns slug.
    /// Use sparingly — each call invokes the binary twice.
    pub fn setup_project(&self) -> String {
        let fixture = self.fixture_path();
        self.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "test-fixture"]);
        self.run_ok(&["build", "test-fixture"]);
        "test-fixture".to_string()
    }
}

/// A shared pre-built graph environment for query-heavy test suites.
/// The graph is built once and reused across all tests in the same binary.
/// Each test gets its OWN CliTestEnv (own HOME) but the graph data is
/// pre-copied from the shared build into the fresh HOME.
pub struct SharedProject {
    /// Path to the pre-built ~/.graphmind dir (inside the OnceLock tempdir)
    pub graphmind_dir: PathBuf,
    /// The fixture project path
    pub fixture_path: PathBuf,
    /// The slug used
    pub slug: String,
    /// config.json content (pre-serialized)
    pub config_json: String,
    // Keep the tempdir alive for the lifetime of the process
    _dir: std::sync::Arc<tempfile::TempDir>,
}

static SHARED_PROJECT: OnceLock<SharedProject> = OnceLock::new();

impl SharedProject {
    /// Build the shared project once. Subsequent calls return the cached result.
    pub fn get() -> &'static SharedProject {
        SHARED_PROJECT.get_or_init(|| {
            let dir = tempfile::tempdir().unwrap();
            let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent().unwrap()
                .parent().unwrap()
                .join("target/debug/graphmind");

            let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent().unwrap()
                .parent().unwrap()
                .join("tests/fixtures/sample-project");

            let slug = "test-fixture".to_string();
            let graphmind_dir = dir.path().join(".graphmind");
            std::fs::create_dir_all(&graphmind_dir).unwrap();

            // Symlink binary into bin dir
            let bin_dir = graphmind_dir.join("bin");
            std::fs::create_dir_all(&bin_dir).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&binary, bin_dir.join("graphmind")).ok();

            // Run register + build once
            let out = Command::new(&binary)
                .args(["register", fixture_path.to_str().unwrap(), "--slug", &slug])
                .env("HOME", dir.path())
                .env_remove("CLAUDE_CONFIG_DIR")
                .env("GRAPHMIND_SKIP_NOTICES", "1")
                .output()
                .expect("register failed");
            assert!(out.status.success(), "shared register failed: {}", String::from_utf8_lossy(&out.stderr));

            let out = Command::new(&binary)
                .args(["build", &slug])
                .env("HOME", dir.path())
                .env_remove("CLAUDE_CONFIG_DIR")
                .env("GRAPHMIND_SKIP_NOTICES", "1")
                .output()
                .expect("build failed");
            assert!(out.status.success(), "shared build failed: {}", String::from_utf8_lossy(&out.stderr));

            // Read the generated config so we can inject it into per-test HOMEs
            let config_json = std::fs::read_to_string(graphmind_dir.join("config.json"))
                .unwrap_or_else(|_| "{}".to_string());

            SharedProject {
                graphmind_dir,
                fixture_path,
                slug,
                config_json,
                _dir: std::sync::Arc::new(dir),
            }
        })
    }
}

/// A test environment that reuses a pre-built graph from SharedProject.
/// The graph DB is copied into a fresh HOME so tests are isolated but fast.
pub struct QueryTestEnv {
    pub home: tempfile::TempDir,
    pub binary: PathBuf,
    pub slug: String,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl QueryTestEnv {
    pub fn new() -> Self {
        let shared = SharedProject::get();
        let lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();

        // Copy the entire .graphmind dir from the shared build into this HOME
        copy_dir_all(&shared.graphmind_dir, &home.path().join(".graphmind")).unwrap();

        // Rewrite config.json to use this home's path (paths are absolute in config)
        // The simplest approach: replace the shared home path with this home path in config
        let shared_home_str = shared.graphmind_dir.parent().unwrap().to_str().unwrap().to_string();
        let new_home_str = home.path().to_str().unwrap().to_string();
        let new_config = shared.config_json.replace(&shared_home_str, &new_home_str);
        std::fs::write(home.path().join(".graphmind").join("config.json"), &new_config).unwrap();

        unsafe { std::env::set_var("HOME", home.path()) };
        // A `claude` subprocess would write to CLAUDE_CONFIG_DIR instead of the temp HOME.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

        let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .parent().unwrap()
            .join("target/debug/graphmind");

        QueryTestEnv {
            home,
            binary,
            slug: shared.slug.clone(),
            _lock: lock,
        }
    }

    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(&self.binary)
            .args(args)
            .env("HOME", self.home.path())
            .env_remove("CLAUDE_CONFIG_DIR")
            .env("GRAPHMIND_SKIP_NOTICES", "1")
            // A background build outliving the command would race the test's assertions.
            .env("GRAPHMIND_NO_AUTO_BUILD", "1")
            .output()
            .expect("failed to run graphmind")
    }

    pub fn run_ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            out.status.success(),
            "command {:?} failed\nstdout: {stdout}\nstderr: {stderr}",
            args
        );
        stdout
    }
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
