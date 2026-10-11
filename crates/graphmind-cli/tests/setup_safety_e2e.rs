//! `graphmind setup`, `install hook-claude` and `uninstall` never rewrite a settings.json, a shell
//! profile or a git hook they can't read: the file stays byte-identical and the output names it.
//! An existing profile or hook is backed up before it changes, and isn't changed when that backup
//! fails; backups never replace each other. The setup summary says what each step did, and only
//! what setup failed to do (not what the person set up themselves) makes it "Setup finished".
//! Every command runs the graphmind binary with a temp HOME and a PATH holding only the system
//! directories.
//!
//! Run with:
//!   cargo test -p graphmind-cli --test setup_safety_e2e -- --test-threads=1

mod common;
use common::{backups, with_home, CliTestEnv};
use graphmind_cli::commands::claude_hook::{install_hook, installed_hooks_outdated};
use graphmind_cli::commands::setup::{install_shell_path_for, uninstall_shell_path, ShellPath};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Text that isn't UTF-8: a Latin-1 "é" in a comment.
const LATIN1_COMMENT: &[u8] = b"# caf\xe9\n";

/// `graphmind <args>` with zsh as the shell and `path_env` as PATH.
fn graphmind(env: &CliTestEnv, args: &[&str], path_env: &str) -> Output {
    env.command(args)
        .current_dir(env.home.path())
        .env("PATH", path_env)
        .env("SHELL", "/bin/zsh")
        .output()
        .expect("failed to run graphmind")
}

/// stderr, then stdout: the summary stays at the end.
fn output_text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout))
}

/// `graphmind setup` with graphmind not on PATH; its output. Setup succeeds even when it leaves
/// files unchanged.
fn setup(env: &CliTestEnv) -> String {
    setup_with_path(env, "/usr/bin:/bin")
}

fn setup_with_path(env: &CliTestEnv, path_env: &str) -> String {
    let out = graphmind(env, &["setup"], path_env);
    let text = output_text(&out);
    assert!(out.status.success(), "setup failed:\n{text}");
    text
}

/// The summary: everything after setup's separator line.
fn summary(output: &str) -> &str {
    output.rsplit_once('─').map_or(output, |(_, after)| after)
}

fn claude_dir(env: &CliTestEnv) -> PathBuf {
    env.home.path().join(".claude")
}

/// A settings.json that isn't UTF-8 (a Latin-1 byte) and still registers the retired Stop hook,
/// with that script in place. Returns its bytes.
fn non_utf8_settings_with_retired_hook(env: &CliTestEnv) -> Vec<u8> {
    let hooks = claude_dir(env).join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    let stop = hooks.join("graphmind-stop.sh");
    fs::write(&stop, "#!/bin/sh\n").unwrap();
    let entry = json!({ "hooks": { "Stop": [{ "matcher": "*", "hooks": [{ "type": "command", "command": stop }] }] } });
    let mut bytes = entry.to_string().into_bytes();
    bytes.splice(1..1, b"\"note\": \"caf\xe9\", ".iter().copied());
    fs::write(claude_dir(env).join("settings.json"), &bytes).unwrap();
    bytes
}

/// `graphmind register <dir> --slug <slug>` with graphmind not on PATH.
fn register(env: &CliTestEnv, dir: &Path, slug: &str) {
    let out = graphmind(env, &["register", dir.to_str().unwrap(), "--slug", slug], "/usr/bin:/bin");
    assert!(out.status.success(), "register failed:\n{}", output_text(&out));
}

/// A registered git repo whose hooks folder holds graphmind's post-commit hook (an install from
/// before the other hooks existed). Returns the hooks folder.
fn repo_with_graphmind_post_commit(env: &CliTestEnv) -> PathBuf {
    let repo = env.home.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    let out = Command::new("git").arg("-C").arg(&repo).args(["init", "-q"]).env("HOME", env.home.path()).output().unwrap();
    assert!(out.status.success(), "git init: {}", String::from_utf8_lossy(&out.stderr));
    let hooks = repo.join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(hooks.join("post-commit"), "#!/bin/sh\n# graphmind: auto-rebuild\ngraphmind build 2>/dev/null &\n").unwrap();
    register(env, &repo, "repo");
    // As git reports it, symlinks resolved (/var is /private/var on macOS).
    hooks.canonicalize().unwrap()
}

/// How many times `needle` appears in `haystack`.
fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

// ---------------------------------------------------------------------------
// settings.json
// ---------------------------------------------------------------------------

#[test]
fn setup_leaves_a_non_utf8_settings_json_unchanged() {
    let env = CliTestEnv::new();
    let before = non_utf8_settings_with_retired_hook(&env);
    let settings = claude_dir(&env).join("settings.json");

    let out = setup(&env);

    assert_eq!(fs::read(&settings).unwrap(), before, "settings.json rewritten:\n{out}");
    let reason = format!("can't read {}", settings.display());
    assert!(out.contains(&reason) && out.contains("left unchanged"), "the reason names settings.json:\n{out}");
    assert_eq!(count(&out, &reason), 1, "the reason is printed once:\n{out}");
    assert!(claude_dir(&env).join("hooks/graphmind-stop.sh").exists(), "settings.json still runs the retired script");
    let summary = summary(&out);
    assert!(summary.contains("Claude Code hooks not registered"), "{summary}");
    assert!(!summary.contains("hooks registered ("), "{summary}");
    assert!(out.contains("Setup finished") && !out.contains("Setup complete"), "{out}");
}

#[test]
fn install_hook_claude_leaves_a_non_utf8_settings_json_unchanged() {
    let env = CliTestEnv::new();
    let before = non_utf8_settings_with_retired_hook(&env);

    let out = graphmind(&env, &["install", "hook-claude"], "/usr/bin:/bin");

    assert!(!out.status.success(), "hooks not registered is a failure:\n{}", output_text(&out));
    assert_eq!(fs::read(claude_dir(&env).join("settings.json")).unwrap(), before);
    assert!(claude_dir(&env).join("hooks/graphmind-stop.sh").exists(), "retired script deleted");
    assert!(output_text(&out).contains("left unchanged"), "{}", output_text(&out));
}

#[test]
fn uninstall_unregisters_before_deleting_the_hook_scripts() {
    let env = CliTestEnv::new();
    env.run_ok(&["install", "hook-claude"]);
    let settings = claude_dir(&env).join("settings.json");
    let mut unreadable = fs::read(&settings).unwrap();
    unreadable.extend_from_slice(b"\n\xe9");
    fs::write(&settings, &unreadable).unwrap();
    let script = claude_dir(&env).join("hooks/graphmind-session.sh");
    assert!(script.exists());

    let out = graphmind(&env, &["uninstall", "hook-claude"], "/usr/bin:/bin");
    assert!(!out.status.success(), "{}", output_text(&out));
    assert!(!output_text(&out).contains("Hooks uninstalled"), "{}", output_text(&out));
    assert_eq!(fs::read(&settings).unwrap(), unreadable);
    assert!(script.exists(), "settings.json still runs the script it points at");

    let out = graphmind(&env, &["uninstall", "all"], "/usr/bin:/bin");
    let text = output_text(&out);
    assert!(out.status.success(), "{text}");
    assert_eq!(fs::read(&settings).unwrap(), unreadable);
    assert!(script.exists(), "uninstall all deleted a script settings.json still runs");
    assert_eq!(count(&text, &format!("can't read {}", settings.display())), 1, "the reason is printed once:\n{text}");
    assert!(text.contains("Integrations removed, except the Claude Code hooks"), "{text}");
    assert!(!text.contains("Integrations removed.\n"), "{text}");

    // Once settings.json reads again, uninstall removes the entries, then the scripts.
    fs::write(&settings, &unreadable[..unreadable.len() - 2]).unwrap();
    env.run_ok(&["uninstall", "hook-claude"]);
    assert!(!script.exists());
    assert!(!fs::read_to_string(&settings).unwrap().contains("graphmind-session.sh"));
}

// ---------------------------------------------------------------------------
// Shell profile
// ---------------------------------------------------------------------------

#[test]
fn setup_leaves_a_non_utf8_profile_unchanged() {
    let env = CliTestEnv::new();
    let zshenv = env.home.path().join(".zshenv");
    let before = [LATIN1_COMMENT, b"export EDITOR=vim\n"].concat();
    fs::write(&zshenv, &before).unwrap();

    let out = setup(&env);

    assert_eq!(fs::read(&zshenv).unwrap(), before, "profile rewritten:\n{out}");
    assert!(out.contains(&format!("can't read {}", zshenv.display())) && out.contains("left unchanged"), "{out}");
    let bin_dir = env.home.path().join(".graphmind/bin");
    let add_yourself = format!("add {} to PATH yourself", bin_dir.display());
    let summary = summary(&out);
    assert!(summary.contains("PATH left unchanged") && summary.contains(&add_yourself), "{summary}");
    assert!(!summary.contains("PATH configured"), "{summary}");
    assert!(out.contains("Setup finished") && !out.contains("Setup complete"), "{out}");
    assert!(backups(&zshenv).is_empty(), "nothing to back up when nothing is written");
}

#[test]
fn setup_backs_up_an_existing_profile_before_appending() {
    let env = CliTestEnv::new();
    let zshenv = env.home.path().join(".zshenv");
    fs::write(&zshenv, "export EDITOR=vim\n").unwrap();

    let out = setup(&env);

    let content = fs::read_to_string(&zshenv).unwrap();
    assert!(content.starts_with("export EDITOR=vim\nexport PATH=") && content.contains("# added by graphmind setup"), "{content}");
    assert_eq!(backups(&zshenv), vec![b"export EDITOR=vim\n".to_vec()], "{out}");
    assert!(summary(&out).contains("PATH configured in ~/.zshenv"), "{out}");
}

#[test]
fn setup_keeps_the_profile_from_before_setup_when_it_backs_up_twice() {
    let env = CliTestEnv::new();
    // No binary in ~/.graphmind/bin: its old PATH line is dead and goes, then the line for the
    // binary's own folder is appended. Both steps back the profile up within the same second.
    fs::remove_file(env.home.path().join(".graphmind/bin/graphmind")).unwrap();
    let zshenv = env.home.path().join(".zshenv");
    let legacy = format!("export PATH=\"{}:$PATH\"", env.home.path().join(".graphmind/bin").display());
    let original = format!("export EDITOR=vim\n{legacy}\n");
    fs::write(&zshenv, &original).unwrap();

    let out = setup(&env);

    let backups = backups(&zshenv);
    assert_eq!(backups.len(), 2, "{out}");
    assert!(backups.contains(&original.into_bytes()), "no backup holds the profile from before setup");
    assert!(backups.contains(&b"export EDITOR=vim\n".to_vec()));
}

#[test]
fn setup_creates_a_missing_profile() {
    let env = CliTestEnv::new();
    let out = setup(&env);

    let content = fs::read_to_string(env.home.path().join(".zshenv")).unwrap();
    let bin_dir = env.home.path().join(".graphmind/bin");
    assert_eq!(content, format!("export PATH=\"{}:$PATH\"  # added by graphmind setup\n", bin_dir.display()));
    let summary = summary(&out);
    assert!(summary.contains("PATH configured in ~/.zshenv"), "{summary}");
    assert!(summary.contains("3 hooks registered (PreToolUse, SessionStart, UserPromptSubmit)"), "{summary}");
    assert!(summary.contains("/gm skill + 19 sub-skills installed"), "{summary}");
    assert!(out.contains("Setup complete"), "{out}");
}

#[test]
fn setup_summary_says_when_graphmind_is_already_on_path() {
    let env = CliTestEnv::new();
    let bin_dir = env.home.path().join(".graphmind/bin");
    let out = setup_with_path(&env, &format!("{}:/usr/bin:/bin", bin_dir.display()));

    assert!(!env.home.path().join(".zshenv").exists());
    let summary = summary(&out);
    assert!(summary.contains("graphmind already on PATH"), "{summary}");
    assert!(!summary.contains("PATH configured"), "{summary}");
    assert!(out.contains("Setup complete"), "{out}");
}

/// A second setup before the shell is restarted: the profile has the line, this PATH doesn't.
#[test]
fn setup_summary_says_when_only_the_profile_has_the_path_line() {
    let env = CliTestEnv::new();
    setup(&env);
    let out = setup(&env);

    let summary = summary(&out);
    assert!(summary.contains("PATH configured in ~/.zshenv (open a new shell)"), "{summary}");
    assert!(!summary.contains("already on PATH"), "{summary}");
    assert!(out.contains("Setup complete"), "{out}");
}

#[test]
fn setup_summary_says_when_skills_are_not_installed() {
    let env = CliTestEnv::new();
    fs::create_dir_all(claude_dir(&env)).unwrap();
    // A file where the skills folder goes.
    fs::write(claude_dir(&env).join("skills"), "").unwrap();

    let out = setup(&env);

    let summary = summary(&out);
    assert!(summary.contains("skills not fully installed"), "{summary}");
    assert!(!summary.contains("sub-skills installed"), "{summary}");
    assert!(summary.contains("3 hooks registered"), "the steps after it still ran: {summary}");
    assert!(out.contains("Setup finished") && !out.contains("Setup complete"), "{out}");
}

#[cfg(unix)]
#[test]
fn setup_leaves_a_profile_alone_when_it_cannot_be_backed_up() {
    use std::os::unix::fs::PermissionsExt;
    with_home(|home| {
        let zshenv = home.join(".zshenv");
        fs::write(&zshenv, "export EDITOR=vim\n").unwrap();
        fs::set_permissions(home, fs::Permissions::from_mode(0o555)).unwrap();
        if fs::write(home.join("probe"), "").is_ok() {
            fs::set_permissions(home, fs::Permissions::from_mode(0o755)).unwrap();
            println!("skipped: running as root, a read-only folder is still writable");
            return;
        }

        let bin_dir = home.join("opt").join("bin");
        let outcome = install_shell_path_for(&bin_dir.join("graphmind"), "/usr/bin:/bin", "/bin/zsh");
        fs::set_permissions(home, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(outcome, ShellPath::LeftUnchanged(bin_dir));
        assert_eq!(fs::read_to_string(&zshenv).unwrap(), "export EDITOR=vim\n");
    });
}

#[test]
fn uninstall_leaves_a_non_utf8_profile_unchanged() {
    with_home(|home| {
        let zshrc = home.join(".zshrc");
        let marked = format!("export PATH=\"{}:$PATH\"  # added by graphmind setup\n", home.join(".graphmind/bin").display());
        let before = [LATIN1_COMMENT, marked.as_bytes()].concat();
        fs::write(&zshrc, &before).unwrap();

        uninstall_shell_path();

        assert_eq!(fs::read(&zshrc).unwrap(), before);
        assert!(backups(&zshrc).is_empty());
    });
}

/// The hooks are installed and settings.json can't be read: auto-setup has nothing to fix there,
/// so it doesn't count as outdated (setup would run every day and leave it unchanged). A missing
/// settings.json is outdated: setup creates it.
#[test]
fn hooks_are_not_outdated_while_settings_json_cannot_be_read() {
    with_home(|home| {
        install_hook().unwrap();
        let settings = home.join(".claude/settings.json");
        assert!(!installed_hooks_outdated());

        fs::write(&settings, b"{\"note\": \"caf\xe9\"}").unwrap();
        assert!(!installed_hooks_outdated(), "not UTF-8");
        fs::write(&settings, "{ not json").unwrap();
        assert!(!installed_hooks_outdated(), "not JSON");

        fs::remove_file(&settings).unwrap();
        assert!(installed_hooks_outdated(), "missing");
    });
}

// ---------------------------------------------------------------------------
// Git hooks
// ---------------------------------------------------------------------------

/// Setup adds the hooks newer versions install next to graphmind's post-commit hook; a hook there
/// that isn't UTF-8 is left byte-identical, with the reason.
#[test]
fn setup_leaves_a_non_utf8_git_hook_unchanged() {
    let env = CliTestEnv::new();
    let hooks = repo_with_graphmind_post_commit(&env);
    let post_merge = hooks.join("post-merge");
    let before = [b"#!/bin/sh\n".as_slice(), LATIN1_COMMENT, b"make deps\n"].concat();
    fs::write(&post_merge, &before).unwrap();

    let out = setup(&env);

    assert_eq!(fs::read(&post_merge).unwrap(), before, "post-merge rewritten:\n{out}");
    assert!(backups(&post_merge).is_empty(), "nothing to back up when nothing is written");
    assert!(out.contains(&format!("can't read {}", post_merge.display())), "{out}");
    assert!(fs::read_to_string(hooks.join("post-checkout")).unwrap().contains("graphmind"), "the other hooks are added");
    assert!(summary(&out).contains("1 git hook(s) left unchanged"), "{out}");
    assert!(out.contains("Setup finished") && !out.contains("Setup complete"), "{out}");
}

/// A git hook that can't be backed up isn't written.
#[cfg(unix)]
#[test]
fn setup_leaves_a_git_hook_alone_when_it_cannot_be_backed_up() {
    use std::os::unix::fs::PermissionsExt;
    let env = CliTestEnv::new();
    let hooks = repo_with_graphmind_post_commit(&env);
    let post_merge = hooks.join("post-merge");
    fs::write(&post_merge, "#!/bin/sh\nmake deps\n").unwrap();
    // The hook stays writable; its folder doesn't take a backup.
    fs::set_permissions(&hooks, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(hooks.join("probe"), "").is_ok() {
        fs::set_permissions(&hooks, fs::Permissions::from_mode(0o755)).unwrap();
        println!("skipped: running as root, a read-only folder is still writable");
        return;
    }

    let out = setup(&env);
    fs::set_permissions(&hooks, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(fs::read_to_string(&post_merge).unwrap(), "#!/bin/sh\nmake deps\n", "{out}");
    assert!(backups(&post_merge).is_empty());
    assert!(out.contains(&format!("can't back up {}", post_merge.display())), "{out}");
    assert!(out.contains("Setup finished"), "{out}");
}

// ---------------------------------------------------------------------------
// What the person set up themselves
// ---------------------------------------------------------------------------

/// A CLAUDE.md block the person edited is theirs: setup keeps it, says so on an informational
/// line, and still reads "Setup complete".
#[test]
fn an_edited_claude_md_block_does_not_fail_setup() {
    let env = CliTestEnv::new();
    let claude_md = claude_dir(&env).join("CLAUDE.md");
    fs::create_dir_all(claude_dir(&env)).unwrap();
    let mine = "<!-- GM:START -->\n<!-- GM:VERSION:0.2.82 -->\n\n## Mandatory: Code exploration via graphmind\n\nOnly for callers and blast radius.\n\n<!-- GM:END -->\n";
    fs::write(&claude_md, mine).unwrap();

    let out = setup(&env);

    assert_eq!(fs::read_to_string(&claude_md).unwrap(), mine);
    assert!(summary(&out).contains("CLAUDE.md instruction block left as you set it"), "{out}");
    assert!(out.contains("Setup complete") && !out.contains("Setup finished"), "{out}");
}

/// A hand-made graphmind entry in .claude.json (its command isn't a graphmind binary) is the
/// person's: setup keeps it, says so, and still reads "Setup complete".
#[test]
fn a_hand_made_claude_code_entry_does_not_fail_setup() {
    let env = CliTestEnv::new();
    let project = env.home.path().join("app");
    fs::create_dir_all(&project).unwrap();
    let key = project.canonicalize().unwrap().to_string_lossy().into_owned();
    let claude_json = env.home.path().join(".claude.json");
    let mine = json!({ "projects": { &key: { "mcpServers": { "graphmind": { "command": "/opt/my-wrapper.sh" } } } } }).to_string();
    fs::write(&claude_json, &mine).unwrap();
    register(&env, &project, "app");

    let out = setup(&env);

    let config: serde_json::Value = serde_json::from_str(&fs::read_to_string(&claude_json).unwrap()).unwrap();
    assert_eq!(config["projects"][&key]["mcpServers"]["graphmind"]["command"], "/opt/my-wrapper.sh", "{out}");
    let summary = summary(&out);
    assert!(summary.contains(&format!("graphmind entry left as you set it in {key}")), "{summary}");
    assert!(!summary.contains("Claude Code (per project)"), "{summary}");
    assert!(out.contains("Setup complete") && !out.contains("Setup finished"), "{out}");
}
