//! Atomic file replacement shared by config, setup and hook installation.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Numbers each call's temp file, so threads of one process writing the same file don't share it.
static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

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
    use super::write_atomic;
    use std::fs;

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
