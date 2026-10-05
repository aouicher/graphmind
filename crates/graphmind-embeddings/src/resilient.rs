//! Batch embedding that degrades gracefully instead of dropping a whole batch.
//!
//! A batch can fail for reasons that a smaller batch avoids: a request timeout on a slow local
//! server, a provider's per-request token limit, or one bad input. On failure the batch is split
//! in half and each half retried, down to `MIN_SPLIT` texts; only what still fails is reported.
//!
//! Splitting must not turn an outage into hundreds of slow calls, so a [`Breaker`] counts
//! failed calls since the last success. One bad input fails at most once per split level before
//! a sibling succeeds, so the breaker trips only after more consecutive failures than there are
//! levels; after that every remaining text is reported as failed without another request.

use crate::engine::{EmbedError, EmbeddingEngine};

/// Batches at or below this size are not split further when they fail.
pub const MIN_SPLIT: usize = 1;

/// Per-input outcome of [`embed_resilient`].
pub struct ResilientBatch {
    /// One entry per input text, in order: `Some(vector)` or `None` if it could not be embedded.
    pub vectors: Vec<Option<Vec<f32>>>,
    /// First error seen, for the summary line. `None` when everything succeeded.
    pub first_error: Option<String>,
}

impl ResilientBatch {
    pub fn failed(&self) -> usize {
        self.vectors.iter().filter(|v| v.is_none()).count()
    }
}

/// Stops requests after too many failures in a row. Share one across the batches of a run.
pub struct Breaker {
    consecutive: usize,
    limit: usize,
}

impl Breaker {
    /// A breaker for batches of up to `batch_size` texts.
    pub fn for_batch_size(batch_size: usize) -> Self {
        // Calls on the longest failing path: batch_size, /2, /4, ... down to MIN_SPLIT.
        let mut levels = 1;
        let mut n = batch_size.max(1);
        while n > MIN_SPLIT {
            n = n.div_ceil(2);
            levels += 1;
        }
        Self { consecutive: 0, limit: levels + 1 }
    }

    pub fn tripped(&self) -> bool {
        self.consecutive >= self.limit
    }
}

pub fn embed_resilient(engine: &dyn EmbeddingEngine, texts: &[&str]) -> ResilientBatch {
    embed_resilient_with(engine, texts, &mut Breaker::for_batch_size(texts.len()))
}

pub fn embed_resilient_with(engine: &dyn EmbeddingEngine, texts: &[&str], breaker: &mut Breaker) -> ResilientBatch {
    let mut vectors = Vec::with_capacity(texts.len());
    let mut first_error = None;
    embed_into(engine, texts, &mut vectors, &mut first_error, breaker);
    if breaker.tripped() && first_error.is_none() {
        first_error = Some("embedding stopped after repeated failures".into());
    }
    ResilientBatch { vectors, first_error }
}

fn embed_into(
    engine: &dyn EmbeddingEngine,
    texts: &[&str],
    out: &mut Vec<Option<Vec<f32>>>,
    first_error: &mut Option<String>,
    breaker: &mut Breaker,
) {
    if texts.is_empty() {
        return;
    }
    if breaker.tripped() {
        out.extend(std::iter::repeat_with(|| None).take(texts.len()));
        return;
    }
    let err: EmbedError = match engine.embed_batch(texts) {
        // A response with the wrong number of vectors can't be matched to its inputs.
        Ok(v) if v.len() == texts.len() => {
            breaker.consecutive = 0;
            out.extend(v.into_iter().map(Some));
            return;
        }
        Ok(v) => EmbedError::ApiError(format!("expected {} embeddings, got {}", texts.len(), v.len())),
        Err(e) => e,
    };
    breaker.consecutive += 1;
    if first_error.is_none() {
        *first_error = Some(err.to_string());
    }
    // A missing key or disabled provider fails the same way at any size: don't retry.
    if matches!(err, EmbedError::NotConfigured(_)) {
        breaker.consecutive = breaker.limit;
    }
    if texts.len() <= MIN_SPLIT || breaker.tripped() {
        out.extend(std::iter::repeat_with(|| None).take(texts.len()));
        return;
    }
    let mid = texts.len() / 2;
    embed_into(engine, &texts[..mid], out, first_error, breaker);
    embed_into(engine, &texts[mid..], out, first_error, breaker);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Fails any batch larger than `max`, and any batch containing "bad".
    struct FlakyEngine {
        max: usize,
        calls: AtomicUsize,
    }

    impl EmbeddingEngine for FlakyEngine {
        fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
            self.embed_batch(&[text]).map(|mut v| v.remove(0))
        }
        fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if texts.len() > self.max {
                return Err(EmbedError::ApiError("operation timed out".into()));
            }
            if texts.contains(&"bad") {
                return Err(EmbedError::ApiError("invalid input".into()));
            }
            Ok(texts.iter().map(|t| vec![t.len() as f32]).collect())
        }
        fn dimensions(&self) -> usize { 1 }
        fn model_id(&self) -> &str { "flaky" }
        fn provider_name(&self) -> &str { "test" }
        fn is_available(&self) -> bool { true }
    }

    fn texts(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("text {i}")).collect()
    }

    #[test]
    fn whole_batch_succeeds_in_one_call() {
        let e = FlakyEngine { max: 1000, calls: AtomicUsize::new(0) };
        let t = texts(256);
        let refs: Vec<&str> = t.iter().map(|s| s.as_str()).collect();
        let r = embed_resilient(&e, &refs);
        assert_eq!(r.vectors.len(), 256);
        assert_eq!(r.failed(), 0);
        assert!(r.first_error.is_none());
        assert_eq!(e.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn timeout_on_large_batch_recovers_everything_by_splitting() {
        let e = FlakyEngine { max: 64, calls: AtomicUsize::new(0) };
        let t = texts(256);
        let refs: Vec<&str> = t.iter().map(|s| s.as_str()).collect();
        let r = embed_resilient(&e, &refs);
        assert_eq!(r.failed(), 0, "splitting should recover every text");
        // Order is preserved: vector value encodes the input text length.
        for (v, t) in r.vectors.iter().zip(&t) {
            assert_eq!(v.as_ref().unwrap()[0], t.len() as f32);
        }
    }

    #[test]
    fn one_bad_input_loses_only_its_smallest_batch() {
        let e = FlakyEngine { max: 1000, calls: AtomicUsize::new(0) };
        let mut t = texts(256);
        t[100] = "bad".into();
        let refs: Vec<&str> = t.iter().map(|s| s.as_str()).collect();
        let r = embed_resilient(&e, &refs);
        assert_eq!(r.failed(), 1, "only the bad text is lost");
        assert!(r.vectors[100].is_none());
        assert!(r.first_error.unwrap().contains("invalid input"));
    }

    #[test]
    fn not_configured_is_not_retried() {
        struct Off(AtomicUsize);
        impl EmbeddingEngine for Off {
            fn embed(&self, _: &str) -> Result<Vec<f32>, EmbedError> { unreachable!() }
            fn embed_batch(&self, _: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Err(EmbedError::NotConfigured("no key".into()))
            }
            fn dimensions(&self) -> usize { 1 }
            fn model_id(&self) -> &str { "off" }
            fn provider_name(&self) -> &str { "test" }
            fn is_available(&self) -> bool { false }
        }
        let e = Off(AtomicUsize::new(0));
        let t = texts(256);
        let refs: Vec<&str> = t.iter().map(|s| s.as_str()).collect();
        let r = embed_resilient(&e, &refs);
        assert_eq!(r.failed(), 256);
        assert_eq!(e.0.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn outage_trips_breaker_instead_of_splitting_everything() {
        let e = FlakyEngine { max: 0, calls: AtomicUsize::new(0) };
        let t = texts(256);
        let refs: Vec<&str> = t.iter().map(|s| s.as_str()).collect();
        let mut breaker = Breaker::for_batch_size(256);
        let r = embed_resilient_with(&e, &refs, &mut breaker);
        assert_eq!(r.failed(), 256);
        let calls = e.calls.load(Ordering::Relaxed);
        assert!(calls <= 11, "outage made {calls} calls");
        // Later batches in the same run make no calls at all.
        let r2 = embed_resilient_with(&e, &refs, &mut breaker);
        assert_eq!(r2.failed(), 256);
        assert_eq!(e.calls.load(Ordering::Relaxed), calls);
    }

    #[test]
    fn bad_input_at_the_front_does_not_trip_breaker() {
        let e = FlakyEngine { max: 1000, calls: AtomicUsize::new(0) };
        let mut t = texts(256);
        t[0] = "bad".into();
        let refs: Vec<&str> = t.iter().map(|s| s.as_str()).collect();
        let mut breaker = Breaker::for_batch_size(256);
        let r = embed_resilient_with(&e, &refs, &mut breaker);
        assert_eq!(r.failed(), 1);
        assert!(!breaker.tripped());
    }
}
