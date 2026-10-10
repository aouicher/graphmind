//! Names in search queries: file paths, code names, env vars, scoped names, ticket ids.
//!
//! Facts are found by the words people search with, and the most reliable of those are exact
//! names (`fetchInBatches`, `FEATURE_GRADES`, `scripts/check.ts`, `web-app:direct`). A query term
//! written like a name that names something in an entry scores like one of its tags.

fn has_lower(s: &str) -> bool {
    s.chars().any(|c| c.is_lowercase())
}
fn has_upper(s: &str) -> bool {
    s.chars().any(|c| c.is_uppercase())
}

use crate::terms::is_stopword;

/// Whether a raw token from `tokens()` (as typed, before lower-casing) is written like a name.
/// A hyphenated token is one only when none of its parts is a function word: `retry-policy`
/// is, `peut-être`, `follow-up` and `built-in` aren't.
pub(crate) fn looks_like_name(raw: &str) -> bool {
    let letters = raw.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 {
        return false;
    }
    let camel = has_lower(raw) && raw.chars().skip(1).any(|c| c.is_uppercase());
    let env_or_snake = raw.contains('_');
    let path = raw.contains('/');
    let dotted = raw.contains('.');
    // Also ticket ids (ADR-0042).
    let kebab = raw.contains('-') && raw.split('-').all(|p| !p.is_empty() && !is_stopword(&p.to_lowercase()));
    let scoped = raw.contains(':');
    let shouty = raw.len() >= 4 && has_upper(raw) && !has_lower(raw);
    camel || env_or_snake || path || dotted || kebab || scoped || shouty
}

/// Whether lower-cased names `q` and `e` name the same thing: equal; one a path ending in the
/// other (`src/models/x.ts` vs `x.ts` or `models/x.ts`); or one a part of the other's scoped name
/// (`web-app` or `direct` vs `web-app:direct`).
pub(crate) fn same_thing(q: &str, e: &str) -> bool {
    if q == e {
        return true;
    }
    let (long, short) = if q.len() > e.len() { (q, e) } else { (e, q) };
    let suffix = long.ends_with(short) && matches!(long.as_bytes()[long.len() - short.len() - 1], b'/' | b':');
    let scope = long.starts_with(short) && long.as_bytes()[short.len()] == b':';
    suffix || scope
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_told_apart_from_words() {
        for name in [
            "fetchInBatches", "FEATURE_GRADES", "scripts/check-items.ts", "config.embedding.mode",
            "web-app", "web-app:direct", "@scope/pkg", "ADR-0042", "JSON",
        ] {
            assert!(looks_like_name(name), "{name} should be a name");
        }
        for word in ["tests", "Docker", "graphmind", "sure", "is", "x", "API", "est-ce", "peut-être", "follow-up", "built-in"] {
            assert!(!looks_like_name(word), "{word} should be a plain word");
        }
    }

    #[test]
    fn paths_match_their_basename_and_suffix() {
        assert!(same_thing("src/models/routing/retry-policy.ts", "retry-policy.ts"));
        assert!(same_thing("retry-policy.ts", "src/models/routing/retry-policy.ts"));
        assert!(same_thing("routing/retry-policy.ts", "src/models/routing/retry-policy.ts"));
        assert!(!same_thing("link-policy.ts", "src/models/retry-policy.ts"));
        assert!(!same_thing("src", "src/models/retry-policy.ts"), "a leading path segment is not the file");
    }

    #[test]
    fn scoped_names_match_their_parts() {
        assert!(same_thing("web-app", "web-app:direct"));
        assert!(same_thing("direct", "web-app:direct"));
        assert!(same_thing("test:e2e", "test:e2e"));
        assert!(!same_thing("web", "web-app:direct"));
        assert!(!same_thing("app:direct", "web-app:direct"));
    }
}
