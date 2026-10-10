//! E2E tests for the shell PATH step of `graphmind setup`, and for the symlink check that decides
//! whether PATH already reaches the binary. A profile line is added once, only when PATH doesn't
//! reach the binary; a profile that can't be read or backed up is left as it is.
//!
//! Run with:
//!   cargo test -p graphmind-cli --test setup_shell_path_e2e -- --test-threads=1

use common::{run_setup_as, skip_as_root, with_home};
use graphmind_cli::commands::setup::{already_on_path, install_shell_path_for};
use std::fs;
use std::path::Path;

mod common;

// ---------------------------------------------------------------------------
// 1. Shell PATH
// ---------------------------------------------------------------------------

#[test]
fn setup_shell_path_idempotent() {
    with_home(|home| {
        // Not on PATH, zsh: written once, to .zshenv only.
        let bin_dir = home.join("opt").join("bin");
        install_shell_path_for(&bin_dir.join("graphmind"), "/usr/bin:/bin", "/bin/zsh");
        install_shell_path_for(&bin_dir.join("graphmind"), "/usr/bin:/bin", "/bin/zsh");

        let zshenv = home.join(".zshenv");
        let content = fs::read_to_string(&zshenv).expect(".zshenv should be created");
        let entry = format!("\"{}:$PATH\"", bin_dir.display());
        let count = content.matches(&entry).count();
        assert_eq!(count, 1, "export line should appear exactly once, found {count}:\n{content}");
        assert!(!home.join(".zshrc").exists(), ".zshrc must not be touched for zsh");
    });
}

#[test]
fn setup_shell_path_skips_dir_already_on_path() {
    with_home(|home| {
        let bin_dir = home.join(".local").join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let path_env = format!("/usr/bin:{}", bin_dir.display());
        install_shell_path_for(&bin_dir.join("graphmind"), &path_env, "/bin/zsh");
        assert!(!home.join(".zshenv").exists(), "nothing to add when the binary is already on PATH");
    });
}

#[test]
fn setup_shell_path_removes_dead_legacy_line() {
    with_home(|home| {
        // Older setups always added ~/.graphmind/bin, even when the binary lived elsewhere.
        let legacy = format!("export PATH=\"{}:$PATH\"", home.join(".graphmind/bin").display());
        fs::write(home.join(".zshrc"), format!("alias ll='ls -la'\n{legacy}\n")).unwrap();
        fs::write(home.join(".bashrc"), format!("{legacy}\n")).unwrap();

        let bin_dir = home.join(".local").join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        install_shell_path_for(&bin_dir.join("graphmind"), &bin_dir.display().to_string(), "/bin/zsh");

        assert_eq!(fs::read_to_string(home.join(".zshrc")).unwrap(), "alias ll='ls -la'\n");
        assert!(!fs::read_to_string(home.join(".bashrc")).unwrap().contains(".graphmind/bin"));
    });
}

/// `.bak` files in `dir` whose name starts with `name`.
fn profile_backups(dir: &std::path::Path, name: &str) -> Vec<std::path::PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(&format!("{name}.")))
        .filter(|p| p.extension().is_some_and(|ext| ext == "bak"))
        .collect()
}

/// A profile that isn't UTF-8 would be replaced by the export line alone: it is left as is, and
/// setup says so. Runs setup as a command (the setup the desktop app spawns: graphmind not on PATH).
#[test]
fn setup_shell_path_leaves_a_non_utf8_profile_alone() {
    let home = tempfile::tempdir().unwrap();
    let zshenv = home.path().join(".zshenv");
    let content = b"# caf\xe9 au lait\nexport EDITOR=vim\n";
    fs::write(&zshenv, content).unwrap();

    let out = run_setup_as(Path::new(env!("CARGO_BIN_EXE_graphmind")), home.path(), "/bin/zsh");
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert_eq!(fs::read(&zshenv).unwrap(), content, ".zshenv changed\n{stdout}");
    assert!(profile_backups(home.path(), ".zshenv").is_empty());
    let line = stdout.lines().find(|l| l.contains("⊘") && l.contains(&*zshenv.to_string_lossy()));
    assert!(line.is_some_and(|l| l.contains("left unchanged")), "no left-as-is message:\n{stdout}");
}

/// The reported case: a `.zshenv` with the dead `~/.graphmind/bin` line. Setup backs it up before
/// removing that line, and again before appending the new one. Both backups must survive, so the
/// one holding the file as it was before setup isn't overwritten by the second.
#[test]
fn setup_keeps_the_profile_from_before_setup_when_it_backs_up_twice() {
    let home = tempfile::tempdir().unwrap();
    let zshenv = home.path().join(".zshenv");
    let original = format!(
        "alias ll='ls -la'\nexport PATH=\"{}:$PATH\"\n",
        home.path().join(".graphmind/bin").display()
    );
    fs::write(&zshenv, &original).unwrap();

    let out = run_setup_as(Path::new(env!("CARGO_BIN_EXE_graphmind")), home.path(), "/bin/zsh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "setup failed\n{stdout}");

    let backups = profile_backups(home.path(), ".zshenv");
    assert_eq!(backups.len(), 2, "one backup before the removal, one before the append: {backups:?}");
    assert!(
        backups.iter().any(|b| fs::read_to_string(b).unwrap() == original),
        "no backup holds .zshenv as it was before setup: {backups:?}"
    );
    let content = fs::read_to_string(&zshenv).unwrap();
    assert!(!content.contains(".graphmind/bin"), "dead line still there:\n{content}");
    assert!(content.contains("# added by graphmind setup"), "new line missing:\n{content}");
}

/// A profile that can't be backed up is not rewritten, and the summary names it. The home folder
/// is read-only once the profile exists, so the backup next to it can't be created.
#[cfg(unix)]
#[test]
fn setup_shell_path_leaves_a_profile_alone_when_it_cannot_be_backed_up() {
    use std::os::unix::fs::PermissionsExt;
    let test = "setup_shell_path_leaves_a_profile_alone_when_it_cannot_be_backed_up";
    if skip_as_root(test) {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let zshenv = home.path().join(".zshenv");
    let content = "export EDITOR=vim\n";
    fs::write(&zshenv, content).unwrap();
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o555)).unwrap();

    let out = run_setup_as(Path::new(env!("CARGO_BIN_EXE_graphmind")), home.path(), "/bin/zsh");

    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(fs::read_to_string(&zshenv).unwrap(), content, ".zshenv changed\n{stdout}");
    let line = stdout.lines().find(|l| l.contains("⊘") && l.contains(".zshenv"));
    assert!(line.is_some_and(|l| l.contains("can't back up") && l.contains("left unchanged")), "{stdout}");
}

#[test]
fn setup_shell_path_backs_up_an_existing_profile_before_appending() {
    with_home(|home| {
        let zshenv = home.join(".zshenv");
        fs::write(&zshenv, "export EDITOR=vim\n").unwrap();
        let bin_dir = home.join("opt").join("bin");

        install_shell_path_for(&bin_dir.join("graphmind"), "/usr/bin:/bin", "/bin/zsh");

        let backups = profile_backups(home, ".zshenv");
        assert_eq!(backups.len(), 1, "{backups:?}");
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), "export EDITOR=vim\n");
        let content = fs::read_to_string(&zshenv).unwrap();
        assert!(content.starts_with("export EDITOR=vim\nexport PATH="), "{content}");
    });
}

#[test]
fn setup_shell_path_creates_a_missing_profile() {
    with_home(|home| {
        let bin_dir = home.join("opt").join("bin");
        install_shell_path_for(&bin_dir.join("graphmind"), "/usr/bin:/bin", "/bin/bash");

        let content = fs::read_to_string(home.join(".bashrc")).expect(".bashrc should be created");
        assert!(content.starts_with(&format!("export PATH=\"{}:$PATH\"", bin_dir.display())), "{content}");
        assert_eq!(content.lines().count(), 1, "{content}");
        assert!(profile_backups(home, ".bashrc").is_empty(), "nothing to back up");
    });
}

/// Linuxbrew / symlinked dev builds: the binary runs from a versioned directory that isn't on
/// PATH, but the link to it is. That counts as "on PATH", so setup adds nothing.
#[cfg(unix)]
#[test]
fn symlinked_binary_on_path_counts_as_on_path() {
    let dir = tempfile::tempdir().unwrap();
    let cellar = dir.path().join("Cellar/graphmind/0.3.2/bin");
    let linked = dir.path().join("linuxbrew/bin");
    let other = dir.path().join("other/bin");
    for d in [&cellar, &linked, &other] {
        std::fs::create_dir_all(d).unwrap();
    }
    let exe = cellar.join("graphmind");
    std::fs::write(&exe, "binary").unwrap();
    std::os::unix::fs::symlink(&exe, linked.join("graphmind")).unwrap();
    std::fs::write(other.join("graphmind"), "a different graphmind").unwrap();

    let path_env = format!("/usr/bin:{}:{}", other.display(), linked.display());
    assert_eq!(already_on_path(&exe, &path_env), Some(linked.join("graphmind")));
    // Another graphmind on PATH, or none, doesn't count.
    assert_eq!(already_on_path(&exe, &format!("/usr/bin:{}", other.display())), None);
    assert_eq!(already_on_path(&exe, "/usr/bin:/bin"), None);
}

/// The whole flow for a symlinked install: the link on PATH is enough, no profile is written.
#[cfg(unix)]
#[test]
fn setup_shell_path_writes_nothing_for_a_linked_binary() {
    with_home(|home| {
        let cellar = home.join("Cellar/graphmind/0.3.2/bin");
        let linked = home.join("linuxbrew/bin");
        fs::create_dir_all(&cellar).unwrap();
        fs::create_dir_all(&linked).unwrap();
        let exe = cellar.join("graphmind");
        fs::write(&exe, "binary").unwrap();
        std::os::unix::fs::symlink(&exe, linked.join("graphmind")).unwrap();

        install_shell_path_for(&exe, &format!("/usr/bin:{}", linked.display()), "/bin/zsh");
        assert!(!home.join(".zshenv").exists(), "the versioned Cellar dir must not be added");
    });
}
