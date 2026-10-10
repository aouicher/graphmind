//! Atomic file replacement shared by config, setup and hook installation.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Replace `path`'s content atomically: a temp file in the same directory, written with the
/// target's permissions already set, synced, then renamed over the target. A symlink is
/// followed, so a dotfile-managed link stays a link and its target is updated. Readers see the
/// old file or the new one, never a truncated one.
pub fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = target.with_file_name(format!(".{name}.tmp.{}", std::process::id()));
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
