//! Atomic file replacement shared by config, setup and hook installation.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Numbers each call's temp file, so threads of one process writing the same file don't share it.
static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Backups of one file made in the same second: `<name>.<ts>.bak`, then `-1`, `-2`, ... before
/// `.bak`. Past this many, `back_up` gives up rather than loop forever.
const MAX_BACKUPS_PER_SECOND: u32 = 100;

/// Copy `path` to a new backup next to it, `<name>.<UTC timestamp>.bak`, and return its path.
/// It never overwrites an existing backup: a name already taken gets a suffix, so the older
/// backup, the state before the first change, is kept. The copy keeps the file's permissions.
/// `Err` names the file and the reason, and leaves no partial backup behind.
pub fn back_up(path: &Path) -> Result<PathBuf, String> {
    let fail = |e: std::io::Error| format!("can't back up {} ({e}); left unchanged", path.display());
    let mut source = fs::File::open(path).map_err(fail)?;
    let perms = source.metadata().map_err(fail)?.permissions();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    for n in 0..MAX_BACKUPS_PER_SECOND {
        let suffix = if n == 0 { String::new() } else { format!("-{n}") };
        let backup = path.with_file_name(format!("{name}.{ts}{suffix}.bak"));
        let mut dest = match fs::OpenOptions::new().write(true).create_new(true).open(&backup) {
            Ok(dest) => dest,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(fail(e)),
        };
        let copied = dest
            .set_permissions(perms.clone())
            .and_then(|()| std::io::copy(&mut source, &mut dest).map(drop));
        return match copied {
            Ok(()) => Ok(backup),
            Err(e) => {
                fs::remove_file(&backup).ok();
                Err(fail(e))
            }
        };
    }
    Err(format!("can't back up {} (too many backups this second); left unchanged", path.display()))
}

/// Replace `path`'s content atomically: a temp file in the same directory, written with the
/// target's permissions already set, synced, then renamed over the target. A symlink is
/// followed, so a dotfile-managed link stays a link and its target is updated. Readers see the
/// old file or the new one, never a truncated one.
pub fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let seq = WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = target.with_file_name(format!(".{name}.tmp.{}.{seq}", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        if let Ok(meta) = fs::metadata(&target) {
            file.set_permissions(meta.permissions())?;
        }
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&tmp, &target)
    })();
    if result.is_err() {
        fs::remove_file(&tmp).ok();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{back_up, write_atomic};
    use std::fs;

    /// Two backups in the same second (a profile backed up before a removal and again before the
    /// append) both exist: the second gets a suffix, so the first, the original state, survives.
    #[test]
    fn back_up_never_overwrites_a_backup_from_the_same_second() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        fs::write(&path, "original\n").unwrap();
        let first = back_up(&path).unwrap();
        fs::write(&path, "changed\n").unwrap();
        let second = back_up(&path).unwrap();

        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(&first).unwrap(), "original\n");
        assert_eq!(fs::read_to_string(&second).unwrap(), "changed\n");
        assert!(first.file_name().unwrap().to_string_lossy().starts_with(".zshenv."));
        assert!(first.extension().is_some_and(|e| e == "bak"));
    }

    #[test]
    fn back_up_of_a_missing_file_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.json");
        let err = back_up(&path).unwrap_err();
        assert!(err.contains("missing.json") && err.contains("left unchanged"), "{err}");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0, "no partial backup");
    }

    #[cfg(unix)]
    #[test]
    fn back_up_keeps_the_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.json");
        fs::write(&path, "{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let backup = back_up(&path).unwrap();
        assert_eq!(fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
    }

    /// Config files are often managed with dotfile tools: a symlink stays a link, its target gets
    /// the new content with its permissions kept, and no temp file is left behind.
    #[cfg(unix)]
    #[test]
    fn config_writes_keep_symlink_and_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles-claude.json");
        fs::write(&real, "old").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join(".claude.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        write_atomic(&link, b"new").unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "link kept");
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2, "temp file left behind");
    }

    /// Threads of one process (the desktop app, the MCP server) can save the same file at once:
    /// each write gets its own temp file, so every call succeeds and the result is one whole
    /// write, never a mix of two.
    #[test]
    fn concurrent_writes_in_one_process_do_not_mix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "").unwrap();
        let contents: Vec<Vec<u8>> = (0..8u8).map(|i| vec![b'a' + i; 16 * 1024]).collect();
        std::thread::scope(|scope| {
            for content in &contents {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..20 {
                        write_atomic(path, content).unwrap();
                    }
                });
            }
        });
        let written = fs::read(&path).unwrap();
        assert!(contents.contains(&written), "config holds a mix of writes");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "temp file left behind");
    }
}
