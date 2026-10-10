//! Remedy for installs upgraded from a version that didn't record `promoted`: every automatic
//! search counted as a recall, so most entries were auto-promoted and now look pinned. Kept apart
//! from the store so it can be dropped once those installs are gone.

use super::{MemoryEntry, MemoryStore, Priority};
use chrono::{DateTime, Utc};
use std::fs;
use std::io::Write;
use std::path::Path;

/// Recall count at which consolidate promoted entries before `promoted` was recorded.
const LEGACY_PROMOTE_AT: u32 = 3;

/// In the memory directory: the time (RFC 3339) this version first consolidated or edited
/// memory. Entries last written before it were written by a version without `promoted`.
const LEGACY_CUTOFF_STAMP: &str = ".legacy-promotions-marked";

impl MemoryEntry {
    /// Marks an entry auto-promoted by a version without `promoted`, i.e. last written before
    /// `cutoff` (see `MemoryStore::record_legacy_cutoff`). Those versions only wrote `updated`
    /// when they promoted an entry, at a recall count of 3, so such an entry with
    /// `updated != created` and enough recalls was promoted, while one saved as priority still
    /// has `updated == created`. An entry this version wrote has `updated` at or after the cutoff
    /// and is never marked: a pin through `update` stays a pin. Marking leaves `updated` as it
    /// is, so an entry an earlier version rewrites without the flag is marked again. Returns
    /// whether it changed.
    pub(crate) fn mark_legacy_promotion(&mut self, cutoff: DateTime<Utc>) -> bool {
        let written_before_cutoff = DateTime::parse_from_rfc3339(&self.updated)
            .map(|updated| updated < cutoff)
            .unwrap_or(false);
        let legacy = self.priority() == Priority::Pinned
            && self.recall_count >= LEGACY_PROMOTE_AT
            && self.updated != self.created
            && written_before_cutoff;
        if legacy {
            self.promoted = true;
        }
        legacy
    }
}

impl MemoryStore {
    /// The recorded legacy cutoff, if there is one. Never writes (for dry runs).
    pub(crate) fn legacy_cutoff(&self) -> Option<DateTime<Utc>> {
        stamp_time(&self.memory_dir.join(LEGACY_CUTOFF_STAMP))
    }

    /// The legacy cutoff, recorded as `now` if there is none yet; if another process records
    /// it first, its time is used. `None` when it can neither be read nor recorded: callers then
    /// skip the marking, since any cutoff they made up would mark later pins as promoted.
    ///
    /// The stamp is load-bearing: deleting it moves the cutoff to the next run, and entries
    /// pinned through `memory update` before that would then be marked as promoted.
    pub(crate) fn record_legacy_cutoff(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let stamp = self.memory_dir.join(LEGACY_CUTOFF_STAMP);
        if let Some(cutoff) = stamp_time(&stamp) {
            return Some(cutoff);
        }
        match create_stamp(&stamp, now) {
            Ok(()) => Some(now),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => stamp_time(&stamp),
            Err(e) => {
                eprintln!("Warning: legacy promotions not marked: couldn't write {}: {e}", stamp.display());
                None
            }
        }
    }
}

/// The time a stamp file records, or its modification time if its content isn't one (empty, or
/// read while being created). `None` if there is no readable stamp.
fn stamp_time(path: &Path) -> Option<DateTime<Utc>> {
    let meta = fs::metadata(path).ok()?;
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw.trim()).ok())
        .map(|time| time.with_timezone(&Utc))
        .or_else(|| meta.modified().ok().map(Into::into))
}

/// Create the stamp holding `time` without replacing another process's: written to a temp file,
/// then hard-linked into place (complete or not at all), which fails with `AlreadyExists` if a
/// stamp is there. Where hard links aren't supported (exFAT, some network shares) it is created
/// with `create_new` instead; a reader that finds it still empty uses its modification time.
fn create_stamp(path: &Path, time: DateTime<Utc>) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), time.timestamp_nanos_opt().unwrap_or_default()));
    let linked = fs::write(&tmp, time.to_rfc3339()).and_then(|_| fs::hard_link(&tmp, path));
    fs::remove_file(&tmp).ok();
    match linked {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => create_new_stamp(path, time),
        result => result,
    }
}

fn create_new_stamp(path: &Path, time: DateTime<Utc>) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(time.to_rfc3339().as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_new_stamp_writes_the_time_and_never_replaces_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LEGACY_CUTOFF_STAMP);
        let time = Utc::now();
        create_new_stamp(&path, time).unwrap();
        assert_eq!(stamp_time(&path).unwrap().to_rfc3339(), time.to_rfc3339());
        let err = create_new_stamp(&path, time + chrono::Duration::hours(1)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(stamp_time(&path).unwrap().to_rfc3339(), time.to_rfc3339(), "first stamp kept");
    }
}
