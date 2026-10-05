//! Keep a project's embedding store in step with its graph.
//!
//! Symbols are matched by `(file, name, kind, text)`, where `text` is what gets embedded
//! (signature + start of the body). A symbol whose text changed is re-embedded, a stored row
//! with no matching symbol is deleted, and a symbol that failed to embed in an earlier run is
//! picked up again, so one bad build never leaves a permanent gap.

use crate::engine::EmbeddingEngine;
use crate::resilient::{embed_resilient_with, Breaker};
use crate::store::{float32_to_bytes, EmbeddingStore, NewEmbeddingRow};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// One symbol to embed: identity plus the exact text sent to the model.
#[derive(Debug, Clone)]
pub struct EmbedInput {
    pub symbol_name: String,
    pub symbol_kind: String,
    pub file: String,
    pub text: String,
}

/// Build the text embedded for a symbol. Kept in one place so every caller (CLI, watch mode,
/// desktop app) produces the same text and change detection stays stable.
pub fn symbol_text(kind: &str, name: &str, file: &str, signature: Option<&str>, content: Option<&str>) -> String {
    let mut t = format!("{kind} {name} ({file})");
    if let Some(sig) = signature {
        t.push('\n');
        t.push_str(sig);
    }
    if let Some(content) = content {
        t.push('\n');
        t.extend(content.chars().take(512));
    }
    t
}

#[derive(Debug, Default)]
pub struct SyncPlan {
    /// Inputs that have no up-to-date row in the store.
    pub to_embed: Vec<EmbedInput>,
    /// Store row ids that no longer match any symbol.
    pub stale_ids: Vec<i64>,
}

type Key = (String, String, String, String);

fn key(file: &str, name: &str, kind: &str, text: &str) -> Key {
    (file.to_string(), name.to_string(), kind.to_string(), text.to_string())
}

/// Compare the wanted symbols with what is stored. `scope` limits the comparison to rows of
/// these files (watch mode); `None` compares the whole store (full build).
pub fn plan(store: &EmbeddingStore, wanted: Vec<EmbedInput>, scope: Option<&[String]>) -> SyncPlan {
    // Multiset of stored rows: the same symbol can legitimately appear more than once in a file.
    let mut stored: HashMap<Key, Vec<i64>> = HashMap::new();
    for (id, file, name, kind, text) in store.index_rows(scope) {
        stored.entry(key(&file, &name, &kind, &text)).or_default().push(id);
    }
    let mut to_embed = Vec::new();
    for input in wanted {
        let k = key(&input.file, &input.symbol_name, &input.symbol_kind, &input.text);
        match stored.get_mut(&k).and_then(|ids| ids.pop()) {
            Some(_) => {}
            None => to_embed.push(input),
        }
    }
    let stale_ids = stored.into_values().flatten().collect();
    SyncPlan { to_embed, stale_ids }
}

#[derive(Debug, Default)]
pub struct EmbedSummary {
    pub requested: usize,
    pub embedded: usize,
    pub failed: usize,
    pub first_error: Option<String>,
    pub cancelled: bool,
}

/// Embed `inputs` in batches of `batch_size`, splitting failed batches (see
/// [`crate::resilient`]), and insert every vector that came back. `progress` is called after
/// each batch with `(done, total)`.
pub fn embed_and_store(
    engine: &dyn EmbeddingEngine,
    store: &EmbeddingStore,
    inputs: &[EmbedInput],
    batch_size: usize,
    cancel: Option<&AtomicBool>,
    mut progress: impl FnMut(&EmbedSummary),
) -> EmbedSummary {
    let mut summary = EmbedSummary { requested: inputs.len(), ..Default::default() };
    let batch_size = batch_size.max(1);
    let mut breaker = Breaker::for_batch_size(batch_size);
    for chunk in inputs.chunks(batch_size) {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            summary.cancelled = true;
            break;
        }
        let texts: Vec<&str> = chunk.iter().map(|i| i.text.as_str()).collect();
        let batch = embed_resilient_with(engine, &texts, &mut breaker);
        if summary.first_error.is_none() {
            summary.first_error = batch.first_error.clone();
        }
        let rows: Vec<NewEmbeddingRow> = chunk
            .iter()
            .zip(&batch.vectors)
            .filter_map(|(input, v)| {
                v.as_ref().map(|v| NewEmbeddingRow {
                    symbol_name: input.symbol_name.clone(),
                    symbol_kind: input.symbol_kind.clone(),
                    file: input.file.clone(),
                    text: input.text.clone(),
                    embedding: float32_to_bytes(v),
                })
            })
            .collect();
        match store.insert_batch(&rows) {
            Ok(()) => {
                summary.embedded += rows.len();
                summary.failed += chunk.len() - rows.len();
            }
            Err(e) => {
                summary.failed += chunk.len();
                if summary.first_error.is_none() {
                    summary.first_error = Some(format!("store insert: {e}"));
                }
            }
        }
        progress(&summary);
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EmbedError;

    struct Fake {
        fail_text: Option<&'static str>,
    }
    impl EmbeddingEngine for Fake {
        fn embed(&self, t: &str) -> Result<Vec<f32>, EmbedError> {
            self.embed_batch(&[t]).map(|mut v| v.remove(0))
        }
        fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
            if let Some(bad) = self.fail_text {
                if texts.iter().any(|t| t.contains(bad)) {
                    return Err(EmbedError::ApiError("boom".into()));
                }
            }
            Ok(texts.iter().map(|t| vec![t.len() as f32, 1.0]).collect())
        }
        fn dimensions(&self) -> usize { 2 }
        fn model_id(&self) -> &str { "fake" }
        fn provider_name(&self) -> &str { "test" }
        fn is_available(&self) -> bool { true }
    }

    fn input(file: &str, name: &str, body: &str) -> EmbedInput {
        EmbedInput {
            symbol_name: name.into(),
            symbol_kind: "function".into(),
            file: file.into(),
            text: symbol_text("function", name, file, None, Some(body)),
        }
    }

    fn store() -> (tempfile::TempDir, EmbeddingStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = EmbeddingStore::open(&dir.path().join("e.db")).unwrap();
        (dir, s)
    }

    fn sync(store: &EmbeddingStore, engine: &Fake, wanted: Vec<EmbedInput>) -> EmbedSummary {
        let p = plan(store, wanted, None);
        store.delete_ids(&p.stale_ids).unwrap();
        embed_and_store(engine, store, &p.to_embed, 4, None, |_| {})
    }

    #[test]
    fn unchanged_symbols_are_not_re_embedded() {
        let (_d, s) = store();
        let e = Fake { fail_text: None };
        let wanted = vec![input("a.rs", "f", "1"), input("a.rs", "g", "2")];
        assert_eq!(sync(&s, &e, wanted.clone()).embedded, 2);
        assert_eq!(plan(&s, wanted, None).to_embed.len(), 0);
    }

    #[test]
    fn edited_symbol_is_re_embedded_and_old_row_removed() {
        let (_d, s) = store();
        let e = Fake { fail_text: None };
        sync(&s, &e, vec![input("a.rs", "f", "old"), input("a.rs", "g", "same")]);
        let r = sync(&s, &e, vec![input("a.rs", "f", "new"), input("a.rs", "g", "same")]);
        assert_eq!(r.embedded, 1, "only the edited symbol");
        let rows = s.all().unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.text.ends_with("new")));
        assert!(!rows.iter().any(|r| r.text.ends_with("old")));
    }

    #[test]
    fn failed_symbols_are_retried_on_next_sync() {
        let (_d, s) = store();
        let wanted: Vec<_> = (0..20).map(|i| input("a.rs", &format!("f{i}"), if i == 7 { "POISON" } else { "ok" })).collect();
        let r = sync(&s, &Fake { fail_text: Some("POISON") }, wanted.clone());
        // Batches of 4: the poisoned batch is split down to the one bad symbol.
        assert_eq!(r.failed, 1, "only the poisoned symbol is lost");
        assert_eq!(r.embedded, 19);
        assert!(r.first_error.unwrap().contains("boom"));
        let r = sync(&s, &Fake { fail_text: None }, wanted);
        assert_eq!((r.requested, r.embedded), (1, 1), "the gap is filled next time");
        assert_eq!(s.count().unwrap(), 20);
    }

    #[test]
    fn duplicate_symbols_in_one_file_are_counted() {
        let (_d, s) = store();
        let e = Fake { fail_text: None };
        let wanted = vec![input("a.rs", "f", "x"), input("a.rs", "f", "x")];
        assert_eq!(sync(&s, &e, wanted.clone()).embedded, 2);
        assert_eq!(sync(&s, &e, wanted[..1].to_vec()).embedded, 0);
        assert_eq!(s.count().unwrap(), 1, "the extra copy is removed");
    }

    #[test]
    fn scoped_plan_leaves_other_files_alone() {
        let (_d, s) = store();
        let e = Fake { fail_text: None };
        sync(&s, &e, vec![input("a.rs", "f", "1"), input("b.rs", "g", "2")]);
        let p = plan(&s, vec![input("a.rs", "f", "changed")], Some(&["a.rs".to_string()]));
        assert_eq!(p.to_embed.len(), 1);
        assert_eq!(p.stale_ids.len(), 1, "only a.rs's old row");
    }

    #[test]
    fn cancel_stops_before_next_batch() {
        let (_d, s) = store();
        let cancel = AtomicBool::new(true);
        let inputs: Vec<_> = (0..10).map(|i| input("a.rs", &format!("f{i}"), "x")).collect();
        let r = embed_and_store(&Fake { fail_text: None }, &s, &inputs, 4, Some(&cancel), |_| {});
        assert!(r.cancelled);
        assert_eq!(r.embedded, 0);
    }
}
