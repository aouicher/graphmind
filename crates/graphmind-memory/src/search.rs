use crate::identifiers::same_thing;
use crate::store::MemoryEntry;
use crate::terms::{is_stopword, query_terms, tokens, Term};

/// An entry's searchable text, lower-cased once per search.
struct EntryText {
    content: String,
    /// Content tokens; only computed when a term needs them (names, terms under 3 characters).
    tokens: Vec<String>,
    tags: Vec<String>,
    entry_type: String,
}

impl EntryText {
    fn new(entry: &MemoryEntry, with_tokens: bool) -> Self {
        let content = entry.content.to_lowercase();
        Self {
            tokens: if with_tokens { tokens(&content) } else { Vec::new() },
            content,
            tags: entry.tags.iter().map(|t| t.to_lowercase()).collect(),
            entry_type: entry.entry_type.to_string(),
        }
    }
}

/// Points `term` earns on `entry`; 0 means it doesn't match.
/// - +1 when it appears in the text (as a whole word if under 3 characters, anywhere otherwise),
///   plus 0.1 per occurrence up to 3, so long entries can't win by repeating common words.
/// - +2 when it names one of the entry's tags, or when it is written like a name (path,
///   camelCase, ENV_VAR, ...) and names a word of the text. A path matches its basename and a
///   scoped name its parts, both ways.
/// - +1.5 when it is the entry's type (`decision`, `bug`, ...).
fn term_score(term: &Term, entry: &EntryText) -> f64 {
    let mut score = 0.0;
    let hits = if !term.short() {
        entry.content.matches(term.text.as_str()).count()
    } else {
        entry.tokens.iter().filter(|t| **t == term.text).count()
    };
    if hits > 0 {
        score += 1.0 + 0.1 * hits.min(3) as f64;
    }
    let names = |words: &[String]| words.iter().any(|w| same_thing(&term.text, w));
    // A word of the text counts only when equal, or when it is long enough to name something:
    // `a::run` must not match the article in "write a test".
    let names_word = || {
        entry.tokens.iter().any(|w| *w == term.text || (w.chars().count() >= 3 && !is_stopword(w) && same_thing(&term.text, w)))
    };
    if names(&entry.tags) || (term.name && names_word()) {
        score += 2.0;
    }
    if entry.entry_type == term.text {
        score += 1.5;
    }
    score
}

/// Rank memory entries for a search query: the sum of `term_score` over the query's terms
/// (stopwords dropped). With three or more terms an entry must match at least two of them, so
/// one shared word doesn't make it related. Equal scores: newest first, then by id. A blank
/// query lists the newest entries; a query of only stopwords ("yes please") finds nothing.
pub fn search(entries: &[MemoryEntry], query: &str, limit: usize) -> Vec<MemoryEntry> {
    if query.trim().is_empty() {
        return entries.iter().take(limit).cloned().collect();
    }
    let terms = query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let need_tokens = terms.iter().any(Term::needs_tokens);
    let required = if terms.len() >= 3 { 2 } else { 1 };

    let mut scored: Vec<(f64, &MemoryEntry)> = entries
        .iter()
        .filter_map(|entry| {
            let text = EntryText::new(entry, need_tokens);
            let scores: Vec<f64> = terms.iter().map(|term| term_score(term, &text)).collect();
            let matched = scores.iter().filter(|s| **s > 0.0).count();
            (matched >= required).then(|| (scores.iter().sum(), entry))
        })
        .collect();

    scored.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| b.1.created.cmp(&a.1.created))
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    scored.into_iter().take(limit).map(|(_, e)| e.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{AddOptions, MemoryStore};

    fn store_with(facts: &[&str]) -> (tempfile::TempDir, Vec<MemoryEntry>) {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new(dir.path());
        for f in facts {
            store.add(f, AddOptions::default());
        }
        let entries = store.list(None);
        (dir, entries)
    }

    #[test]
    fn stopwords_do_not_let_long_filler_facts_win() {
        let long = format!("Auth0 setup notes. {}", "the user in the tenant is the one that is in the app and the flow. ".repeat(20));
        let (_d, entries) = store_with(&[&long, "report-worker server is idle after 18:00 and can be shut down"]);
        let r = search(&entries, "is the report-worker server idle, can we shut it down", 5);
        assert_eq!(r[0].content, "report-worker server is idle after 18:00 and can be shut down");
        assert_eq!(r.len(), 1, "the filler fact shares only stopwords: {:?}", r.iter().map(|e| &e.content[..20]).collect::<Vec<_>>());
    }

    #[test]
    fn full_path_query_finds_fact_with_basename_and_back() {
        let (_d, entries) = store_with(&["retry-policy.ts decides when a payment is retried"]);
        assert_eq!(search(&entries, "src/models/routing/retry-policy.ts", 5).len(), 1);
        let (_d2, entries) = store_with(&["Retries live in src/models/routing/retry-policy.ts"]);
        assert_eq!(search(&entries, "retry-policy.ts retries", 5).len(), 1);
    }

    #[test]
    fn identifiers_in_text_score_like_tags() {
        let (_d, entries) = store_with(&[
            "Testing stance: integration tests stub fetchInBatches and assert pendingKeys retries",
            "Testing stance for the UI: snapshot tests are banned",
        ]);
        let r = search(&entries, "how do we test fetchInBatches pendingKeys", 5);
        assert!(r[0].content.contains("fetchInBatches"));
    }

    #[test]
    fn env_assignment_and_scoped_names_match() {
        let (_d, entries) = store_with(&[
            "FEATURE_GRADES={} still yields web-app:direct because the default grade applies",
            "web-app deploys on Fridays",
        ]);
        let r = search(&entries, "setting FEATURE_GRADES={} still gives web-app:direct", 5);
        assert!(r[0].content.starts_with("FEATURE_GRADES"), "{:?}", r[0].content);
    }

    #[test]
    fn short_terms_match_whole_words_only() {
        let (_d, entries) = store_with(&["Docker images are rebuilt nightly"]);
        assert!(search(&entries, "ui", 5).is_empty(), "'ui' must not match inside 'rebuilt'");
    }

    #[test]
    fn many_terms_need_two_matches() {
        let (_d, entries) = store_with(&["wallet balance is cached for 30 seconds", "the job runner retries three times"]);
        // Only "job" matches the second entry: not enough for a 4-term query.
        let r = search(&entries, "wallet balance cache job", 10);
        assert_eq!(r.iter().map(|e| e.content.as_str()).collect::<Vec<_>>(), ["wallet balance is cached for 30 seconds"]);
        // A short query still matches on one term.
        assert_eq!(search(&entries, "job", 10).len(), 1);
        // Natural-language filler finds nothing in an unrelated entry.
        let (_d2, entries) = store_with(&["this list of the other things"]);
        assert!(search(&entries, "what is the retry policy", 10).is_empty());
    }

    #[test]
    fn equal_scores_list_the_newest_first() {
        let (_d, mut entries) = store_with(&["alpha one", "alpha two"]);
        let newest = entries[0].content.clone();
        entries.reverse();
        let r = search(&entries, "alpha", 10);
        assert_eq!(r[0].content, newest);
    }

    /// Only a query term written like a name gets the +2 for naming a word of the text, so
    /// "make sure" doesn't turn "sure" into a name.
    #[test]
    fn only_names_score_like_tags() {
        let (_d, entries) = store_with(&["make sure the cache is warm", "Tests stub fetchInBatches; see retry-policy.ts"]);
        let score = |content: &str, text: &str, name: bool| {
            let entry = entries.iter().find(|e| e.content == content).unwrap();
            term_score(&Term { text: text.into(), name }, &EntryText::new(entry, true))
        };
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(score("make sure the cache is warm", "sure", false), 1.1));
        let names = "Tests stub fetchInBatches; see retry-policy.ts";
        assert!(close(score(names, "fetchinbatches", true), 3.1));
        assert!(close(score(names, "src/models/retry-policy.ts", true), 2.0), "path matches the basename");
        assert!(close(score(names, "stub", false), 1.1));
    }

    #[test]
    fn a_query_of_only_stopwords_finds_nothing() {
        let (_d, entries) = store_with(&["retry policy: exponential backoff", "web-app deploys on Fridays"]);
        for q in ["yes please", "why", "the", "ok, do it", "oui", "qu'est-ce que"] {
            assert!(search(&entries, q, 5).is_empty(), "{q:?} must not return the newest entries");
        }
        assert_eq!(search(&entries, "  ", 5).len(), 2, "a blank query still lists entries");
    }

    #[test]
    fn french_function_words_do_not_outrank_the_topic() {
        let (_d, entries) = store_with(&["qu'il pleuve, est-ce grave ? peut-être", "retry policy: exponential backoff"]);
        let r = search(&entries, "qu'est-ce que la retry policy", 5);
        assert_eq!(r.len(), 1, "{:?}", r.iter().map(|e| &e.content).collect::<Vec<_>>());
        assert_eq!(r[0].content, "retry policy: exponential backoff");
    }

    #[test]
    fn short_or_stopword_scope_parts_do_not_match() {
        let (_d, entries) = store_with(&["write a test first", "alpha::run starts the job"]);
        let r = search(&entries, "a::run", 5);
        assert_eq!(r[0].content, "alpha::run starts the job", "{:?}", r.iter().map(|e| &e.content).collect::<Vec<_>>());
        assert!(r.iter().all(|e| e.content != "write a test first"), "the article \"a\" isn't the scope");
    }
}
