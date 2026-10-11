mod common;

use common::CliTestEnv;
use std::fs;

/// Commands the skills name that don't take `--slug`: `list` and `cross query` span all
/// projects, `which` names the current directory's project and `init` registers a folder.
const WITHOUT_PROJECT: &[&str] = &["list", "cross query", "which", "init"];

/// Every `graphmind ...` command written in backticks in the installed skills.
fn skill_commands(env: &CliTestEnv) -> Vec<(String, String)> {
    env.run_ok(&["install", "skill"]);
    let skills = env.home.path().join(".claude").join("skills");
    let mut commands = Vec::new();
    for entry in fs::read_dir(&skills).unwrap().flatten() {
        let text = fs::read_to_string(entry.path().join("SKILL.md")).unwrap();
        for (i, span) in text.split('`').enumerate() {
            if i % 2 == 1 && span.starts_with("graphmind ") && !span.contains('\n') {
                commands.push((entry.file_name().to_string_lossy().to_string(), span.to_string()));
            }
        }
    }
    assert!(commands.len() > 20, "skills not installed or not parsed: {commands:?}");
    commands
}

/// The subcommand path of `graphmind <words...>`: one word, or two when the first has subcommands.
fn subcommand(env: &CliTestEnv, command: &str) -> Vec<String> {
    let words: Vec<&str> = command.split_whitespace().skip(1).collect();
    let help = env.run_ok(&[words[0], "--help"]);
    let depth = if help.contains("\nCommands:") { 2 } else { 1 };
    words[..depth].iter().map(|w| w.to_string()).collect()
}

/// The long flags a command's `--help` lists under "Options:", e.g. `--limit` from
/// `  -l, --limit <LIMIT>  Max results`.
fn listed_flags(help: &str) -> Vec<String> {
    let options = &help[help.find("\nOptions:\n").expect("Options section")..];
    options
        .lines()
        .skip(2)
        .map(str::trim_start)
        .filter(|line| line.starts_with('-'))
        .flat_map(|line| {
            line.split_whitespace()
                .take_while(|w| w.starts_with('-') || w.starts_with('<'))
                .filter(|w| w.starts_with("--"))
                .map(|w| w.trim_end_matches(',').to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The skills tell the agent to pass `--slug` to every command outside an indexed project, and
/// use other flags in their examples: each command must accept every flag it is shown with, and
/// `--slug` unless it spans all projects. Keeps the skill text in step with the CLI.
#[test]
fn every_command_in_the_skills_accepts_its_flags_and_slug() {
    let env = CliTestEnv::new();
    let mut checked = std::collections::HashSet::new();
    for (skill, command) in skill_commands(&env) {
        let path = subcommand(&env, &command);
        let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
        args.push("--help");
        let help = env.run_ok(&args);
        let name = path.join(" ");

        let listed = listed_flags(&help);
        let mut flags: Vec<&str> = command.split_whitespace().filter(|w| w.starts_with("--")).collect();
        if !WITHOUT_PROJECT.contains(&name.as_str()) {
            flags.push("--slug");
        }
        for flag in flags {
            assert!(
                listed.iter().any(|l| l == flag),
                "{skill}: `{command}` — `graphmind {name}` doesn't accept {flag} (options: {listed:?})"
            );
        }
        checked.insert(name);
    }
    for expected in ["search", "fn", "map", "status", "memory search", "cross query", "list", "which"] {
        assert!(checked.contains(expected), "skills no longer mention `graphmind {expected}`: {checked:?}");
    }
}

#[test]
fn listed_flags_reads_only_the_options_section() {
    let help = "Search symbols\n\nUsage: graphmind search [OPTIONS] <QUERY>\n\nArguments:\n  <QUERY>  Use --limit to cap\n\nOptions:\n  -l, --limit <LIMIT>  Max results, see --offset\n      --slug <SLUG>    Project slug\n                       (wraps --kind)\n  -h, --help           Print help\n";
    assert_eq!(listed_flags(help), ["--limit", "--slug", "--help"]);
}

/// What the skill relies on: `graphmind which` tells inside from outside, and outside every
/// project a command with `--slug` answers from that project.
#[test]
fn commands_target_a_project_by_slug_from_outside_it() {
    let env = CliTestEnv::new();
    let slug = env.setup_project();
    let other = tempfile::tempdir().unwrap();
    env.run_ok(&["register", other.path().to_str().unwrap(), "--slug", "other-project"]);
    let outside = tempfile::tempdir().unwrap();

    let which = env.run_in(outside.path(), &["which"]);
    assert_eq!(which.status.code(), Some(1));
    assert!(which.stdout.is_empty(), "{}", String::from_utf8_lossy(&which.stdout));
    assert_eq!(env.run_ok_in(&env.fixture_path(), &["which"]).trim(), slug);

    let list = env.run_ok_in(outside.path(), &["list"]);
    assert!(list.contains(&slug), "{list}");

    for command in [
        vec!["status", "--slug", &slug],
        vec!["map", "--slug", &slug],
        vec!["cycles", "--slug", &slug],
        vec!["export", "--slug", &slug, "--format", "mermaid"],
        vec!["search", "main", "--slug", &slug],
    ] {
        env.run_ok_in(outside.path(), &command);
    }
}

#[test]
fn build_takes_slug_from_outside_the_project() {
    let env = CliTestEnv::new();
    env.run_ok(&["register", env.fixture_path().to_str().unwrap(), "--slug", "build-me"]);
    let outside = tempfile::tempdir().unwrap();
    let out = env.run_ok_in(outside.path(), &["build", "--slug", "build-me"]);
    assert!(out.contains("Building build-me at"), "{out}");
    let status = env.run_ok_in(outside.path(), &["status", "--slug", "build-me"]);
    assert!(status.contains("Last build:") && !status.contains("Last build: never"), "{status}");
}

/// As the skill says: outside a project, `memory search` without `--slug` succeeds and reads
/// global memory only; with `--slug` it finds the project's facts.
#[test]
fn memory_search_needs_slug_from_outside_the_project() {
    let env = CliTestEnv::new();
    env.run_ok(&["register", env.fixture_path().to_str().unwrap(), "--slug", "remembers"]);
    let other = tempfile::tempdir().unwrap();
    env.run_ok(&["register", other.path().to_str().unwrap(), "--slug", "other-project"]);
    let outside = tempfile::tempdir().unwrap();
    env.run_ok_in(outside.path(), &["memory", "add", "zebra retry policy is per tenant", "--slug", "remembers"]);
    env.run_ok_in(outside.path(), &["memory", "add", "zebra commits are signed", "--global"]);

    let without = env.run_ok_in(outside.path(), &["memory", "search", "zebra"]);
    assert!(without.contains("zebra commits are signed"), "{without}");
    assert!(!without.contains("per tenant"), "{without}");

    let with = env.run_ok_in(outside.path(), &["memory", "search", "zebra", "--slug", "remembers"]);
    assert!(with.contains("zebra retry policy is per tenant"), "{with}");
}

/// The positional SLUG and `--slug` name the same thing: giving both is rejected.
#[test]
fn positional_slug_and_slug_flag_together_are_rejected() {
    let env = CliTestEnv::new();
    for command in ["status", "build", "map", "cycles", "export"] {
        let out = env.run(&[command, "one", "--slug", "two"]);
        assert!(!out.status.success(), "{command}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("cannot be used with"), "{command}: {stderr}");
    }
}
