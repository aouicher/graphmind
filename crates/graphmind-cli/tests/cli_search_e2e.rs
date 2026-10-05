mod common;
use common::QueryTestEnv;

#[test]
fn search_with_punctuation_and_keywords_still_finds_symbols() {
    let env = QueryTestEnv::new();
    // Unquoted, these were FTS5 syntax errors and the search returned nothing.
    for query in ["getBalance()", "AND getBalance", "wallet.getBalance"] {
        let out = env.run_ok(&["search", query, "--in", &env.slug]);
        assert!(out.contains("getBalance"), "search {query:?} should find getBalance, got: {out}");
    }
}

#[test]
fn search_ranking_is_stable_across_runs() {
    let env = QueryTestEnv::new();
    let first = env.run_ok(&["search", "wallet balance", "--in", &env.slug, "--limit", "10"]);
    for _ in 0..5 {
        let again = env.run_ok(&["search", "wallet balance", "--in", &env.slug, "--limit", "10"]);
        assert_eq!(first, again, "same query must give the same ranking");
    }
}

#[test]
fn search_offset_pages_past_the_first_results() {
    let env = QueryTestEnv::new();
    let all = env.run_ok(&["search", "wallet", "--in", &env.slug, "--limit", "4"]);
    let page2 = env.run_ok(&["search", "wallet", "--in", &env.slug, "--limit", "2", "--offset", "2"]);
    let lines = |s: &str| s.lines().filter(|l| l.contains(':')).map(str::to_string).collect::<Vec<_>>();
    let all = lines(&all);
    let page2 = lines(&page2);
    assert!(all.len() >= 4, "fixture should have 4+ wallet hits: {all:?}");
    assert!(!page2.is_empty(), "offset 2 should still return hits");
    assert!(all[2..].iter().any(|l| page2.contains(l)), "page 2 should continue page 1: {all:?} vs {page2:?}");
}
