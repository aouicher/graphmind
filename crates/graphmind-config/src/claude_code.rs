//! Claude Code's per-project MCP entries in .claude.json (`projects.<key>.mcpServers.graphmind`):
//! the key Claude Code reads for a project, registering and unregistering a project's entry, setup's
//! sync of every registered project, and the uninstall sweep. Reads and writes go through
//! [`crate::mcp_clients`].

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::mcp_clients::{
    current_entry, describe, has_graphmind_entry, is_current, points_at_graphmind, read_json_config, remove,
    remove_with, server_entry, servers_mut, write_config, ConfigRead, McpClient, Outcome, SERVER_NAME,
};
use crate::paths::find_on_path;

/// The key Claude Code files `project`'s local-scope servers under in .claude.json, which is also
/// where it looks them up when started anywhere in the project:
/// - in a linked worktree (`git worktree add`) or one of its subfolders: the repository's common
///   git dir, or its parent when it is named `.git`. That is the main checkout for a regular
///   repository, the bare repository for a bare repository's worktree, and
///   `<superproject>/.git/modules/<name>` for a worktree of a submodule;
/// - elsewhere in a git work tree (a main checkout, a submodule, a `--separate-git-dir` work tree,
///   or a subfolder of one): the work tree's root, `git rev-parse --show-toplevel`;
/// - outside git: the project's own path.
///
/// Paths are canonical, symlinks resolved, as Claude Code resolves its working directory.
pub fn claude_code_key(project: &Path) -> PathBuf {
    let project = project.canonicalize().unwrap_or_else(|_| project.to_path_buf());
    match crate::git_identity::work_tree(&project) {
        Some(tree) if tree.linked => {
            let common = tree.common_dir;
            match common.parent() {
                Some(parent) if common.file_name().is_some_and(|name| name == ".git") => parent.to_path_buf(),
                _ => common,
            }
        }
        Some(tree) => tree.root,
        None => project,
    }
}

/// The distinct Claude Code keys of the registered projects on disk: one per repository, however
/// many of its worktrees and subfolders are registered.
pub fn claude_code_keys() -> Vec<PathBuf> {
    let keys: BTreeSet<PathBuf> = registered_keys().into_iter().map(|(_, key)| key).collect();
    keys.into_iter().collect()
}

/// The registered projects on disk, each with its Claude Code key.
fn registered_keys() -> Vec<(PathBuf, PathBuf)> {
    keyed(crate::Registry::list().into_iter().map(|p| PathBuf::from(p.path)))
}

fn keyed(projects: impl IntoIterator<Item = PathBuf>) -> Vec<(PathBuf, PathBuf)> {
    projects
        .into_iter()
        .filter(|path| path.is_dir())
        .map(|path| {
            let key = claude_code_key(&path);
            (path, key)
        })
        .collect()
}

/// What [`sync_claude_code_projects`] did.
#[derive(Debug)]
pub struct ClaudeCodeSync {
    /// How many distinct Claude Code keys the registered projects have (see [`claude_code_keys`]).
    pub keys: usize,
    /// Keys whose entry was added or rewritten.
    pub written: Vec<PathBuf>,
    /// Key entries already current.
    pub current: usize,
    /// Keys holding a `graphmind` entry set up by hand: left alone.
    pub hand_made: Vec<PathBuf>,
    /// Registered worktrees and subfolders whose own graphmind entry, one Claude Code never reads,
    /// was removed; their key holds the entry.
    pub moved: Vec<PathBuf>,
    /// Why an entry, or the whole file, was left unchanged (path included).
    pub left_alone: Vec<String>,
    /// The user-scope graphmind entry: `Written` when removed, `Unchanged` when there is none (or
    /// it doesn't run graphmind), `LeftAlone` when it stays.
    pub user_scope: Outcome,
}

impl ClaudeCodeSync {
    /// Every key entry graphmind looks after is current, and no user-scope graphmind entry is
    /// left.
    pub fn is_complete(&self) -> bool {
        self.left_alone.is_empty() && self.user_scope.is_in_place()
    }
}

/// Bring Claude Code's graphmind entries in .claude.json up to date, one per key of the registered
/// projects (see [`claude_code_keys`]):
/// - an entry that runs a graphmind binary but isn't the current one (a moved binary, an `env`
///   block) is rewritten; a `graphmind` entry that doesn't run graphmind is left alone;
/// - with `add_missing`, each key without an entry gets one;
/// - a graphmind entry under a registered worktree's or subfolder's own path (which Claude Code
///   never reads) is removed, with the `mcpServers` object it leaves empty, and its key gets the
///   entry;
/// - a user-scope entry that runs graphmind is removed once no key failed (with no registered
///   project too): Claude Code reads graphmind per project.
///
/// The key entries go in one write, so one backup holds the file as it was. The user-scope entry
/// goes in that write too, unless the `claude` CLI is on PATH: `claude mcp remove -s user` removes
/// it then, and if the CLI fails, the file is written again without a second backup. A .claude.json that can't be read or parsed is left alone and the reason returned.
pub fn sync_claude_code_projects(bin: &str, add_missing: bool, on_cli_fallback: &mut dyn FnMut(&str)) -> ClaudeCodeSync {
    let projects = registered_keys();
    let keys: BTreeSet<PathBuf> = projects.iter().map(|(_, key)| key.clone()).collect();
    let mut sync = ClaudeCodeSync {
        keys: keys.len(),
        written: Vec::new(),
        current: 0,
        hand_made: Vec::new(),
        moved: Vec::new(),
        left_alone: Vec::new(),
        user_scope: Outcome::Unchanged,
    };
    let user = McpClient::ClaudeCodeUser;
    let path = user.config_path();
    let mut config = match read_json_config(&path, false) {
        ConfigRead::Missing => json!({}),
        ConfigRead::Parsed { value, .. } => value,
        ConfigRead::Unparseable(reason) => {
            sync.left_alone.push(reason.clone());
            sync.user_scope = Outcome::LeftAlone(reason);
            return sync;
        }
    };

    let mut moved_to = BTreeSet::new();
    for (project, key) in &projects {
        let servers = McpClient::ClaudeCodeProject(project.clone()).servers_path();
        if project != key && server_entry(&config, &servers).is_some_and(points_at_graphmind) {
            let emptied = servers_mut(&mut config, &servers).is_some_and(|map| {
                map.remove(SERVER_NAME);
                map.is_empty()
            });
            // An empty `mcpServers` goes too; the project's other settings stay.
            let project_entry = servers[..servers.len() - 1].iter().try_fold(&mut config, |value, k| value.get_mut(k.as_str()));
            if let (true, Some(entry)) = (emptied, project_entry.and_then(Value::as_object_mut)) {
                entry.remove("mcpServers");
            }
            sync.moved.push(project.clone());
            moved_to.insert(key.clone());
        }
    }

    for key in keys {
        let client = McpClient::ClaudeCodeProject(key.clone());
        let servers = client.servers_path();
        let wanted = client.entry(bin);
        match server_entry(&config, &servers) {
            Some(entry) if is_current(entry, &wanted) => {
                sync.current += 1;
                continue;
            }
            Some(entry) if !points_at_graphmind(entry) => {
                sync.hand_made.push(key);
                continue;
            }
            None if !add_missing && !moved_to.contains(&key) => continue,
            _ => {}
        }
        match servers_mut(&mut config, &servers) {
            Some(map) => {
                map.insert(SERVER_NAME.to_string(), wanted);
                sync.written.push(key);
            }
            None => sync.left_alone.push(format!(
                "\"{}\" in {} isn't an object; left unchanged",
                servers.join("."),
                path.display()
            )),
        }
    }

    let remove_user = server_entry(&config, &user.servers_path()).is_some_and(points_at_graphmind);
    let user_in_write = remove_user && sync.left_alone.is_empty() && find_on_path("claude").is_none();
    if user_in_write {
        if let Some(servers) = config.get_mut("mcpServers").and_then(Value::as_object_mut) {
            servers.remove(SERVER_NAME);
        }
    }
    let mut wrote = false;
    if !sync.written.is_empty() || !sync.moved.is_empty() || user_in_write {
        match write_config(&path, &config) {
            Ok(()) => wrote = true,
            Err(reason) => {
                sync.written.clear();
                sync.moved.clear();
                sync.left_alone.push(reason);
            }
        }
    }
    if remove_user {
        sync.user_scope = if !sync.left_alone.is_empty() {
            Outcome::LeftAlone(format!(
                "the user-scope entry in {} stays until every project has its own",
                path.display()
            ))
        } else if user_in_write {
            Outcome::Written
        } else {
            // A direct write after a failed `claude mcp remove` reuses this run's backup.
            remove_with(&user, !wrote, on_cli_fallback)
        };
    }
    sync
}

/// Every key of the registered projects holds graphmind's current entry, or a `graphmind` entry
/// set up by hand (which graphmind leaves alone). False with no registered project, or when
/// .claude.json can't be read.
pub fn claude_code_configured(bin: &str) -> bool {
    match read_json_config(&McpClient::ClaudeCodeUser.config_path(), false) {
        ConfigRead::Parsed { value, .. } => keys_configured(&value, &claude_code_keys(), bin),
        ConfigRead::Missing | ConfigRead::Unparseable(_) => false,
    }
}

fn keys_configured(config: &Value, keys: &[PathBuf], bin: &str) -> bool {
    !keys.is_empty()
        && keys.iter().all(|key| {
            let client = McpClient::ClaudeCodeProject(key.clone());
            server_entry(config, &client.servers_path())
                .is_some_and(|entry| is_current(entry, &client.entry(bin)) || !points_at_graphmind(entry))
        })
}

/// Remove the Claude Code entry of `project`, which is no longer registered: the entry under its
/// key (see [`claude_code_key`]), kept while a registered project still maps to that key.
///
/// A folder that no longer exists can't be keyed through git: its entry is the graphmind entry
/// under the folder's path or one of its parents that no registered project uses. When more than
/// one could be, all are left alone and the outcome says so.
pub fn remove_claude_code_project(project: &Path, on_cli_fallback: &mut dyn FnMut(&str)) -> Outcome {
    let registered = claude_code_keys();
    let key = if project.exists() {
        claude_code_key(project)
    } else {
        match orphan_key(project, &registered) {
            Ok(key) => key,
            Err(outcome) => return outcome,
        }
    };
    let client = McpClient::ClaudeCodeProject(key.clone());
    if registered.contains(&key) {
        return match current_entry(&client) {
            Ok(Some(_)) => Outcome::LeftAlone(format!(
                "{}: left alone: another registered project uses this entry",
                describe(&client)
            )),
            Ok(None) | Err(_) => Outcome::Unchanged,
        };
    }
    remove(&client, on_cli_fallback)
}

/// The key of `project`, a folder that no longer exists: the one key, `project` or a parent of it,
/// holding a graphmind entry that no registered project uses; `project` itself when there is none
/// (or .claude.json can't be read, which [`remove`] then reports). `Err` when several could be.
fn orphan_key(project: &Path, registered: &[PathBuf]) -> Result<PathBuf, Outcome> {
    let path = McpClient::ClaudeCodeUser.config_path();
    let ConfigRead::Parsed { value: config, .. } = read_json_config(&path, false) else {
        return Ok(project.to_path_buf());
    };
    let candidates = orphan_candidates(&config, project, registered);
    match candidates.as_slice() {
        [] => Ok(project.to_path_buf()),
        [key] => Ok(key.clone()),
        _ => {
            let keys: Vec<String> = candidates.iter().map(|key| key.display().to_string()).collect();
            Err(Outcome::LeftAlone(format!(
                "{}: left alone: {} no longer exists and more than one graphmind entry could be its own ({})",
                path.display(),
                project.display(),
                keys.join(", ")
            )))
        }
    }
}

/// The keys of `config` that are `project` or a parent of it and hold a graphmind entry no
/// registered project uses.
fn orphan_candidates(config: &Value, project: &Path, registered: &[PathBuf]) -> Vec<PathBuf> {
    let Some(projects) = config.get("projects").and_then(Value::as_object) else {
        return Vec::new();
    };
    projects
        .iter()
        .filter(|(_, entry)| entry.get("mcpServers").and_then(|s| s.get(SERVER_NAME)).is_some_and(points_at_graphmind))
        .map(|(key, _)| PathBuf::from(key))
        .filter(|key| project.starts_with(key) && !registered.contains(key))
        .collect()
}

/// What [`uninstall_claude_code`] did.
#[derive(Debug)]
pub struct ClaudeCodeRemoval {
    /// How many per-project entries were removed, or why .claude.json was left alone.
    pub projects: Result<usize, String>,
    /// The user-scope entry: `Unchanged` when there is none that runs graphmind.
    pub user_scope: Outcome,
}

/// Remove graphmind from .claude.json: every per-project entry that runs a graphmind binary
/// (registered project or not) and a user-scope one. `graphmind` entries set up by hand, other
/// servers, projects and keys stay.
pub fn uninstall_claude_code(on_cli_fallback: &mut dyn FnMut(&str)) -> ClaudeCodeRemoval {
    let projects = remove_claude_project_entries();
    let user = McpClient::ClaudeCodeUser;
    let user_scope = if has_graphmind_entry(&user) { remove(&user, on_cli_fallback) } else { Outcome::Unchanged };
    ClaudeCodeRemoval { projects, user_scope }
}

fn remove_claude_project_entries() -> Result<usize, String> {
    let path = McpClient::ClaudeCodeUser.config_path();
    let mut config = match read_json_config(&path, false) {
        ConfigRead::Missing => return Ok(0),
        ConfigRead::Parsed { value, .. } => value,
        ConfigRead::Unparseable(reason) => return Err(reason),
    };
    let removed = remove_graphmind_project_entries(&mut config);
    if removed > 0 {
        write_config(&path, &config)?;
    }
    Ok(removed)
}

fn remove_graphmind_project_entries(config: &mut Value) -> usize {
    let mut removed = 0;
    if let Some(projects) = config.get_mut("projects").and_then(Value::as_object_mut) {
        for project in projects.values_mut() {
            let Some(servers) = project.get_mut("mcpServers").and_then(Value::as_object_mut) else {
                continue;
            };
            if servers.get(SERVER_NAME).is_some_and(points_at_graphmind) {
                servers.remove(SERVER_NAME);
                removed += 1;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=test", "-c", "user.email=test@test.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// A repo at `<dir>/main` with one commit and a `packages/api` subfolder, plus a linked
    /// worktree at `<dir>/wt`. Canonical paths of the main checkout and the worktree.
    fn repo_with_worktree(dir: &Path) -> (PathBuf, PathBuf) {
        let main = dir.join("main");
        fs::create_dir_all(main.join("packages/api")).unwrap();
        fs::write(main.join("packages/api/lib.rs"), "pub fn f() {}\n").unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["add", "."]);
        git(&main, &["commit", "-q", "-m", "init"]);
        git(&main, &["worktree", "add", "-q", "--detach", dir.join("wt").to_str().unwrap()]);
        (main.canonicalize().unwrap(), dir.join("wt").canonicalize().unwrap())
    }

    #[test]
    fn claude_code_key_is_the_main_checkout_of_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        let (main, worktree) = repo_with_worktree(dir.path());
        assert_eq!(claude_code_key(&main), main);
        assert_eq!(claude_code_key(&main.join("packages/api")), main, "a subfolder");
        assert_eq!(claude_code_key(&worktree), main, "a linked worktree");
        assert_eq!(claude_code_key(&worktree.join("packages/api")), main, "a subfolder of a worktree");
        // On macOS the temp dir is under /var, a symlink to /private/var: the key is the physical path.
        assert_eq!(claude_code_key(&dir.path().join("wt/packages/api")), main, "canonical");

        let plain = dir.path().join("plain");
        fs::create_dir_all(&plain).unwrap();
        assert_eq!(claude_code_key(&plain), plain.canonicalize().unwrap(), "outside git");
        #[cfg(unix)]
        {
            let link = dir.path().join("link");
            std::os::unix::fs::symlink(main.join("packages"), &link).unwrap();
            assert_eq!(claude_code_key(&link.join("api")), main, "symlinks resolved");
        }
        let gone = dir.path().join("gone");
        assert_eq!(claude_code_key(&gone), gone, "a folder that no longer exists keeps its path");

        // One key per repository, however many of its folders are registered.
        let projects = keyed([main.clone(), worktree.join("packages/api"), plain.clone(), gone]);
        let keys: Vec<&PathBuf> = projects.iter().map(|(_, key)| key).collect();
        assert_eq!(keys, vec![&main, &main, &plain.canonicalize().unwrap()], "a folder that's gone has no entry");
    }

    #[test]
    fn claude_code_key_follows_git_for_bare_repositories_submodules_and_separate_git_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let (main, _) = repo_with_worktree(dir.path());
        let canonical = |path: PathBuf| path.canonicalize().unwrap();

        // A bare repository's worktree, and a subfolder of it: the bare repository.
        let bare = dir.path().join("bare.git");
        git(dir.path(), &["clone", "-q", "--bare", main.to_str().unwrap(), bare.to_str().unwrap()]);
        git(&bare, &["worktree", "add", "-q", "--detach", dir.path().join("bwt").to_str().unwrap()]);
        assert_eq!(claude_code_key(&dir.path().join("bwt")), canonical(bare.clone()), "a bare repository's worktree");
        assert_eq!(claude_code_key(&dir.path().join("bwt/packages/api")), canonical(bare), "its subfolder");

        // A work tree whose git dir lives elsewhere (`--separate-git-dir`): the work tree.
        let sep = dir.path().join("sep");
        fs::create_dir_all(sep.join("sub")).unwrap();
        fs::write(sep.join("sub/lib.rs"), "pub fn f() {}\n").unwrap();
        git(dir.path(), &["init", "-q", "--separate-git-dir", dir.path().join("sep-git").to_str().unwrap(), sep.to_str().unwrap()]);
        git(&sep, &["add", "."]);
        git(&sep, &["commit", "-q", "-m", "init"]);
        assert_eq!(claude_code_key(&sep), canonical(sep.clone()));
        assert_eq!(claude_code_key(&sep.join("sub")), canonical(sep), "a subfolder of a --separate-git-dir work tree");

        // A submodule: its own root, from any subfolder; a worktree of it: the submodule's git dir.
        let sup = dir.path().join("super");
        fs::create_dir_all(&sup).unwrap();
        fs::write(sup.join("README"), "super\n").unwrap();
        git(&sup, &["init", "-q"]);
        git(&sup, &["add", "."]);
        git(&sup, &["commit", "-q", "-m", "init"]);
        git(&sup, &["-c", "protocol.file.allow=always", "submodule", "--quiet", "add", main.to_str().unwrap(), "libs/sub"]);
        let sub = canonical(sup.join("libs/sub"));
        assert_eq!(claude_code_key(&sup.join("libs/sub")), sub, "a submodule");
        assert_eq!(claude_code_key(&sup.join("libs/sub/packages/api")), sub, "a subfolder of a submodule");
        assert_eq!(claude_code_key(&sup.join("libs")), canonical(sup.clone()), "the superproject");
        git(&sub, &["worktree", "add", "-q", "--detach", dir.path().join("subwt").to_str().unwrap()]);
        let modules = canonical(sup.join(".git/modules/libs/sub"));
        assert_eq!(claude_code_key(&dir.path().join("subwt")), modules, "a submodule's worktree");
        assert_eq!(claude_code_key(&dir.path().join("subwt/packages")), modules, "its subfolder");
    }

    #[test]
    fn a_deleted_folder_is_keyed_to_the_one_unclaimed_entry_above_it() {
        let gm = json!({ "command": "/bin/graphmind", "args": ["mcp"] });
        let config = json!({ "projects": {
            "/repo": { "mcpServers": { "graphmind": gm } },
            "/rep": { "mcpServers": { "graphmind": gm } },
            "/repo/hand": { "mcpServers": { "graphmind": { "command": "/opt/wrapper" } } },
            "/repo/pkg": { "mcpServers": { "graphmind": gm } },
            "/elsewhere": { "mcpServers": { "graphmind": gm } }
        } });
        let candidates = |project: &str, registered: &[&str]| {
            let registered: Vec<PathBuf> = registered.iter().map(PathBuf::from).collect();
            orphan_candidates(&config, Path::new(project), &registered)
        };
        assert_eq!(candidates("/repo/src/gone", &[]), vec![PathBuf::from("/repo")], "a parent, by path components");
        assert_eq!(candidates("/repo/hand/gone", &[]), vec![PathBuf::from("/repo")], "a hand-made entry is not graphmind's");
        assert!(candidates("/repo/src/gone", &["/repo"]).is_empty(), "a registered project uses it");
        let mut both = candidates("/repo/pkg/gone", &[]);
        both.sort();
        assert_eq!(both, vec![PathBuf::from("/repo"), PathBuf::from("/repo/pkg")], "more than one: ambiguous");
        assert_eq!(candidates("/repo/pkg", &["/repo"]), vec![PathBuf::from("/repo/pkg")], "its own path");
        assert!(candidates("/gone", &[]).is_empty());
    }

    #[test]
    fn keys_are_configured_when_each_holds_a_current_or_hand_made_entry() {
        let entry = json!({ "type": "stdio", "command": "/bin/graphmind", "args": ["mcp"], "env": {} });
        let keys = [PathBuf::from("/repo/a"), PathBuf::from("/repo/b")];
        let config = |b: Value| json!({ "projects": { "/repo/a": { "mcpServers": { "graphmind": entry } }, "/repo/b": { "mcpServers": { "graphmind": b } } } });
        assert!(keys_configured(&config(entry.clone()), &keys, "/bin/graphmind"));
        assert!(keys_configured(&config(json!({ "command": "/opt/my-wrapper" })), &keys, "/bin/graphmind"), "hand-made");
        assert!(!keys_configured(&config(json!({ "command": "/old/graphmind" })), &keys, "/bin/graphmind"), "stale");
        assert!(!keys_configured(&json!({ "projects": { "/repo/a": { "mcpServers": { "graphmind": entry } } } }), &keys, "/bin/graphmind"));
        assert!(!keys_configured(&json!({}), &[], "/bin/graphmind"), "no registered project");
    }

    #[test]
    fn project_sweep_removes_only_graphmind_entries() {
        let mut config = json!({
            "mcpServers": { "graphmind": { "command": "/bin/graphmind" } },
            "projects": {
                "/repo/a": { "mcpServers": { "graphmind": { "command": "/old/graphmind" }, "other": { "command": "other" } } },
                "/repo/b": { "mcpServers": { "graphmind": { "command": "/opt/my-wrapper" } } },
                "/repo/c": { "allowedTools": [] }
            }
        });
        assert_eq!(remove_graphmind_project_entries(&mut config), 1);
        assert_eq!(config["projects"]["/repo/a"]["mcpServers"], json!({ "other": { "command": "other" } }));
        assert_eq!(config["projects"]["/repo/b"]["mcpServers"]["graphmind"]["command"], "/opt/my-wrapper");
        assert_eq!(config["projects"]["/repo/c"], json!({ "allowedTools": [] }));
        assert!(config["mcpServers"]["graphmind"].is_object(), "user scope is not a project entry");
    }
}
