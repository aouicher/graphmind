//! Memory upkeep behind `memory clean`, `memory consolidate`, `list --clean` and the daily
//! background clean: expire, drop `[commit]` noise, merge near-duplicates, auto-promote.

use std::collections::HashSet;
use std::num::NonZeroU32;

use crate::store::{MemoryEntry, MemoryStore, Priority};

pub struct Policy {
    /// Promote a normal entry once deliberate searches have recalled it this many times;
    /// `None` turns auto-promotion off.
    pub promote_after: Option<NonZeroU32>,
    /// Count what would change without writing anything.
    pub dry_run: bool,
}

/// What a consolidate run changed (or would change, on a dry run).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub expired: usize,
    pub commits: usize,
    pub duplicates: usize,
    pub promoted: usize,
    /// Entries auto-promoted by an earlier version, now marked as such: on the first run after
    /// upgrading, and again whenever a process still running that version rewrote a file and
    /// dropped the flag.
    pub legacy_marked: usize,
}

/// Consolidates the global file and `project`'s file, or every memory file when `project` is
/// `None`. Each file is one locked read-modify-write; a file that can't be read or written is
/// left as it is and counts nothing.
pub fn consolidate(store: &MemoryStore, project: Option<&str>, policy: &Policy) -> Report {
    let now = chrono::Utc::now();
    let mut report = Report {
        legacy_marked: mark_legacy_promotions(store, now, policy.dry_run),
        ..Report::default()
    };

    let paths = match project {
        Some(_) => store.files(project),
        None => store.all_files(),
    };
    for path in paths.iter().filter(|p| p.exists()) {
        let counts = store.update_file(path, |entries| {
            let file = consolidate_entries(entries, now, policy.promote_after);
            let changed = !policy.dry_run && file != Report::default();
            (file, changed)
        });
        let Some(file) = counts else { continue };
        report.expired += file.expired;
        report.commits += file.commits;
        report.duplicates += file.duplicates;
        report.promoted += file.promoted;
    }
    report
}

/// Steps A (expire), B (`[commit]` purge), C (near-duplicate merge), D (auto-promote) on one
/// file's entries.
fn consolidate_entries(
    entries: &mut Vec<MemoryEntry>,
    now: chrono::DateTime<chrono::Utc>,
    promote_after: Option<NonZeroU32>,
) -> Report {
    let before = entries.len();

    // Step A — purge expired entries
    entries.retain(|e| {
        if let Some(ref exp) = e.expires_at {
            if let Ok(exp_dt) = chrono::DateTime::parse_from_rfc3339(exp) {
                return exp_dt.with_timezone(&chrono::Utc) > now;
            }
        }
        true
    });
    let after_a = entries.len();

    // Step B — purge [commit] entries
    entries.retain(|e| !e.content.starts_with("[commit]"));
    let after_b = entries.len();

    // Step C — content-based dedup (Jaccard > 0.85)
    let dedup_ids = find_dedup_ids(entries);
    if !dedup_ids.is_empty() {
        entries.retain(|e| !dedup_ids.contains(&e.id));
    }

    // Step D — auto-promote entries recalled `promote_after` times
    let mut promoted = 0;
    if let Some(threshold) = promote_after {
        for entry in entries.iter_mut() {
            if entry.recall_count >= threshold.get() && entry.promote() {
                entry.updated = now.to_rfc3339();
                promoted += 1;
            }
        }
    }

    Report {
        expired: before - after_a,
        commits: after_a - after_b,
        duplicates: dedup_ids.len(),
        promoted,
        legacy_marked: 0,
    }
}

/// Remedy for installs upgraded from a version that didn't record `promoted`: every automatic
/// search counted as a recall, so most entries were promoted and now look pinned. Marks them
/// auto-promoted (they stay priority, ordered after pinned ones) in every memory file. It runs
/// on every consolidate, not once: a process still running the earlier version (an MCP server
/// started before the upgrade) rewrites files without the flag, and the entries it touched are
/// marked again. The cutoff keeps entries this version wrote out of it (see
/// `MemoryEntry::mark_legacy_promotion`). A file that can't be updated is retried next time.
/// Without a cutoff (the stamp can't be read or written) nothing is marked: entries stay pinned.
///
/// Until processes still running the earlier version restart, their rewrites also drop what
/// this version records in that window: an entry it auto-promotes stays pinned, a demotion is
/// lost (and the entry can be promoted again), and a legacy entry edited through `update` keeps
/// its pin, because each was written after the cutoff.
fn mark_legacy_promotions(store: &MemoryStore, now: chrono::DateTime<chrono::Utc>, dry_run: bool) -> usize {
    // A dry run doesn't record the cutoff; without one it counts against `now`.
    let cutoff = if dry_run { store.legacy_cutoff().or(Some(now)) } else { store.record_legacy_cutoff(now) };
    let Some(cutoff) = cutoff else { return 0 };
    let mut marked = 0;
    for path in store.all_files().iter().filter(|p| p.exists()) {
        // On a dry run the marks are counted but not written: the file is unchanged.
        let in_file = store.update_file(path, |entries| {
            let mut n = 0;
            for entry in entries.iter_mut() {
                if entry.mark_legacy_promotion(cutoff) {
                    n += 1;
                }
            }
            (n, !dry_run && n > 0)
        });
        marked += in_file.unwrap_or(0);
    }
    marked
}

/// Dedup keeps a deliberate state over recall counts: a pinned entry or a demotion (whose count
/// restarted at zero) must not disappear behind a near-duplicate that was searched more often.
fn dedup_rank(entry: &MemoryEntry) -> u8 {
    match entry.priority() {
        Priority::Pinned | Priority::Demoted => 2,
        Priority::Promoted => 1,
        Priority::Normal => 0,
    }
}

/// Identify IDs to remove for near-duplicate entries (Jaccard > 0.85).
/// Keeps the entry with the higher `dedup_rank`, then the higher recall_count, then the most
/// recent created.
fn find_dedup_ids(entries: &[MemoryEntry]) -> HashSet<String> {
    let mut to_remove: HashSet<String> = HashSet::new();

    for i in 0..entries.len() {
        if to_remove.contains(&entries[i].id) {
            continue;
        }
        for j in (i + 1)..entries.len() {
            if to_remove.contains(&entries[j].id) {
                continue;
            }
            let sim = content_jaccard(&entries[i].content, &entries[j].content);
            if sim > 0.85 {
                let key = |e: &MemoryEntry| (dedup_rank(e), e.recall_count);
                let keep_i = key(&entries[i]) > key(&entries[j])
                    || (key(&entries[i]) == key(&entries[j]) && entries[i].created >= entries[j].created);
                if keep_i {
                    to_remove.insert(entries[j].id.clone());
                } else {
                    to_remove.insert(entries[i].id.clone());
                    break;
                }
            }
        }
    }
    to_remove
}

/// Token-level Jaccard similarity between two content strings.
fn content_jaccard(a: &str, b: &str) -> f64 {
    let tokens_a: HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: HashSet<&str> = b.split_whitespace().collect();
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    let intersection = tokens_a.intersection(&tokens_b).count();
    let union_count = tokens_a.union(&tokens_b).count();
    if union_count == 0 {
        return 0.0;
    }
    intersection as f64 / union_count as f64
}
