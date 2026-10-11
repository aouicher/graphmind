use crate::state::AppState;
use crate::types::{GraphStats, ProjectInfo};
use super::integrations::log_cli_fallback;
use graphmind_config::claude_code;
use graphmind_config::mcp_clients::{self, McpClient, Outcome};
use graphmind_config::{paths, Registry};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn list_projects(state: State<Mutex<AppState>>) -> Vec<ProjectInfo> {
    Registry::forget_gone_worktrees();
    let app_state = state.lock().unwrap();
    let projects = Registry::list();

    projects
        .into_iter()
        .map(|p| {
            let stats = get_stats(&p.slug);
            let is_watching = app_state.watchers.contains_key(&p.slug);
            ProjectInfo {
                slug: p.slug,
                path: p.path,
                last_build: p.last_build,
                languages: p.languages,
                stats,
                is_watching,
            }
        })
        .collect()
}

#[tauri::command]
pub async fn add_project(path: String, app: AppHandle) -> Result<ProjectInfo, String> {
    let project = Registry::register(&path, None, &[]);
    let slug = project.slug.clone();
    // The entry may go through the `claude` CLI: written in the background, outcome logged.
    let project_path = PathBuf::from(&project.path);
    tokio::task::spawn_blocking(move || add_claude_code_entry(&project_path));
    let info = ProjectInfo {
        slug: project.slug,
        path: project.path,
        last_build: project.last_build,
        languages: project.languages,
        stats: None,
        is_watching: false,
    };

    let app_clone = app.clone();
    tokio::spawn(async move {
        super::indexing::build_project_uncancelled(slug, true, app_clone).await.ok();
    });

    Ok(info)
}

#[tauri::command]
pub async fn remove_project(slug: String) -> Result<bool, String> {
    let graph_dir = paths::graph_dir(&slug);
    if graph_dir.exists() {
        std::fs::remove_dir_all(&graph_dir).ok();
    }
    let project_path = Registry::get(&slug).map(|p| PathBuf::from(p.path));
    let removed = Registry::unregister(&slug);
    if let (true, Some(project_path)) = (removed, project_path) {
        tokio::task::spawn_blocking(move || remove_claude_code_entry(&project_path)).await.ok();
    }
    Ok(removed)
}

/// The Claude Code entry the project uses (`projects.<key>.mcpServers` in .claude.json, one key
/// per repository), as `graphmind init` and `graphmind register` write it. The registration
/// stands without it: why it wasn't written is logged.
fn add_claude_code_entry(project: &Path) -> Outcome {
    let client = McpClient::ClaudeCodeProject(claude_code::claude_code_key(project));
    let outcome = mcp_clients::upsert(&client, &mcp_clients::graphmind_binary(), &mut log_cli_fallback);
    if let Outcome::LeftAlone(reason) | Outcome::Kept(reason) = &outcome {
        eprintln!("[graphmind] Claude Code MCP entry not written: {reason}");
    }
    outcome
}

/// The Claude Code entry of a removed project (see `claude_code::remove_claude_code_project`);
/// why it stays, when it does, is logged.
fn remove_claude_code_entry(project: &Path) -> Outcome {
    let outcome = claude_code::remove_claude_code_project(project, &mut log_cli_fallback);
    if let Outcome::LeftAlone(reason) | Outcome::Kept(reason) = &outcome {
        eprintln!("[graphmind] Claude Code MCP entry not removed: {reason}");
    }
    outcome
}

#[tauri::command]
pub fn get_project_status(slug: String) -> Result<ProjectInfo, String> {
    let project = Registry::get(&slug).ok_or_else(|| format!("Project {slug} not found"))?;
    let stats = get_stats(&slug);
    Ok(ProjectInfo {
        slug: project.slug,
        path: project.path,
        last_build: project.last_build,
        languages: project.languages,
        stats,
        is_watching: false,
    })
}

fn get_stats(slug: &str) -> Option<GraphStats> {
    let db_path = paths::graph_db_path(slug);
    if !db_path.exists() {
        return None;
    }
    let db_path_str = db_path.to_string_lossy().to_string();
    let db = graphmind_db::schema::init_database(&db_path_str).ok()?;
    let queries = graphmind_db::queries::GraphQueries::new(&db);
    let stats = queries.stats();
    Some(GraphStats {
        symbols: stats.symbols,
        edges: stats.edges,
        files: stats.files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::ffi::OsString;
    use std::fs;
    use std::process::Command;
    use std::sync::MutexGuard;

    /// A temp HOME and CLAUDE_CONFIG_DIR, and a PATH without the machine's `claude`, under the env
    /// lock; the previous values come back on drop, panics included.
    struct TestEnv {
        home: tempfile::TempDir,
        saved: Vec<(&'static str, Option<OsString>)>,
        _lock: MutexGuard<'static, ()>,
    }

    impl TestEnv {
        fn new() -> Self {
            let lock = crate::commands::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let home = tempfile::tempdir().unwrap();
            let config_dir = home.path().join("claude-config");
            fs::create_dir_all(&config_dir).unwrap();
            let vars: [(&'static str, OsString); 3] = [
                ("HOME", home.path().into()),
                ("CLAUDE_CONFIG_DIR", config_dir.into()),
                ("PATH", "/usr/bin:/bin".into()),
            ];
            let saved = vars.iter().map(|(name, _)| (*name, std::env::var_os(name))).collect();
            for (name, value) in vars {
                unsafe { std::env::set_var(name, value) };
            }
            TestEnv { home, saved, _lock: lock }
        }

        fn claude_json(&self) -> PathBuf {
            self.home.path().join("claude-config/.claude.json")
        }

        fn read(&self) -> Value {
            serde_json::from_str(&fs::read_to_string(self.claude_json()).unwrap()).unwrap()
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                match value {
                    Some(value) => unsafe { std::env::set_var(name, value) },
                    None => unsafe { std::env::remove_var(name) },
                }
            }
        }
    }

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

    #[test]
    fn added_projects_get_their_claude_code_entry_and_a_broken_file_is_left_alone() {
        let env = TestEnv::new();
        let project = env.home.path().join("app");
        fs::create_dir_all(&project).unwrap();
        let key = project.canonicalize().unwrap().to_string_lossy().into_owned();

        assert_eq!(add_claude_code_entry(&project), Outcome::Written);
        let entry = env.read()["projects"][&key]["mcpServers"]["graphmind"].clone();
        assert_eq!(entry["args"], json!(["mcp"]), "{entry}");
        assert_eq!(add_claude_code_entry(&project), Outcome::Unchanged);

        assert_eq!(remove_claude_code_entry(&project), Outcome::Written, "not registered: removed");
        assert!(env.read()["projects"][&key]["mcpServers"]["graphmind"].is_null());

        let broken = r#"{"projects": {"#;
        fs::write(env.claude_json(), broken).unwrap();
        assert!(matches!(add_claude_code_entry(&project), Outcome::LeftAlone(_)));
        match remove_claude_code_entry(&project) {
            Outcome::LeftAlone(reason) => assert!(reason.contains("is not valid JSON"), "{reason}"),
            other => panic!("the reason is reported: {other:?}"),
        }
        assert_eq!(fs::read_to_string(env.claude_json()).unwrap(), broken);
    }

    #[test]
    fn adding_a_project_keeps_a_hand_made_entry() {
        let env = TestEnv::new();
        let project = env.home.path().join("app");
        fs::create_dir_all(&project).unwrap();
        let key = project.canonicalize().unwrap().to_string_lossy().into_owned();
        let mine = json!({ "projects": { key: { "mcpServers": { "graphmind": { "command": "/opt/my-wrapper.sh" } } } } }).to_string();
        fs::write(env.claude_json(), &mine).unwrap();

        match add_claude_code_entry(&project) {
            Outcome::Kept(reason) => assert!(reason.contains("hand-made graphmind entry"), "{reason}"),
            other => panic!("kept: {other:?}"),
        }
        assert!(matches!(remove_claude_code_entry(&project), Outcome::Kept(_)));
        assert_eq!(fs::read_to_string(env.claude_json()).unwrap(), mine);
    }

    #[test]
    fn install_writes_one_entry_per_repository() {
        let env = TestEnv::new();
        let main = env.home.path().join("app");
        fs::create_dir_all(main.join("packages/api")).unwrap();
        fs::write(main.join("packages/api/lib.rs"), "pub fn f() {}\n").unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["add", "."]);
        git(&main, &["commit", "-q", "-m", "init"]);
        let worktree = env.home.path().join("app-wt");
        git(&main, &["worktree", "add", "-q", "--detach", worktree.to_str().unwrap()]);
        Registry::register(main.join("packages/api").to_str().unwrap(), Some("api"), &[]);
        Registry::register(worktree.to_str().unwrap(), Some("app-wt"), &[]);
        assert!(!claude_code::claude_code_configured(&mcp_clients::graphmind_binary()));

        super::super::integrations::install_claude_code().unwrap();

        let projects = env.read()["projects"].clone();
        let keys: Vec<&String> = projects.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec![&main.canonicalize().unwrap().to_string_lossy().into_owned()], "{projects}");
        assert!(claude_code::claude_code_configured(&mcp_clients::graphmind_binary()), "the card reads configured");

        // The main checkout's entry stays while the worktree is registered.
        Registry::unregister("api");
        assert!(matches!(remove_claude_code_entry(&main.join("packages/api")), Outcome::LeftAlone(_)));
        assert_eq!(env.read()["projects"], projects);
    }
}
