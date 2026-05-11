# Bugs
**Issue and bug tracking for Cadmus**

id: fb2aixeyeiy2guqnmqjtbpjj
template: bug

Tracks defects and accepted deviations from the target vision. Cards
with `severity: accepted` document conscious deviations — the
"won't fix" bucket per CLAUDE.md.

## Open
id: 4eg4bdjpr1eybhzyhemevgu9

### `TranscribeOptions::threads` not implemented (definition.md §4.2)
id: aymr0v0a44swyl4rlpftqf2b
severity: accepted
priority: low

`definition.md §4.2` lists `threads: Option<u32>` on
`TranscribeOptions` ("per-call thread count override"). Cadmus does
not surface this field — `TranscribeOptions` exposes only `language`
and `beam_size`.

Reason: `ct2rs 0.9.18` has no per-call thread override. Threading
lives on `Config::num_threads_per_replica`, which is set when
`Whisper::new` is called and cannot be changed for the life of the
instance. The only feasible per-call workaround would tear down and
rebuild the `Whisper` instance per call — orders of magnitude more
expensive than the inference itself, plus it would re-load the model
weights from disk on every call.

Accepted deviation. `LoadModelOptions::threads` remains the only
thread knob. Reintroduce when ct2rs grows a per-call equivalent.

### Detected language not surfaced when `TranscribeOptions::language == None`
id: dh0gxqm0uy5vl7zuw5st9ijp
severity: accepted
priority: low

When the caller leaves `TranscribeOptions::language` unset,
`Whisper::generate(samples, None, ...)` runs language detection
internally and uses the detected `<|xx|>` token in the prompt prefix,
but discards it before returning chunks. ct2rs 0.9.18's high-level
`Whisper` returns only the model-generated tokens, not the prompt
prefix — empirically verified with `tiny` on the German fixture:
chunks contain only `<|0.00|> ... <|3.00|>`, no language token.

`PLAN_public_api.md` assumption A5 expected the token to be in the
output stream and added the helper
`inference::detect_language_from_chunks` to parse it. The helper is
correct in isolation (3 unit tests pass) but ct2rs gives it nothing
to find. The helper stays in the code so behavior auto-corrects when
upstream surfaces the detected token.

Result: when `TranscribeOptions::language == None`,
`TranscriptResult.language == ""`. When the caller passes an explicit
`Some("de")`, that value is echoed back verbatim — the round-trip
case (the common one) works.

The fix would require dropping to `ct2rs::sys::Whisper` plus an
in-house mel spectrogram pipeline (~150 LOC of ndarray/rustfft
plumbing) — out of v1 scope. Reactivate when the ct2rs upstream
backlog item lands (see `docs/backlog.kanban.md`: "Surface ct2rs
internally-detected language token").

### `free()` during in-flight `transcribe()` rejects the in-flight Promise with `AlreadyFreed`
id: {new}
severity: high
priority: high

`tests/lifecycle.test.mjs:40` (`free-during-inflight: in-flight
Promise resolves; subsequent transcribe rejects`) fails on Linux
x86_64 with `error: 'model already freed', code: 'AlreadyFreed'`.
The contract documented by the test is: a `transcribe()` Promise
that was created before `free()` must still resolve with valid
segments; only subsequent `transcribe()` calls may reject.

**Mechanism.** The JS path runs decode before the inference handle's
freed check:

```
TranscribeTask::compute()
  → api::CadmusModel::transcribe()
    → transcribe_with_handle()
      → decode_to_pcm16k(audio)            // slow, no freed check
      → InferenceHandle::transcribe_with_options()
          if self.freed.load(...) { return AlreadyFreed }   // too late
```

If JS calls `model.free()` between the start of `decode_to_pcm16k`
and the freed check inside `transcribe_with_options`, the in-flight
task observes `freed=true` and rejects. The Rust-side test
`inference::tests::free_during_inflight_completes_normally` does not
catch this because it skips decode and calls `InferenceHandle::transcribe`
on raw samples — the worker reaches the freed check before
`free()` is called and proceeds with a held `Arc<Whisper>` clone.

The race is platform-independent. macOS just hides it: with fast
cores, decode of the 30 s padded WAV often finishes inside the
test's `sleep(50)`, so `free()` lands after the Promise has already
resolved. On the slower Linux build host (1–2 cores) the window
opens reliably.

**What was attempted.** The `PLAN_linux_x64_build` session
(2026-05-11, since killed) tried to fix this by adding an
`AtomicUsize inflight` counter to `napi::CadmusModel` and a `Drop`
on `TranscribeTask` that defers `inner.free()` until inflight
reaches zero. The patch is preserved at
`docs/archive/PLAN_linux_x64_build.napi_rogue_attempt.diff` for
reference. It was reverted because (a) it was out of plan scope
(Rule 8) and (b) it has its own race: between the freed check in
`transcribe()` and the `inflight.fetch_add(1)`, `free()` can land,
see inflight==0, and call `inner.free()` before the in-flight task
increments the counter.

**Recommended approach for a proper fix.** Either:

1. Move the freed observation into `compute()` so the entire
   in-flight task uses a snapshot taken at task-start (e.g. let
   `transcribe()` clone an `Arc<Whisper>` immediately and pass it
   to the AsyncTask instead of the bare `Arc<api::CadmusModel>`),
   so `free()` cannot interrupt mid-decode.
2. Accept the simpler contract: `free()` aborts in-flight Promises
   with `AlreadyFreed` synchronously, and update
   `tests/lifecycle.test.mjs:40` plus `definition.md §5` accordingly.

Option 1 keeps the documented contract; option 2 simplifies the
implementation. Decision belongs to a separate plan.

## In Progress
id: fpxmyg2qwsy8kuxtv3lzrige

## Done
id: 9bd2g6q54xi7cfr0hhnqzls0

### Test cache race in `cargo test` between `storage` and `inference` test modules
id: {new}
severity: medium
priority: medium

`src/storage::tests::download_tiny_smoke` and
`src/inference::tests::ensure_tiny` both touch the shared cache
directory `target/cadmus-test-cache/tiny`. `cargo test` runs the
tests inside a single binary in parallel by default, so the two
paths can race on the cache: one stages the model files while the
other is part-way through `ensure_present` / `InferenceHandle::new`,
producing partial files and intermittent failures. The race is
visible on slower hosts (Linux build host, 1–2 cores) and latent
on fast macOS.

Fix: test-only `Mutex` guard `test_cache_lock()` in `src/storage.rs`
(`#[cfg(test)]`), acquired by both `download_tiny_smoke` and
`ensure_tiny`. `ensure_tiny` additionally validates the cached
model by attempting `InferenceHandle::new`; on failure it removes
the directory and re-stages from scratch. No production code
touched.

Discovered during PLAN_linux_x64_build verification.

<!-- markdown-kanban
name: bug
description: |
  Tracks defects and accepted deviations from the target vision.
columnsLocked: false
columns:
  - key: open
    title: Open
    description: Confirmed defects or accepted deviations awaiting work or acknowledgement.
  - key: inprogress
    title: In Progress
    description: Being actively worked on.
  - key: done
    title: Done
    description: Resolved or shipped.
cardFields:
  - key: severity
    type: select
    options:
      - low
      - medium
      - high
      - accepted
    description: |
      low / medium / high — defect severity.
      accepted — conscious deviation from target vision; will not be fixed.
  - key: priority
    type: select
    options:
      - none
      - low
      - medium
      - high
    description: |
      none / low / medium / high — relative ordering for "Open" defects.
-->
