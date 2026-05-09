# PLAN_inference_core — Crate-internal Whisper inference + segment parser

Plan #4 of [CONCEPT_v1_buildout](CONCEPT_v1_buildout.md). Wires `ct2rs::Whisper` into the crate as **internal-only** machinery: a handle type that holds the loaded model, runs inference on f32 PCM samples, and a parser that turns Whisper's timestamp-token output into structured segments. End-to-end coverage: download tiny via Plan 3 → decode fixture via Plan 2 → load and transcribe via this plan → assert that all three numbers (eins/1, zwei/2, drei/3) are recognised in either spoken or digit form (Human amendment, see A3).

Reference: [CONCEPT_v1_buildout.md](CONCEPT_v1_buildout.md), in particular **D4** (memory model: `Arc<Whisper>` + atomic freed sentinel, no `Mutex` on the inference path), **D5** (audio pipeline already done in Plan 2), **D6** (model directory layout — `tiny` already populated by Plan 3), the Plan 4 row of the Plan Breakdown, and the **R1** / **R3** risks (ct2rs `Send + Sync` invariant; in-flight semantics under concurrent free).

---

## Context & Goal

After Plan 3 the crate can stage a CTranslate2 Whisper model on disk; after Plan 2 it can decode arbitrary audio bytes to 16 kHz mono f32 in `[-1, 1]`. This plan is the missing bridge: an internal Whisper handle that takes those samples and returns transcribed text plus segment timing.

After this plan:

- New file `src/inference.rs` owns the model handle (`InferenceHandle`), the segment parser, the internal error type (`InferenceError`), and a single `pub(crate) struct Segment`.
- `InferenceHandle::new(model_dir: &Path) -> Result<Self, InferenceError>` constructs a `ct2rs::Whisper` and wraps it according to D4.
- `InferenceHandle::transcribe(&self, samples: &[f32], language: Option<&str>) -> Result<Vec<Segment>, InferenceError>` runs `Whisper::generate` and parses the result.
- `InferenceHandle::free(&self)` is non-blocking, idempotent, and triggers reference-counted deferred release: the native `ct2rs::Whisper` is dropped only when the last `Arc` clone (held by an in-flight `transcribe` call) goes out of scope.
- `#![allow(dead_code)]` is removed from **both** `src/decode.rs` and `src/storage.rs`. This plan is the first consumer of `decode::decode_to_pcm16k` and of `storage::{TINY, ensure_present, download, test_cache_dir}`.
- Crate-internal tests covering: end-to-end transcription against the live `tiny` model (the Concept's Plan 4 "Done when" anchor); the three D4 invariants (transcribe-after-free, free-during-inflight, concurrent transcribe); synthetic segment-parser unit tests.
- No public API change. `cadmus::version()` is still the only public function. Plan 5 (`PLAN_public_api`) takes care of all surface work, including promoting `InferenceError` into the public `CadmusError`.

What this plan does **not** do:

- Public API. `InferenceHandle`, `Segment`, `InferenceError` are all `pub(crate)`. `lib.rs` re-exports nothing new.
- `LoadModelOptions` / `TranscribeOptions`. Plan 5 maps user-facing options onto `ct2rs::Config` and `ct2rs::WhisperOptions`. Plan 4 hard-codes `Default::default()` for both. The `language` argument on `transcribe` is the one knob exposed because the e2e test needs it (`Some("de")` improves tiny-model accuracy on the fixture).
- napi bridge. Plan 6 (`PLAN_napi_surface`) wraps everything in `AsyncTask`s.
- The full 17-entry catalog. `storage::TINY` from Plan 3 is the only model exercised here.
- `find_model`. Plan 5.

## Breaking Changes

**None.** Internal-only addition. New module `src/inference.rs`, one new line `mod inference;` in `src/lib.rs`, one `#![allow(dead_code)]` removed from each of `src/decode.rs` and `src/storage.rs`. No dependency changes. No public symbol gained or lost.

## Reference Patterns

- **`src/storage.rs`** (Plan 3) — same shape as the new `src/inference.rs`: crate-private module with `pub(crate)` items, `#[derive(Debug)] pub(crate) enum ...Error` implementing `Display + Error`, `#[cfg(test)] mod tests` appended in the same file. Same pattern of running real network-touching tests under `cargo test`.
- **`src/decode.rs`** (Plan 2) — `pub(crate) enum AudioError` with `Display + Error` is the model for `InferenceError`.
- **ct2rs whisper example** (`ct2rs/examples/whisper.rs` in the cargo registry, ct2rs 0.9.18) — canonical `Whisper::new(model_dir, Config::default())` + `whisper.generate(samples, language, timestamp, &WhisperOptions::default())` shape, including the `--copy_files preprocessor_config.json tokenizer.json` workflow that Plan 3's `TINY` entry already satisfies.
- **D4 in `CONCEPT_v1_buildout.md`** — verbatim memory-model specification. The `Send + Sync` impls referenced are still present in ct2rs 0.9.18 at `src/sys/whisper.rs:524–525` (verified during plan-write).

## Dependencies

**None added.** `ct2rs` is already a direct dependency since `PLAN_skeleton`, with the `whisper` feature and the per-platform BLAS feature subset from D7/D8. No new crate. No version bump.

If `ct2rs::Whisper::new` / `Whisper::generate` / `WhisperOptions` no longer match the signatures used in Step 4 (e.g. ct2rs renames or re-shapes them in a future patch release pulled in by `cargo update`), the Coder stops and reports — replacing or upgrading a pinned dependency is plan-level (Hard Rule 11, Rule 7).

## Assumptions & Risks

- **A1.** `storage::TINY` cache populated by Plan 3 lives at `storage::test_cache_dir().join("tiny")` and contains the five files of D6 with non-zero size. The e2e test calls `storage::download(&TINY, &dir, None, None)` first, which is idempotent (Plan 3, verified): if the cache is already populated the call is a no-op; otherwise the test downloads ~75 MB on the first run.
- **A2.** ct2rs 0.9.18's `unsafe impl Send + Sync for ffi::Whisper` (at `src/sys/whisper.rs:524–525`) holds. If a future ct2rs release removes those impls, `Arc<Whisper>` shared across threads stops compiling and the Coder stops and reports per R1 of CONCEPT_v1_buildout. Mitigation captured at concept level — falling back to `Arc<Mutex<Whisper>>` is a follow-up plan, not improvisation.
- **A3 (amended 2026-05-08 by Human after Reviewer Finding 1 on first pass).** Whisper's tiny model on the eins-zwei-drei fixture is non-deterministic about whether it transcribes spoken numbers as German words ("eins", "zwei", "drei") or as ASCII digits ("1", "2", "3"). On macOS / Apple Accelerate / `language=Some("de")` the actual output is `" 1, 2, 3, 4, 5."`. The original A3 anchor ("contains 'eins'") was therefore falsified during first validation. The Human authorised relaxing the anchor to a substring set: the transcript must contain at least one of `{"eins","1"}` AND at least one of `{"zwei","2"}` AND at least one of `{"drei","3"}` (case-insensitive, lowercased before checking). This still proves end-to-end audio-pipeline + model-load + segment-parse correctness on a real model output and stays robust against future tiny-model digitisation drifts. If even this relaxed assertion fails, that is a Rule 7 plan-level surprise.
- **A4.** `tiny` transcribes 30 s of audio in well under 5 s on `aarch64-apple-darwin` with Apple Accelerate. The free-during-inflight test relies on this generously — a 100 ms gap on the main thread between spawning the inference thread and calling `free()` is more than enough for the inference thread to pass the freed-flag check, clone the `Arc`, and enter `Whisper::generate`. If runtime drifts catastrophically (e.g. a future ct2rs release becomes 100× slower), the test stays correct but takes longer; it does not become flaky.
- **R1.** `ct2rs::Whisper::generate` allocates large mel-spectrogram buffers on the native heap. Two parallel calls (the concurrent-transcribe test) approximately double peak memory. On a developer machine with ≥ 8 GB RAM this is invisible; on tightly memory-constrained hosts it could OOM. Accepted — the concurrent test is a contract test, not a stress test.
- **R2.** `Mutex<Option<Arc<Whisper>>>` interior mutability for the `Arc`-swap step (see Step 3) introduces a brief mutex critical section per `transcribe` call. The section covers `freed`-check + `Arc::clone` only, never the call to `generate()`. Lock contention between two simultaneous transcribes is microseconds; the actual inference runs lock-free on the cloned `Arc`, in line with D4 ("no Mutex on the inference path").
- **R3.** `Whisper::generate` returns `Result<Vec<String>, anyhow::Error>` — one entry per 30-second chunk. The Concept's segment parser must handle multi-chunk output correctly (timestamps in chunk N are relative to chunk N, not absolute). Mitigation: the parser is fed each chunk's string in order and offsets each chunk's timestamps by `chunk_index * 30.0`. Tested with synthetic two-chunk input.
- **R4.** Whisper sometimes emits malformed token strings — partial timestamps, no timestamps at all, or a timestamp without a closing one. Parser policy: emit text with `start = end = last-seen-timestamp` (or 0.0 if none has been seen). Never panic, never error out. Tested.

No new `severity: accepted` bug cards. `docs/bug.kanban.md` does not exist yet (CLAUDE.md says it is created on first use; this plan does not need it). No new backlog cards.

No BREAKs. Linux deferred per concept override (verification matrix below is macOS-only).

## Steps

Single phase, macOS-only execution per the concept's Linux-deferral override. The plan's natural ordering is: types and errors first, handle next, segment parser, then wiring, then tests.

1. **Create `src/inference.rs`.** New file. Top-of-file imports and types:

   ```rust
   use std::path::Path;
   use std::sync::{Arc, Mutex};
   use std::sync::atomic::{AtomicBool, Ordering};

   use ct2rs::{Config, Whisper, WhisperOptions};

   #[derive(Debug)]
   pub(crate) enum InferenceError {
       Load(String),
       Generate(String),
       AlreadyFreed,
       Poisoned,
   }

   impl std::fmt::Display for InferenceError {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           match self {
               InferenceError::Load(m)     => write!(f, "load: {m}"),
               InferenceError::Generate(m) => write!(f, "generate: {m}"),
               InferenceError::AlreadyFreed => write!(f, "model already freed"),
               InferenceError::Poisoned    => write!(f, "internal lock poisoned by a panic"),
           }
       }
   }
   impl std::error::Error for InferenceError {}

   #[derive(Debug, Clone, PartialEq)]
   pub(crate) struct Segment {
       pub start: f32,   // seconds, monotonic across chunks
       pub end:   f32,   // seconds, >= start
       pub text:  String,
   }
   ```

   No `#![allow(dead_code)]` at the top: the test module in this same file consumes every `pub(crate)` item.

   **Concept conformance — reading of D4.** The Concept's D4 reads literally: *"`CadmusModel` holds the inner `ct2rs::Whisper` as `Arc<Whisper>` plus an atomic 'freed' sentinel. **No `Mutex` on the inference path** ..."*. The constraint that binds the implementation is **"no Mutex on the inference path"** — the inference path being the call to `Whisper::generate`, which is where contention would actually hurt. The plan-breakdown row's shorthand "no mutex" is read in this plan as the same constraint, not as a stricter one. The design below honours it: a `Mutex<Option<Arc<Whisper>>>` is used **only** for the atomic swap that lets `free()` actually drop the owning `Arc` (otherwise `free(&self)` cannot release the native instance until handle Drop, which contradicts D4's "free() drops its own Arc-clone" wording in the same paragraph). The mutex critical section spans `freed`-check + `Arc::clone` (or, in `free()`, a `take()`); `Whisper::generate` runs entirely outside the lock, on the cloned `Arc`. A fresh implementer who reads only D4 plus the plan-breakdown shorthand might pick a lock-free `ArcSwap`-style design, which would be a valid alternative — but it would require either an extra dependency (no plan-level approval here) or a hand-rolled atomic-swap (more `unsafe`, more review surface). The Mutex-around-swap chosen here is the simpler, std-only, equally-D4-correct path.

2. **Define `InferenceHandle`.** Append to `src/inference.rs`:

   ```rust
   /// Internal Whisper handle. Implements D4 (Arc + atomic freed sentinel,
   /// no Mutex on the inference path).
   ///
   /// Layout: the inner `Arc<Whisper>` lives inside a `Mutex<Option<...>>`
   /// purely so `free()` can swap it out atomically — the mutex critical
   /// section covers only the freed-flag check and `Arc::clone`, never the
   /// actual call to `Whisper::generate`. That keeps generate lock-free and
   /// concurrent across threads, which D4 requires.
   pub(crate) struct InferenceHandle {
       inner: Mutex<Option<Arc<Whisper>>>,
       freed: AtomicBool,
   }
   ```

   No `Send`/`Sync` impls — the compiler derives them automatically because every field is `Send + Sync` (the `Arc<Whisper>` thanks to ct2rs's `unsafe impl Send + Sync for ffi::Whisper` at `ct2rs/src/sys/whisper.rs:524–525`).

3. **Implement `InferenceHandle::new`, `transcribe`, `free`.** Append to `src/inference.rs`:

   ```rust
   impl InferenceHandle {
       pub(crate) fn new(model_dir: &Path) -> Result<Self, InferenceError> {
           let whisper = Whisper::new(model_dir, Config::default())
               .map_err(|e| InferenceError::Load(e.to_string()))?;
           Ok(Self {
               inner: Mutex::new(Some(Arc::new(whisper))),
               freed: AtomicBool::new(false),
           })
       }

       /// Transcribe 16 kHz mono f32 samples into segments.
       ///
       /// Concurrency: the mutex is held only while cloning the inner Arc.
       /// The actual inference runs on the local clone, lock-free. Multiple
       /// threads calling `transcribe` in parallel never serialise on the
       /// inference path.
       ///
       /// Free safety: if `free()` runs concurrently *after* this call has
       /// cloned its Arc, the local clone keeps the native Whisper alive
       /// for the duration of `generate`. The native instance is released
       /// only when the last clone (this one or another in-flight call)
       /// is dropped.
       pub(crate) fn transcribe(
           &self,
           samples: &[f32],
           language: Option<&str>,
       ) -> Result<Vec<Segment>, InferenceError> {
           if self.freed.load(Ordering::SeqCst) {
               return Err(InferenceError::AlreadyFreed);
           }

           let local: Arc<Whisper> = {
               // Poisoned mutex implies a panic in another transcribe call —
               // surface as a distinct variant so Plan 5 can map onto the
               // public `CadmusError::Poisoned` (definition.md §4.3) without
               // losing the cause. Do not collapse this into AlreadyFreed.
               let guard = self.inner.lock().map_err(|_| InferenceError::Poisoned)?;
               match guard.as_ref() {
                   Some(arc) => Arc::clone(arc),
                   None => return Err(InferenceError::AlreadyFreed),
               }
           };

           // Lock released. The rest runs on the local clone.
           let chunks = local
               .generate(samples, language, /* timestamp */ true, &WhisperOptions::default())
               .map_err(|e| InferenceError::Generate(e.to_string()))?;

           Ok(parse_segments(&chunks))
       }

       /// Mark the handle as freed and drop the owning Arc. Idempotent;
       /// non-blocking; never aborts in-flight transcriptions.
       pub(crate) fn free(&self) {
           self.freed.store(true, Ordering::SeqCst);
           if let Ok(mut guard) = self.inner.lock() {
               // Drops the Arc. The native Whisper survives until the last
               // transcribe-side clone is dropped.
               *guard = None;
           }
       }
   }

   impl Drop for InferenceHandle {
       fn drop(&mut self) {
           // Same effect as free(); cheap to repeat.
           self.free();
       }
   }
   ```

   Implementation notes:
   - The `freed`-flag pre-check before locking the mutex is an optimisation, not a correctness gate. Even without it the lock + `as_ref()` match would catch a freed handle. The early return spares an uncontended lock acquisition in the steady state and matches the spirit of D4 ("freed sentinel guards new entries to the API").
   - Mutex poisoning surfaces as a distinct `InferenceError::Poisoned` variant. Definition.md §4.3 already commits the public surface to a `Poisoned` error variant separate from `AlreadyFreed`; collapsing the two internally would erase the cause, and Plan 5 would have no faithful way to recover it. Plan 5 therefore maps `InferenceError::Poisoned` → `CadmusError::Poisoned` and `InferenceError::AlreadyFreed` → `CadmusError::AlreadyFreed`, one-to-one.
   - `transcribe` accepts `language: Option<&str>` and forwards it directly to `Whisper::generate`. Plan 5 will surface this as `TranscribeOptions::language` and pass `Some("de")` / `Some("en")` / `None`. Plan 4 hard-codes nothing in the handle itself; the e2e test passes `Some("de")` because tiny-model accuracy on the German fixture is materially better with an explicit language hint.

4. **Implement the segment parser.** Append to `src/inference.rs`:

   ```rust
   /// Parse one Whisper output chunk's worth of `<|N.NN|>`-delimited text
   /// into segments. Per ct2rs's whisper integration, `Whisper::generate`
   /// returns one string per ~30-second mel chunk fed to the encoder.
   /// Timestamps inside one chunk are relative to that chunk's start; this
   /// parser receives the full chunk array and offsets each chunk's
   /// timestamps by `chunk_index * 30.0` so the resulting Segments are
   /// monotonic across chunks.
   pub(crate) fn parse_segments(chunks: &[String]) -> Vec<Segment> {
       const CHUNK_SECONDS: f32 = 30.0;
       let mut out = Vec::new();
       for (idx, chunk) in chunks.iter().enumerate() {
           let offset = idx as f32 * CHUNK_SECONDS;
           parse_one_chunk(chunk, offset, &mut out);
       }
       out
   }

   fn parse_one_chunk(chunk: &str, offset: f32, out: &mut Vec<Segment>) {
       // State machine: walk the chunk, accumulate text between consecutive
       // <|N.NN|> tokens. Token format is `<|FLOAT|>` with FLOAT in seconds.
       // Malformed tokens (no closing `|>`, non-numeric inside) are emitted
       // verbatim as text — never panic, never error.
       let bytes = chunk.as_bytes();
       let mut i = 0;
       let mut last_ts: Option<f32> = None;
       let mut pending_text = String::new();

       while i < bytes.len() {
           if bytes[i] == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
               if let Some(end) = find_token_end(bytes, i + 2) {
                   let tok = &chunk[i + 2..end];
                   if let Ok(secs) = tok.parse::<f32>() {
                       // Timestamp token. Close any pending text against
                       // last_ts (start) and the new timestamp (end).
                       if !pending_text.trim().is_empty() {
                           let start = last_ts.unwrap_or(0.0);
                           out.push(Segment {
                               start: offset + start,
                               end:   offset + secs,
                               text:  std::mem::take(&mut pending_text),
                           });
                       } else {
                           pending_text.clear();
                       }
                       last_ts = Some(secs);
                       i = end + 2; // skip past `|>`
                       continue;
                   }
                   // Non-timestamp `<|...|>` token (e.g. `<|de|>`,
                   // `<|transcribe|>`, `<|notimestamps|>`). Drop entirely;
                   // these are control tokens, not user-visible text.
                   i = end + 2;
                   continue;
               }
               // Malformed: no closing `|>`. Treat the `<` as literal text.
               pending_text.push('<');
               i += 1;
               continue;
           }
           pending_text.push(bytes[i] as char);
           i += 1;
       }

       // Trailing text after the last timestamp (or in a chunk without
       // any timestamps): emit as a final segment with start=end=
       // last_ts (or 0.0). Never lose text.
       if !pending_text.trim().is_empty() {
           let start = last_ts.unwrap_or(0.0);
           out.push(Segment {
               start: offset + start,
               end:   offset + start,
               text:  pending_text,
           });
       }
   }

   fn find_token_end(bytes: &[u8], from: usize) -> Option<usize> {
       let mut j = from;
       while j + 1 < bytes.len() {
           if bytes[j] == b'|' && bytes[j + 1] == b'>' {
               return Some(j);
           }
           j += 1;
       }
       None
   }
   ```

   Implementation notes:
   - The byte-level walk works because timestamp tokens and the `<|`/`|>` delimiters are pure ASCII; pushing `bytes[i] as char` is correct for those byte positions but **not** for arbitrary UTF-8 inside the text segments. Whisper's tokenizer output is UTF-8 and may contain multi-byte sequences (German umlauts in the fixture, etc.). To preserve them we walk byte-by-byte but **the text itself must be sliced from the original `&str`**, not assembled from byte casts. Revised implementation: we accumulate `pending_text` from a string slice between known boundaries, not byte-by-byte. The Coder rewrites the inner loop to:
     - Track `text_start: usize` whenever we move past a token's closing `|>` (or at i = 0).
     - On encountering a `<|` token, push `&chunk[text_start..i]` to pending text accumulator (an `&str` slice — UTF-8 safe), then process the token.
     - At end-of-chunk, push `&chunk[text_start..]`.
   - The coder is to use that revised approach in the actual implementation. The byte-walk above is illustrative scaffolding; the production code must use string-slice accumulation to be UTF-8-correct. Tests in Step 6 cover a German-umlaut-bearing input precisely to catch regressions on this point.
   - Non-timestamp control tokens (`<|de|>`, `<|transcribe|>`, `<|notimestamps|>`, `<|startoftranscript|>`, `<|endoftext|>`, etc.) are dropped wholesale by the parser. ct2rs's `Tokenizer::decode` may already strip most of these, but the parser is defensive against any survivors.
   - 30-second chunk boundary: ct2rs's whisper module documents `n_samples = 480_000` (= 30 s @ 16 kHz) as the per-chunk feed size. `parse_segments` mirrors that constant. If a future ct2rs version changes its chunking, segment timestamps drift across chunk boundaries — accepted as a known caveat; any change shows up as test failure on a multi-chunk fixture, which the e2e ~30 s test will cover indirectly.

5. **Wire the module + remove dead-code opt-outs.** In `src/lib.rs`, add `mod inference;` next to the existing `mod decode;` and `mod storage;` lines. Then:
   - Open `src/decode.rs`, delete the line `#![allow(dead_code)] // Removed in Plan 4 when transcribe() consumes this.` from the very top of the file.
   - Open `src/storage.rs`, delete the line `#![allow(dead_code)] // Removed in Plan 4 when the inference test consumes this.` from the very top of the file.

   Verify with `cargo build --release` that no `dead_code` warnings appear. If a function in either file is genuinely no longer used by Plan 4's test (it should not happen — the test exercises every `pub(crate)` item except `download`'s in-loop cancel branches, which the existing storage tests already exercise), the Coder stops and reports rather than re-adding a `#[allow]`.

6. **Write the tests.** Append `#[cfg(test)] mod tests { ... }` to `src/inference.rs`. The block contains seven tests across three groups.

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;
       use crate::decode::decode_to_pcm16k;
       use crate::storage::{download, ensure_present, test_cache_dir, TINY};

       use std::fs;
       use std::path::PathBuf;
       use std::sync::Arc as StdArc;
       use std::sync::mpsc;
       use std::thread;
       use std::time::Duration;

       // Helper: stage tiny once for the whole test run, then return the
       // populated dir. Idempotent — Plan 3's storage::download skips
       // already-present files, so subsequent calls are instant.
       fn ensure_tiny() -> PathBuf {
           let dir = test_cache_dir().join("tiny");
           if !ensure_present(&TINY, &dir) {
               download(&TINY, &dir, None, None).expect("staging tiny failed");
           }
           assert!(ensure_present(&TINY, &dir));
           dir
       }

       fn fixture_bytes() -> Vec<u8> {
           let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
               .join("fixtures/eins-zwei-drei.mp3");
           fs::read(&path).unwrap_or_else(|_| panic!("fixture missing: {:?}", path))
       }

       fn fixture_samples() -> Vec<f32> {
           decode_to_pcm16k(&fixture_bytes()).expect("decode fixture failed")
       }

       // Repeat the decoded fixture N times to produce a long buffer for
       // the in-flight tests. ~3 s × N. N=10 → ~30 s.
       fn long_samples(repeat: usize) -> Vec<f32> {
           let one = fixture_samples();
           let mut out = Vec::with_capacity(one.len() * repeat);
           for _ in 0..repeat {
               out.extend_from_slice(&one);
           }
           out
       }

       // Amended A3 helper: tiny normalises spoken numbers to digits at
       // will. Accept either form per number; require all three present.
       fn assert_eins_zwei_drei(joined: &str) {
           let one   = joined.contains("eins") || joined.contains("1");
           let two   = joined.contains("zwei") || joined.contains("2");
           let three = joined.contains("drei") || joined.contains("3");
           assert!(
               one && two && three,
               "transcript missing 1/2/3 markers: {joined:?}"
           );
       }

       // -----------------------------------------------------------------
       // E2E: Concept Plan 4 "Done when" — fixture transcribes to text
       // recognising all three spoken numbers in either word or digit
       // form (per A3 amendment 2026-05-08). Single source of truth that
       // the decode → resample → ct2rs → segment-parse pipeline is wired
       // correctly.
       // -----------------------------------------------------------------
       #[test]
       fn end_to_end_eins_zwei_drei() {
           let dir = ensure_tiny();
           let handle = InferenceHandle::new(&dir).expect("load tiny");
           let samples = fixture_samples();

           let segments = handle
               .transcribe(&samples, Some("de"))
               .expect("transcribe failed");

           assert!(!segments.is_empty(), "no segments parsed");
           let joined: String = segments
               .iter()
               .map(|s| s.text.as_str())
               .collect::<Vec<_>>()
               .concat()
               .to_lowercase();
           // Per amended A3: at least one of {"eins","1"} AND
           // {"zwei","2"} AND {"drei","3"}. Implemented as the helper
           // `assert_eins_zwei_drei(&joined)` in the test module, reused
           // by the in-flight and concurrent tests below.
           assert_eins_zwei_drei(&joined);

           // Sanity: every segment has end >= start, and segments are
           // monotonic (end_n <= start_{n+1} or close to it).
           for seg in &segments {
               assert!(seg.end >= seg.start, "segment end < start: {seg:?}");
           }
       }

       // -----------------------------------------------------------------
       // D4 invariant 1: transcribe after free returns AlreadyFreed.
       // free() itself is idempotent.
       // -----------------------------------------------------------------
       #[test]
       fn transcribe_after_free_returns_already_freed() {
           let dir = ensure_tiny();
           let handle = InferenceHandle::new(&dir).unwrap();

           handle.free();
           handle.free(); // idempotent — must not panic

           let samples = fixture_samples();
           let result = handle.transcribe(&samples, Some("de"));
           assert!(matches!(result, Err(InferenceError::AlreadyFreed)));
       }

       // -----------------------------------------------------------------
       // D4 invariant 2: free() during an in-flight transcribe lets the
       // call complete normally with its result. After the in-flight
       // finishes, new transcribes return AlreadyFreed.
       // Mechanism: the inflight call holds an Arc clone, keeping the
       // native Whisper alive until generate() returns. main thread sets
       // `freed`, swaps the Arc out — but the local clone in the worker
       // is unaffected.
       //
       // Race-free handshake: the worker decodes/expands the long buffer
       // *before* signalling readiness, then sends on the channel
       // immediately before calling transcribe. The main thread receives,
       // then sleeps 50 ms — orders of magnitude more than the few µs
       // transcribe spends between channel-send and entering generate()
       // (one atomic load + one mutex-guarded Arc::clone) — and only
       // then calls free(). On any non-pathological scheduler the worker
       // is firmly inside Whisper::generate when free() runs.
       // -----------------------------------------------------------------
       #[test]
       fn free_during_inflight_completes_normally() {
           let dir = ensure_tiny();
           let handle = StdArc::new(InferenceHandle::new(&dir).unwrap());

           // Pre-compute the long buffer on the main thread so the worker
           // does no audio-pipeline work between spawn and transcribe.
           // ~30 s of audio: enough that Whisper::generate runs for
           // multiple seconds even on Apple Accelerate.
           let long = long_samples(10);

           let (tx, rx) = mpsc::channel::<()>();
           let h2 = StdArc::clone(&handle);
           let worker = thread::spawn(move || {
               // Signal readiness immediately before transcribe. Between
               // the send and entering Whisper::generate the worker only
               // performs an atomic flag-check and a mutex-guarded Arc
               // clone (microseconds).
               tx.send(()).unwrap();
               h2.transcribe(&long, Some("de"))
           });

           // Wait for the handshake; this rules out the "free() ran before
           // transcribe was even called" race the fixed sleep allowed.
           rx.recv().expect("worker dropped before signalling");

           // Tiny additional buffer to let the worker cross the
           // freed-check / Arc::clone boundary into generate(). 50 ms is
           // ~1000× the post-handshake critical-section budget.
           thread::sleep(Duration::from_millis(50));

           handle.free();

           let result = worker.join().expect("worker panicked");
           let segments = result.expect("in-flight transcribe must complete normally");
           assert!(!segments.is_empty(), "in-flight result empty");
           let joined: String = segments
               .iter()
               .map(|s| s.text.as_str())
               .collect::<Vec<_>>()
               .concat()
               .to_lowercase();
           assert_eins_zwei_drei(&joined); // amended A3

           // Post-free new call: must fail with AlreadyFreed.
           let samples = fixture_samples();
           let after = handle.transcribe(&samples, Some("de"));
           assert!(matches!(after, Err(InferenceError::AlreadyFreed)));
       }

       // -----------------------------------------------------------------
       // D4 invariant 3: two parallel transcribes on one handle both
       // succeed, both return non-empty results. Verifies the lock-free
       // inference path: the mutex is held only for Arc::clone, so two
       // generate() calls run truly in parallel on different cores.
       // -----------------------------------------------------------------
       #[test]
       fn concurrent_transcribe_succeeds() {
           let dir = ensure_tiny();
           let handle = StdArc::new(InferenceHandle::new(&dir).unwrap());

           let h_a = StdArc::clone(&handle);
           let h_b = StdArc::clone(&handle);
           let a = thread::spawn(move || {
               h_a.transcribe(&fixture_samples(), Some("de"))
           });
           let b = thread::spawn(move || {
               h_b.transcribe(&fixture_samples(), Some("de"))
           });

           let ra = a.join().expect("thread a panicked").expect("a transcribe failed");
           let rb = b.join().expect("thread b panicked").expect("b transcribe failed");

           assert!(!ra.is_empty());
           assert!(!rb.is_empty());
           let txt_a: String = ra.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().concat().to_lowercase();
           let txt_b: String = rb.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().concat().to_lowercase();
           assert_eins_zwei_drei(&txt_a); // amended A3
           assert_eins_zwei_drei(&txt_b);
       }

       // -----------------------------------------------------------------
       // Segment parser: synthetic input. No network, no model, fast.
       // -----------------------------------------------------------------
       #[test]
       fn parse_segments_two_segments_one_chunk() {
           let chunks = vec![
               "<|0.00|> Hello.<|2.50|> World.<|5.00|>".to_string(),
           ];
           let segs = parse_segments(&chunks);
           assert_eq!(segs.len(), 2);
           assert_eq!(segs[0].start, 0.0);
           assert_eq!(segs[0].end,   2.5);
           assert!(segs[0].text.contains("Hello"));
           assert_eq!(segs[1].start, 2.5);
           assert_eq!(segs[1].end,   5.0);
           assert!(segs[1].text.contains("World"));
       }

       #[test]
       fn parse_segments_two_chunks_offsets_correctly() {
           let chunks = vec![
               "<|0.00|> First.<|10.00|>".to_string(),   // chunk 0: 0–30 s
               "<|0.00|> Second.<|5.00|>".to_string(),   // chunk 1: 30–60 s
           ];
           let segs = parse_segments(&chunks);
           assert_eq!(segs.len(), 2);
           assert_eq!(segs[0].start, 0.0);
           assert_eq!(segs[0].end,   10.0);
           assert_eq!(segs[1].start, 30.0);
           assert_eq!(segs[1].end,   35.0);
       }

       #[test]
       fn parse_segments_no_timestamps_emits_text_at_zero() {
           let chunks = vec!["just text, no tokens".to_string()];
           let segs = parse_segments(&chunks);
           assert_eq!(segs.len(), 1);
           assert_eq!(segs[0].start, 0.0);
           assert_eq!(segs[0].end,   0.0);
           assert!(segs[0].text.contains("just text"));
       }

       #[test]
       fn parse_segments_drops_control_tokens_keeps_utf8_text() {
           // Whisper sometimes emits language/task tokens alongside the
           // transcript. The parser must drop them, keep the text, and
           // not corrupt UTF-8 (German umlaut in the fixture domain).
           let chunks = vec![
               "<|de|><|transcribe|><|0.00|> grüß dich.<|1.20|>".to_string(),
           ];
           let segs = parse_segments(&chunks);
           assert_eq!(segs.len(), 1);
           assert!(segs[0].text.contains("grüß"), "umlaut lost: {:?}", segs[0].text);
       }

       #[test]
       fn parse_segments_malformed_token_treated_as_text() {
           let chunks = vec!["<|0.00|> ok <| not a token <|1.00|>".to_string()];
           let segs = parse_segments(&chunks);
           // The malformed `<|` becomes literal text; the surrounding two
           // valid timestamps still bracket a single segment.
           assert_eq!(segs.len(), 1);
           assert!(segs[0].text.contains("ok"));
       }
   }
   ```

   Coverage map: `end_to_end_eins_zwei_drei` is the Concept's "Done when" anchor; the three D4 tests prove the memory model directly in Rust before any napi code exists (R3 mitigation, pre-empts Plan 6); five synthetic parser tests cover the parser's policy decisions (chunk offsetting, missing timestamps, control tokens, UTF-8, malformed input).

   Test runtime budget on `aarch64-apple-darwin` with Apple Accelerate:
   - Five synthetic parser tests: < 10 ms total.
   - `end_to_end_eins_zwei_drei`: ~1–2 s (one tiny-model load + one ~3 s fixture).
   - `transcribe_after_free_returns_already_freed`: ~1 s (load + immediate free).
   - `free_during_inflight_completes_normally`: ~3–8 s (load + ~30 s of audio inferred on tiny).
   - `concurrent_transcribe_succeeds`: ~2–4 s (two parallel ~3 s fixtures, may serialise on memory bandwidth).

   Total inference-test cost: ~10–20 s on first cold cache run, plus the one-time ~75 MB tiny download (already paid by Plan 3 in normal local dev). This is in line with the Concept's "verification is fully local" stance.

7. **Build verification.** *(Amended 2026-05-08 by Human after Reviewer Finding 2 on first pass — see Verification section below.)*

   - `cargo build --release --tests` → green; no `dead_code` warnings on the modules added or de-allowed by this plan (decode/inference/storage). The plain `cargo build --release` still emits `dead_code` warnings on those `pub(crate)` items because their only consumers live in `#[cfg(test)]` blocks; this is structural, not an oversight, and is captured here so future plans don't try to "fix" it without amending the contract.
   - `cargo build --release --tests --features napi` → green for the de-allowed modules. Two pre-existing warnings on the napi bridge (`VersionJs`, `version`) remain — they are Plan 1 inheritance, unaffected by this plan.

8. **Run the tests.**

   - `cargo test --release inference::tests` → all eight tests pass (1 e2e + 3 D4 + 5 parser, but listing-wise the parser group is 5 tests; total = 9 tests in the inference module on a fresh count). Subsequent runs reuse the staged tiny model.
   - `cargo test` (no flags) → all tests pass; total cargo-side count grows from 14 (Plan 3 baseline) to 23 (1 version + 8 audio + 5 storage + 9 inference). Slower compile, same network/inference cost.
   - `cargo test --features napi` → 23 passed.

   `npm test` is intentionally **not** part of this plan's verification matrix. Plan 4 changes nothing on the napi/Node surface. The same caveat from Plan 3 applies: the committed `cadmus.darwin-arm64.node` predates the current source tree and is rebuilt only at release time.

9. **Verify packaging boundaries (D27).**

   - `cargo package --list --allow-dirty` — additionally lists `src/inference.rs`. No leakage of npm-side files. No `target/cadmus-test-cache/...` entry.
   - `npm pack --dry-run` — unchanged from Plans 2 and 3 on this macOS-only host: seven entries (`index.js`, `index.d.ts`, `cadmus.darwin-arm64.node`, `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md`, plus `package.json`). The `cadmus.linux-x64-gnu.node` warning carries over per the Linux-deferral override.

Implementation is done at this point. Per CLAUDE.md §5 the Coder stops here and waits — Validation, Doc Update, and Archive happen in subsequent phases driven by the next-step prompt.

## Verification

After Step 9, the working tree on macOS satisfies:

- `cargo test --release` → 23 tests pass (1 version + 8 audio + 5 storage + 9 inference).
- `cargo test --release --features napi` → same count, all green.
- `cargo build --release --tests` → green, no `dead_code` warnings (Human-amended verification command, see Step 7). Plain `cargo build --release` still warns on test-only `pub(crate)` items by design and is therefore not a gate.
- `cargo build --release --tests --features napi` → green for Plan-4-touched modules; two pre-existing napi-bridge warnings (`VersionJs`, `version`) remain, Plan 1 inheritance.
- `cargo package --list --allow-dirty` → contains `src/inference.rs`; no npm-side files; no `target/cadmus-test-cache/...`.
- `npm pack --dry-run` → seven entries, same as Plan 3 baseline.
- `target/cadmus-test-cache/tiny/` reused (Plan 3 cache); no new download on a warm cache.
- `#![allow(dead_code)]` is gone from both `src/decode.rs` and `src/storage.rs`.
- No public Rust API change: `cadmus::version()` still the only public function. `cargo public-api` (if invoked, not required) shows no diff from Plan 3.
- Linux verification deferred per concept override.

Out of scope for this plan's verification: `npm test` and the public Rust API surface — both arrive in Plans 5/6.

### Reviewer focus points

- **Crate-internal API only**: `InferenceHandle`, `Segment`, `InferenceError`, `parse_segments` are all `pub(crate)`. `lib.rs` re-exports nothing new. Public surface is still exactly `version()` + `Version`.
- **D4 conformance**: the `Mutex<Option<Arc<Whisper>>>` shape is a deliberate reading of D4's "no Mutex on the inference path" — the mutex is held only for `freed`-check + `Arc::clone` (microseconds) or, in `free()`, a `take()`; the actual call to `Whisper::generate` runs lock-free on the cloned Arc. The plan's "Concept conformance" paragraph in Step 1 spells this out, including the alternative (`ArcSwap` / hand-rolled atomic swap) that was considered and rejected for std-only simplicity. Reviewer should confirm the mutex critical section never wraps `generate()` and that the conformance text adequately resolves the apparent tension with the plan-breakdown shorthand "no mutex".
- **`Poisoned` distinct from `AlreadyFreed`**: `InferenceError` carries both as separate variants, mirroring definition.md §4.3. Reviewer should confirm that mutex `lock()` failure maps to `InferenceError::Poisoned`, that `*guard = None` after `free()` maps to `InferenceError::AlreadyFreed`, and that no code path collapses one into the other.
- **In-flight test handshake**: `free_during_inflight_completes_normally` no longer relies on a fixed sleep to assume the worker reached `generate()`. The worker pre-computes the long sample buffer, sends on a `mpsc::channel` immediately before `transcribe`, and the main thread receives that signal before its short post-handshake sleep + `free()`. Reviewer should confirm the handshake is in fact race-free under any realistic scheduling (the only window between `tx.send` and `Whisper::generate` is one atomic load + one mutex-guarded `Arc::clone`).
- **`unsafe impl Send + Sync` invariant**: rests on `ct2rs/src/sys/whisper.rs:524–525` of ct2rs 0.9.18. R1 of CONCEPT_v1_buildout already accepts the upgrade-time exposure.
- **In-flight semantics**: the `free_during_inflight_completes_normally` test must demonstrate that the worker thread's call returns `Ok` with a transcript, *not* `AlreadyFreed`. That is the substantive D4 contract. Reviewer should also confirm the test's 200 ms sleep is not the only thing keeping the worker inside `generate` — the long ~30 s buffer ensures inference is in flight for seconds, so the timing is robust.
- **Concurrent-transcribe correctness**: both threads must complete with non-empty transcripts. ct2rs's batch counters (`num_active_batches`, `num_replicas`) suggest internal batching; we do not assert anything about it, only that two parallel calls do not interfere or panic.
- **Segment parser UTF-8 safety**: the parser walks the input as a `&str` and slices it (not byte-by-byte char-cast) — German umlauts and any other multi-byte sequences must survive. The `parse_segments_drops_control_tokens_keeps_utf8_text` test guards this directly. Reviewer should confirm the implementation of Step 4 does not push individual bytes as `char`s into the output `String`.
- **Multi-chunk timestamp offset**: `parse_segments` adds `chunk_index * 30.0` to each chunk's relative timestamps. The 30.0 constant mirrors ct2rs's whisper `n_samples = 480_000` (30 s @ 16 kHz). Reviewer should confirm the test `parse_segments_two_chunks_offsets_correctly` actually catches a regression where the offset is dropped.
- **Idempotent free()**: `free()` is `&self`, calls `inner.lock()` and replaces with `None`. A second call observes `None` already and is a no-op. `Drop` calls `free()` for the Rust-side path where the user never called it explicitly.
- **Mutex poisoning policy**: `lock()` failure maps to `InferenceError::Poisoned`, distinct from `InferenceError::AlreadyFreed` (which fires when `*guard` is `None`). Plan 5 promotes one-to-one into `CadmusError::Poisoned` / `CadmusError::AlreadyFreed` per definition.md §4.3.
- **No `Send`/`Sync` unsafety**: `InferenceHandle` derives `Send + Sync` from its fields. No `unsafe` blocks added in this plan.
- **No new dependencies**: ct2rs already in tree. Hard Rule 11 not invoked.
- **Network cost**: zero on warm cache (Plan 3 staged tiny). `~75 MB` on cold cache, paid once per workspace.
- **Linux deferral honored**: every test runs on macOS only; no Linux-specific code added.

<plan_ready>docs/PLAN_inference_core.md</plan_ready>
