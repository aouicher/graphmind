//! Auto-promotion only counts deliberate searches, and its threshold is configurable.

mod common;
use common::CliTestEnv;

fn entries(env: &CliTestEnv) -> Vec<serde_json::Value> {
    let path = env.home.path().join(".graphmind/memory/global.jsonl");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn set_threshold(env: &CliTestEnv, n: u32) {
    let path = env.home.path().join(".graphmind/config.json");
    let mut config: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({
            "version": "1", "projects": {}, "global_exclude": [],
            "defaults": {"embedding_model": "minilm", "watch_debounce": 2000, "max_depth": 5, "exclude_tests": true},
            "mcp": {"transport": "stdio", "http_port": 37378, "restrict_to_projects": null}
        }));
    config["memory"] = serde_json::json!({ "auto_promote_min_recalls": n });
    std::fs::write(&path, config.to_string()).unwrap();
}

#[test]
fn hook_searches_do_not_count_as_recalls() {
    let env = CliTestEnv::new();
    env.run_ok(&["memory", "add", "zebra stripes are unique", "--global"]);
    for _ in 0..5 {
        env.run_ok(&["memory", "search", "zebra", "--no-recall"]);
    }
    env.run_ok(&["memory", "consolidate"]);
    let e = &entries(&env)[0];
    assert_eq!(e["recall_count"], 0);
    assert_eq!(e["priority"], false);
}

#[test]
fn deliberate_recalls_promote_at_threshold_and_mark_promoted() {
    let env = CliTestEnv::new();
    env.run_ok(&["memory", "add", "zebra stripes are unique", "--global"]);
    for _ in 0..3 {
        env.run_ok(&["memory", "search", "zebra"]);
    }
    env.run_ok(&["memory", "consolidate"]);
    let e = &entries(&env)[0];
    assert_eq!(e["recall_count"], 3);
    assert_eq!(e["priority"], true);
    assert_eq!(e["promoted"], true);
}

#[test]
fn threshold_zero_turns_auto_promotion_off() {
    let env = CliTestEnv::new();
    set_threshold(&env, 0);
    env.run_ok(&["memory", "add", "zebra stripes are unique", "--global"]);
    for _ in 0..4 {
        env.run_ok(&["memory", "search", "zebra"]);
    }
    let out = env.run_ok(&["memory", "consolidate"]);
    assert!(out.contains("0 auto-promoted"), "{out}");
    assert_eq!(entries(&env)[0]["priority"], false);
}

#[test]
fn no_recall_is_accepted_but_hidden_from_help() {
    let env = CliTestEnv::new();
    let help = env.run_ok(&["memory", "search", "--help"]);
    assert!(!help.contains("no-recall"), "{help}");
    env.run_ok(&["memory", "search", "zebra", "--no-recall"]);
}

/// Entries promoted by a version that didn't record `promoted` are marked on the first
/// consolidate, and listed after entries saved as priority.
#[test]
fn first_consolidate_marks_earlier_auto_promotions() {
    let env = CliTestEnv::new();
    env.run_ok(&["memory", "add", "saved on purpose", "--global", "--priority"]);
    let path = env.home.path().join(".graphmind/memory/global.jsonl");
    let legacy = serde_json::json!({
        "id": "legacy-1", "created": "2026-01-01T00:00:00+00:00", "updated": "2026-02-01T00:00:00+00:00",
        "project": null, "global": true, "type": "context", "content": "promoted by hook searches",
        "tags": [], "session": "2026-01-01", "priority": true, "recall_count": 40
    });
    let mut raw = std::fs::read_to_string(&path).unwrap();
    raw.push_str(&format!("{legacy}\n"));
    std::fs::write(&path, raw).unwrap();

    let out = env.run_ok(&["memory", "consolidate"]);
    assert!(out.contains("1 earlier auto-promotions marked"), "{out}");
    let legacy = entries(&env).into_iter().find(|e| e["id"] == "legacy-1").unwrap();
    assert_eq!((legacy["priority"].as_bool(), legacy["promoted"].as_bool()), (Some(true), Some(true)));

    let listed = env.run_ok(&["memory", "list", "--priority"]);
    let (pinned, promoted) = (listed.find("saved on purpose").unwrap(), listed.find("promoted by hook").unwrap());
    assert!(pinned < promoted, "{listed}");

    let again = env.run_ok(&["memory", "consolidate"]);
    assert!(!again.contains("earlier auto-promotions"), "{again}");
}
