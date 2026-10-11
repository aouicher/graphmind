//! Which paths count as a project's files.

use std::path::{Component, Path, PathBuf};

/// `path` with every symlink resolved, when it is a regular file inside `canonical_root`
/// (a project root already passed through [`std::fs::canonicalize`]). `None` for a path
/// that resolves outside the root, for anything that is not a regular file once resolved
/// (directory, FIFO, socket, device), and for a path that does not exist. The first check
/// the indexer applies to a symlink it walks.
pub fn project_file(canonical_root: &Path, path: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(path).ok()?;
    if !resolved.starts_with(canonical_root) {
        return None;
    }
    std::fs::metadata(&resolved).ok()?.is_file().then_some(resolved)
}

/// The file a reader may open for `file`: an indexed file of the project at `root` whose real
/// path, symlinks resolved, is inside the project directory. `file` is relative to the project
/// root (a leading `./` is accepted); absolute paths, `..`, files the graph doesn't cover and
/// symlinks that lead out are refused. The indexer skips dotfiles, files in dot-directories
/// and files that `.gitignore`, `.ignore`, the global gitignore or the exclude patterns match,
/// and indexes a symlink only when its target is a file it indexes; so as of the last build
/// none of these is indexed, directly or through a link. A link's target is checked against
/// those rules at build time; here it is only checked to be inside the project directory.
/// `is_indexed` answers whether a relative path is in the project's graph, typically
/// [`crate::queries::GraphQueries::is_indexed_file`].
///
/// The real path is checked when this is called; the caller then opens it. Swapping a path
/// component for a symlink in between needs write access to the project directory.
pub fn indexed_project_file(root: &Path, file: &str, is_indexed: impl Fn(&str) -> bool) -> Result<PathBuf, String> {
    let rel = file.strip_prefix("./").unwrap_or(file);
    let plain = !rel.is_empty() && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_)));
    if !plain || !is_indexed(rel) {
        return Err(format!("{file}: not an indexed file of this project (use a path relative to the project root)"));
    }
    let root = std::fs::canonicalize(root).map_err(|e| format!("Cannot read the project directory: {e}"))?;
    let path = std::fs::canonicalize(root.join(rel)).map_err(|e| format!("Cannot read {file}: {e}"))?;
    if !path.starts_with(&root) {
        return Err(format!("{file}: resolves outside the project directory"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{indexed_project_file, project_file};

    #[test]
    fn regular_file_inside_resolves_and_traversal_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/a.rs"), "fn a() {}").unwrap();
        std::fs::write(dir.path().join("outside.rs"), "fn secret() {}").unwrap();
        let root = std::fs::canonicalize(&root).unwrap();

        assert_eq!(project_file(&root, &root.join("src/a.rs")), Some(root.join("src/a.rs")));
        assert_eq!(project_file(&root, &root.join("../outside.rs")), None);
        assert_eq!(project_file(&root, &root.join("src")), None, "a directory is not a file");
        assert_eq!(project_file(&root, &root.join("missing.rs")), None);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_resolve_only_when_their_target_is_inside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("real.rs"), "fn real() {}").unwrap();
        std::fs::write(dir.path().join("outside.rs"), "fn secret() {}").unwrap();
        std::os::unix::fs::symlink(root.join("real.rs"), root.join("inside_link.rs")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("outside.rs"), root.join("outside_link.rs")).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();

        assert_eq!(project_file(&root, &root.join("inside_link.rs")), Some(root.join("real.rs")));
        assert_eq!(project_file(&root, &root.join("outside_link.rs")), None);
    }

    /// A project directory with an indexed `src/a.rs`, a `.env` the graph doesn't cover, and a
    /// file outside the project.
    fn project_with_outside() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/a.rs"), "fn a() {}").unwrap();
        std::fs::write(root.join(".env"), "SECRET=1").unwrap();
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, "outside").unwrap();
        (dir, root, outside)
    }

    #[test]
    fn indexed_file_inside_the_project_is_served() {
        let (_d, root, _) = project_with_outside();
        let indexed = |p: &str| p == "src/a.rs";
        let path = indexed_project_file(&root, "src/a.rs", indexed).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "fn a() {}");
        assert!(indexed_project_file(&root, "./src/a.rs", indexed).is_ok());
    }

    #[test]
    fn absolute_parent_empty_and_unindexed_paths_are_refused() {
        let (_d, root, outside) = project_with_outside();
        // Even a lookup that says yes to everything can't make these readable.
        let anything = |_: &str| true;
        assert!(indexed_project_file(&root, outside.to_str().unwrap(), anything).is_err(), "absolute path");
        assert!(indexed_project_file(&root, "../outside.txt", anything).is_err(), "parent dir");
        assert!(indexed_project_file(&root, "src/../../outside.txt", anything).is_err(), "parent dir inside");
        assert!(indexed_project_file(&root, "", anything).is_err(), "empty");
        assert!(indexed_project_file(&root, "./", anything).is_err(), "project root");
        let only_a = |p: &str| p == "src/a.rs";
        assert!(indexed_project_file(&root, ".env", only_a).is_err(), "unindexed dotfile");
    }

    #[cfg(unix)]
    #[test]
    fn indexed_symlink_leading_out_is_refused() {
        let (_d, root, outside) = project_with_outside();
        std::os::unix::fs::symlink(&outside, root.join("src/link.rs")).unwrap();
        let err = indexed_project_file(&root, "src/link.rs", |p| p == "src/link.rs").unwrap_err();
        assert!(err.contains("outside the project"), "{err}");
    }
}
