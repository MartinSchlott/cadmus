# PLAN_napi_surface

## Context & Goal

Plans 1–5 built the full Rust public API (`Cadmus`, `CadmusModel`, `ModelRef`, `list_available_models`, `find_model`, `download_model`, `load_model`, the free one-shot `transcribe`, `version`, the 17-entry catalog, and all D4 lifecycle guarantees). Today the napi feature exposes only `version()`; `index.ts` likewise re-exports only `version`. There are no other JS-callable entry points.

This plan finishes Plan 6 of [CONCEPT_v1_buildout.md](CONCEPT_v1_buildout.md): a complete napi-rs surface for the existing Rust API plus a TypeScript wrapper at the repo root and a `node --test` suite that exercises version, catalog, find, load+transcribe, free-after-free, free-during-inflight, and concurrent transcribe. The macOS prebuilt `.node` is committed; the Linux `.node` is **deferred** to PLAN_linux_followup per the override in CONCEPT_v1_buildout.md §"Plan Breakdown".

After this plan the v1 surface is feature-complete on macOS. Linux pickup is one separate plan that runs the same verifications on a Linux host and commits `cadmus.linux-x64-gnu.node`.

## Breaking Changes

**No.** The Rust public API stays as published in 0.6.0. The napi surface and the TS surface today both expose only `version()`; this plan **replaces** that minimal surface with the full one. Any external consumer that has somehow wired against the npm package's `version()` between 0.6.0 and now keeps working — `version()` stays exported with the same shape.

The Rust integration test `tests/public_api.rs` is currently `#![cfg(not(feature = "napi"))]`-gated; it stays gated and unchanged. `cargo test` (default features) keeps running it; `cargo build --release --features napi` still excludes it.

## Reference Patterns

- Existing napi sketch: [src/lib.rs](../src/lib.rs) lines 36–60 (`mod napi_bridge`) — the established camelCase-pinning pattern (`#[napi(js_name = "ct2rs")]`) and the `version()` shape mapping. Extend the same `napi_bridge` module rather than introducing a parallel one; the production layout is one bridge module per crate.
- ct2rs build interaction: [build.rs](../build.rs) — `napi_build::setup()` already runs when `CARGO_FEATURE_NAPI` is set. No build-script change needed.
- Existing `index.ts` platform dispatch: [index.ts](../index.ts) — the platform-detection branch already lists `darwin-arm64` and `linux-x64-gnu` and throws on anything else. Reuse the dispatch verbatim; only the surface re-exported from the binding grows.
- Sibling project: the Human's neighbouring `endymion` crate uses the same root-`package.json` + committed `.node` pattern. If a question arises during implementation about the napi-rs build script invocation or `package.json` `napi` block shape, that project is the reference for what works locally.
- Rust D4 tests: [src/inference.rs](../src/inference.rs) `tests` module — the `free_during_inflight_completes_normally` and `concurrent_transcribe_succeeds` patterns. The JS-side tests in this plan are the AsyncTask-boundary equivalents; the Rust tests are the source of truth for the invariants themselves.

## Dependencies

No new Rust crate dependencies. `napi` (3.8.6) and `napi-derive` (3.5.5) are already declared `optional = true` and gated by the `napi` feature. `napi-build` is already a build-dep.

No new npm runtime dependencies (the package promises zero npm runtime deps in [definition.md §3](definition.md)). `@napi-rs/cli` and `typescript` are already devDependencies.

## Assumptions & Risks

**A1. AsyncTask granularity.** Each "real work" operation (`downloadModel`, `loadModel`, `transcribe`, both the method form and the free one-shot) is a `napi::bindgen_prelude::AsyncTask` — `compute()` runs on a libuv worker, `resolve()` packages the result for JS. Synchronous-on-the-JS-side operations (`new Cadmus`, `listAvailableModels`, `findModel`, `version`, `model.free`) call directly into the Rust core from the napi entry point. Rationale: the Definition §5 invariant "transcribe() after free() throws synchronously" depends on `AlreadyFreed` being raised before the Promise is constructed; the same shape applies to `UnknownModel` from `loadModel` when called by name.

**A2. Cadmus and CadmusModel are JS classes.** `#[napi]` on a Rust struct + `#[napi(constructor)]` produces a JS class. `new Cadmus({ modelCache: '/path' })` and method calls `cadmus.loadModel(...)`, `model.transcribe(...)`, `model.free()` work directly. The bridge struct holds `std::sync::Arc<cadmus::Cadmus>` and `std::sync::Arc<cadmus::CadmusModel>` respectively — the inner types are cheap to wrap and the `Arc` lets AsyncTasks clone-and-own without a borrow.

**A3. `ModelRef` becomes a discriminated-union object.** TS: `type ModelRef = { name: string } | { path: string }`. The Rust bridge accepts a `ModelRefJs` struct with `name: Option<String>` and `path: Option<String>`, validates that exactly one is set (otherwise returns a synchronous `Error` with `code = "InvalidArgument"`), and constructs `cadmus::ModelRef` accordingly. `loadModel` is the only method that takes a `ModelRef`; the free one-shot `transcribe(audio, modelPath, options)` takes a path string directly per D12.

**A4. Catalog values cross the boundary as plain JS objects.** `ModelInfo` is a `#[napi(object)]` struct on the bridge. `family` is mapped to a string (`"whisper"` / `"distil_whisper"`) — JS doesn't see a Rust enum. All other fields map 1:1 (`size_bytes` → `sizeBytes` via napi-derive's auto-camelcase, `cached: boolean`, `multilingual: boolean`, `description: string`, `repo: string`, `files: string[]`).

**A5. Error mapping carries `code`.** A small helper in `napi_bridge` converts `cadmus::CadmusError` to `napi::Error::new(Status::GenericFailure, message)` and attaches a `code` property on the JS-side error object via the standard napi-rs pattern (`Error::from_reason(...)` plus a manual property set in the resolve path, or returning `napi::Error` whose `Status` carries the code). The mapping is exhaustive (every `CadmusError` variant has a distinct `code` string matching the variant name: `"Load"`, `"Decode"`, `"Resample"`, `"Inference"`, `"Poisoned"`, `"AlreadyFreed"`, `"UnknownModel"`, `"Download"`, `"Io"`, plus `"InvalidArgument"` for `ModelRef` validation failures from A3). Tests assert on `err.code`.

**A6. `downloadModel` progress + cancellation.** `onProgress` is a JS function passed through the AsyncTask via `napi::threadsafe_function::ThreadsafeFunction<...>`. Each Rust `download` callback hop becomes a `tsfn.call(...)` — non-blocking, queued on the JS event loop. `signal: AbortSignal` is mapped before the AsyncTask is dispatched: a one-shot listener on `signal.aborted` flips an `Arc<AtomicBool>` that the Rust download loop already polls. If `signal.aborted` is already `true` at submission, the task short-circuits to `Err(Cancelled)` without spawning. Cancellation is cooperative — same semantics as the Rust side. **No JS-side cancellation test in this plan** (per Discussion); the Rust suite's `cancel_mid_stream_against_local_server` already validates the underlying mechanism.

**A7. `free-during-inflight` JS test uses a synthetic long WAV built from the WAV fixture.** The 2.9 s `eins-zwei-drei.mp3` fixture transcribes too fast on M-series macs (~200–400 ms) for a reliable in-flight window. **Source: `fixtures/eins-zwei-drei.wav`** (already committed: PCM-16 LE, 44.1 kHz mono per [architecture.md §8.1](architecture.md)) — using the WAV fixture avoids any decode step on the JS side. The helper parses the WAV's RIFF/`fmt `/`data` chunks, extracts the PCM payload, and emits a fresh ~30 s WAV that prepends ~25 s of zero-filled PCM silence to the fixture's PCM payload (same sample rate, same bit depth, same channel layout — only the sample count and `data`/`RIFF` length fields change). symphonia handles raw WAV with the already-enabled `wav` + `pcm` features. The resulting `Buffer` goes to `model.transcribe(...)` like any other audio. The test starts the transcribe Promise, awaits a ~50 ms hand-off, calls `model.free()`, and asserts: (a) the in-flight Promise resolves with a non-empty result, (b) a fresh `model.transcribe(...)` call rejects with `code === "AlreadyFreed"`. This mirrors the Rust `free_during_inflight_completes_normally` test 1:1, just over the napi boundary. **No new fixture is committed** — the helper synthesises the long WAV at test time from the existing `eins-zwei-drei.wav`.

**A8. Test concurrency.** `node --test` parallelises across files by default. We disable that with `--test-concurrency=1` in the npm script (or alternately a single test file with sequential subtests). Reason: tests share the model cache on disk and run sequential downloads of `tiny`; parallel test files would race on cache writes. The shared cache is `target/cadmus-test-cache/` — same path the Rust suite already populates; once Rust has run `tiny` is on disk and the JS suite skips the download.

**R1. ThreadsafeFunction lifetime on cancelled / errored paths.** A poorly released `ThreadsafeFunction` keeps the Node event loop alive forever and prevents process exit. Mitigation: `tsfn.unref(env)` on construction so the function does not by itself keep the loop alive; the surrounding Promise keeps it pinned for the duration. The `napi-rs` 3.x `ThreadsafeFunction` API drops cleanly when the AsyncTask resolves. The reviewer should check that `node --test` exits without hanging — the v0.6.0 baseline test already exits cleanly, so any regression is visible.

**R2. WAV header rewriting in `padWavWithSilence`.** Bug-prone (RIFF length, `data` chunk length, byte alignment, padding bytes for odd-length payloads). Mitigation: the helper operates on an already-valid PCM WAV (`fixtures/eins-zwei-drei.wav`) and only **prepends** zero-filled PCM samples plus updates the two length fields (`RIFF` size and `data` size); the `fmt ` chunk is copied verbatim, so sample rate, channel count, and bit depth cannot drift. `tests/wav_helper.test.mjs` (step 20) round-trips `padWavWithSilence(wavBytes, 5)` through `loadModel`+`transcribe` and asserts the eins/zwei/drei markers — proving symphonia accepts the output and the original audio survives — before any lifecycle test depends on the helper.

**R3. CadmusModel JS lifetime vs. native lifetime.** A JS `CadmusModel` that goes out of scope without `free()` leaks the native instance for the process lifetime — by design (Definition §5, no V8 finalizer). The test suite must call `model.free()` in every test's teardown, otherwise repeated test runs accumulate handles. Mitigation: each test builds and frees its own model in a tight `try { ... } finally { model.free(); }` block.

## Steps

### Phase A — Rust napi bridge

1. **Expand `napi_bridge` module structure.** Move it out of `src/lib.rs` into `src/napi.rs` (gated by `#[cfg(feature = "napi")]`); add `mod napi;` plus `#[cfg(feature = "napi")] pub use napi::*;` (or equivalent) in `lib.rs` only for the side effect of registering the napi exports. The existing `version()` napi binding moves into the new file unchanged. Reason: the bridge grows enough that an inline module clutters `lib.rs` and obscures the public Rust API.

2. **Add the `CadmusError` → `napi::Error` mapping helper.** A private `fn cadmus_err_to_napi(e: cadmus::CadmusError) -> napi::Error` returning a `napi::Error` whose `reason` is the `Display` text and whose Status carries the variant code. Wherever the bridge needs to surface a `CadmusError` it goes through this helper — never `unwrap()` or `expect()` on a Rust call.

3. **Add `#[napi(object)] struct ModelInfoJs`** mirroring `cadmus::ModelInfo` with the field-mapping table above (`family: String`). A `From<cadmus::ModelInfo> for ModelInfoJs` keeps the conversion local. Add a unit test under `#[cfg(test)] mod tests` (Rust-side, no napi runtime needed) that constructs a `ModelInfo` and asserts the `family` string round-trip.

4. **Add `#[napi(object)] struct CadmusConfigJs`** with one field `model_cache: String`. Constructor of the bridge `Cadmus` consumes it and constructs `cadmus::CadmusConfig { model_cache: PathBuf::from(s.model_cache) }`.

5. **Add `#[napi(object)] struct ModelRefJs`** with `name: Option<String>` and `path: Option<String>`. Validation helper `fn into_rust(self) -> napi::Result<cadmus::ModelRef>` enforces "exactly one set" — otherwise returns a `napi::Error` with `code = "InvalidArgument"` and a clear message.

6. **Add `#[napi(object)] struct LoadModelOptionsJs`** with `threads: Option<u32>` and `compute_type: Option<String>`. The `compute_type` string accepts `"auto" | "int8" | "int8_float16" | "float16" | "float32"`; unknown values produce `code = "InvalidArgument"`. Default (None) maps to `cadmus::ComputeType::Auto`.

7. **Add `#[napi(object)] struct TranscribeOptionsJs`** with `language: Option<String>` and `beam_size: Option<u32>`. `threads` is intentionally not present — accepted deviation per [bug.kanban.md](bug.kanban.md).

8. **Add `#[napi(object)] struct DownloadModelOptionsJs`** with `on_progress: Option<JsFunction>` (or the napi-rs 3.x `Function<'_, ...>` type, whichever the version in use prefers) and `signal: Option<JsObject>` (the AbortSignal). The `Option<JsFunction>` is converted into an `Option<ThreadsafeFunction<(u64, u64), ...>>` immediately on the JS thread (before AsyncTask dispatch) so the worker can call it without an env handle. The `signal` is also handled JS-thread-side: read `signal.aborted`; if true, short-circuit; otherwise add a one-shot listener that flips an `Arc<AtomicBool>` shared with the worker.

9. **Add `#[napi(object)] struct VersionJs`** — already exists; stays. Add `#[napi(object)] struct SegmentJs { start: f32, end: f32, text: String }`.

10. **Add `#[napi(object)] struct TranscriptResultJs { text: String, language: String, segments: Vec<SegmentJs> }`** with `From<cadmus::TranscriptResult>`.

11. **Define the `#[napi] struct Cadmus`** wrapping `Arc<cadmus::Cadmus>`. Methods (all return `napi::Result`):
    - `#[napi(constructor)] pub fn new(config: CadmusConfigJs) -> napi::Result<Self>` — synchronous; constructs the inner `cadmus::Cadmus` and wraps it in `Arc`. Errors map via the helper.
    - `#[napi] pub fn list_available_models(&self) -> Vec<ModelInfoJs>` — synchronous.
    - `#[napi] pub fn find_model(&self, name: String) -> Option<String>` — synchronous; returns the model directory path string or `None`.
    - `#[napi] pub fn download_model(&self, name: String, options: Option<DownloadModelOptionsJs>) -> AsyncTask<DownloadTask>` — see step 12 below.
    - `#[napi] pub fn load_model(&self, model_ref: ModelRefJs, options: Option<LoadModelOptionsJs>) -> napi::Result<AsyncTask<LoadTask>>` — `ModelRefJs::into_rust` runs synchronously on the JS thread so `InvalidArgument` errors are visible before the Promise. Result on success is the `AsyncTask`.

12. **Define `struct DownloadTask`, `LoadTask`, `TranscribeTask`** — each implements `napi::Task` with `Output = ...` and `JsValue = ...`. Each owns the cloned `Arc` of the inner Rust handle plus the call's options (with the threadsafe progress callback and the cancel-flag `Arc` already wired on the JS thread). `compute()` calls into the synchronous Rust core; `resolve()` maps the result into the JS type.

13. **Define `#[napi] struct CadmusModel`** wrapping `Arc<cadmus::CadmusModel>`. Methods:
    - `#[napi] pub fn transcribe(&self, audio: Buffer, options: Option<TranscribeOptionsJs>) -> AsyncTask<TranscribeTask>` — the `Arc<CadmusModel>` is cloned into the task; the `Buffer` bytes are copied or referenced according to napi-rs's lifetime rules (typically copied — small enough for our fixtures, ~50 KB; for a 30-s WAV ~5.7 MB, still tolerable).
    - `#[napi] pub fn free(&self)` — synchronous; calls `self.inner.free()`. Idempotent (the underlying Rust impl already is).

14. **Add the free one-shot.** `#[napi] pub fn transcribe(audio: Buffer, model_path: String, options: Option<TranscribeOptionsJs>) -> AsyncTask<OneShotTranscribeTask>`. Distinct task type because it constructs a fresh `cadmus::transcribe(...)` call rather than reusing a model handle.

15. **Verify the bridge compiles.** Run `cargo build --release --features napi`. The integration test `tests/public_api.rs` is feature-gated out under `napi`, so this build runs without it. Verify there are no warnings about unused items in either the napi-on or napi-off configurations (the existing surface has none today).

### Phase B — TypeScript wrapper

16. **Move types into `types.ts`.** Hand-written interfaces matching the napi-generated `napi-binding.d.ts` shape but with the public names and the `ModelRef` discriminated union. Exports:
    - `Version` (already exists in `index.ts` — moves to `types.ts`)
    - `ModelFamily = 'whisper' | 'distil_whisper'`
    - `ModelInfo`, `Segment`, `TranscriptResult`
    - `LoadModelOptions`, `TranscribeOptions`, `DownloadModelOptions`
    - `ModelRef = { name: string } | { path: string }`
    - `CadmusConfig = { modelCache: string }`
    - `CadmusError extends Error { code: string }` — type-only narrowing, not a runtime class. Tests assert via `err instanceof Error && (err as { code?: string }).code === '...'`.

17. **Rewrite `index.ts`.** Same platform-dispatch (lines 14–25 today, kept verbatim). The binding's exported items are **re-exported directly — no TS wrapper class**. Re-exports:
    - `version` (function)
    - `Cadmus` (class — the napi-rs class is the public class)
    - `transcribe` (free function)
    
    Plus the type re-exports from `types.ts`. AbortSignal handling lives entirely on the Rust side per Phase A step 8: the napi method receives the `AbortSignal` JS object directly, reads `.aborted` synchronously, and wires a one-shot listener before AsyncTask dispatch. No JS-level translation or wrapper class is needed. The TS surface this plan emits is exactly: platform dispatch + direct re-exports + the type aliases from `types.ts`.

18. **Update `tsconfig.json`** to include `types.ts`. Today the `include` is `["index.ts"]`; change to `["index.ts", "types.ts"]`. `outDir` stays `.` so `tsc` produces `index.js`, `index.d.ts`, `types.js`, `types.d.ts` next to the sources. `package.json`'s `files` array does not list `types.js`/`types.d.ts` — fine, because `index.ts` re-exports everything from `types.ts` and `tsc` inlines the re-exports into `index.d.ts`. **Verify** by running `tsc` and inspecting `index.d.ts`: every declared interface from `types.ts` must appear (or be re-exported from a path the consumer can resolve from `index.d.ts` alone). If not, add `types.js`/`types.d.ts` to `package.json.files`.

### Phase C — Test suite

19. **Add `tests/_helpers/cache.mjs`.** Resolves `target/cadmus-test-cache/` relative to the repo root. Exports `sharedCache(): string` and `ensureTinyDownloaded(cadmus): Promise<void>` (uses `cadmus.listAvailableModels` to check `cached` and only calls `downloadModel` if missing). Reason: every transcribe-touching test needs `tiny`; centralising avoids race-prone copies.

20. **Add `tests/_helpers/wav.mjs`.** Exports `padWavWithSilence(srcWavBytes: Buffer, totalSeconds: number): Buffer`. Source: `fixtures/eins-zwei-drei.wav` (committed; PCM-16 LE, 44.1 kHz mono per [architecture.md §8.1](architecture.md)). The helper parses the source's RIFF header, locates the `fmt ` and `data` chunks, validates `audio_format == 1` (PCM), reads `sample_rate`, `num_channels`, `bits_per_sample`, computes the silence-sample count needed to reach `totalSeconds`, prepends that many zero-filled bytes to the source's PCM payload, and emits a fresh WAV with the same `fmt ` chunk and updated `data` / `RIFF` length fields. No format conversion — same rate, same channels, same bit depth as the input. The lifecycle test calls `padWavWithSilence(wavBytes, 30)`. **Test the helper directly** in `tests/wav_helper.test.mjs` by transcribing `padWavWithSilence(wavBytes, 5)` against `tiny` and asserting `text` contains the eins/zwei/drei markers — proves the helper produces a WAV symphonia accepts and that the original audio survives the prepended silence. **No new fixture is committed.**

21. **`tests/version.test.mjs` (rewrite).** Today's file regex-matches `^0\.2\.0` — broken since 0.3.0. Replace with: `version()` returns three string fields, each non-empty. No version-string regex.

22. **`tests/catalog.test.mjs`.** A `Cadmus` is constructed against the shared cache. Tests:
    - `listAvailableModels()` returns 17 entries.
    - Family-based count: 12 whisper + 5 distil_whisper.
    - Every entry has non-empty `description`, `repo`, `files`, `sizeBytes > 0`.
    - `.en`-suffixed entries have `multilingual === false`.
    - `findModel('nonexistent')` returns `null`.
    - `loadModel({ name: 'nonexistent' })` rejects with `code === 'UnknownModel'`.
    - `loadModel({ name: 'tiny', path: '/x' })` rejects with `code === 'InvalidArgument'` (both fields set).
    - `loadModel({})` rejects with `code === 'InvalidArgument'` (neither field set).

23. **`tests/transcribe.test.mjs`.** Tests:
    - `loadModel({ name: 'tiny' })` → transcribe fixture mp3 → result has non-empty `segments`, `language === 'de'` (because the test passes `language: 'de'`), and `text` contains the eins/zwei/drei markers (same `assert_eins_zwei_drei` logic as the Rust suite — accept "eins"|"1", "zwei"|"2", "drei"|"3"). Then `model.free()` and assert a follow-up `transcribe` rejects with `code === 'AlreadyFreed'`.
    - One-shot `transcribe(audio, modelPath, { language: 'de' })` against the cached `tiny` directory: same content assertion, no model handle to free.

24. **`tests/lifecycle.test.mjs`.** Three tests, each builds and tears down its own `Cadmus` + `CadmusModel`:
    - **free-after-free.** Two consecutive `free()` calls return without throwing; a subsequent `transcribe` rejects with `AlreadyFreed`.
    - **free-during-inflight.** Constructs the long WAV via `_helpers/wav.mjs`; starts `transcribe(longWav, ...)`; awaits a small hand-off (e.g. 50 ms); calls `free()`; awaits the in-flight Promise and asserts it resolves with a non-empty `segments` array; then asserts a fresh `transcribe` rejects with `AlreadyFreed`.
    - **concurrent transcribe.** `await Promise.all([model.transcribe(audio, opts), model.transcribe(audio, opts)])` — both resolve with non-empty `segments`; the eins/zwei/drei markers hold for each.

25. **`tests/download.test.mjs`.** Single happy-path test: a fresh temp cache (NOT the shared one — `fs.mkdtempSync`), construct `Cadmus`, call `downloadModel('tiny', { onProgress })` and assert (a) the call resolves to a directory string, (b) `onProgress` was called at least once with monotonically non-decreasing `received` and a constant `total`, (c) `findModel('tiny')` after the download returns the same directory. Cleanup: remove the temp cache. **No cancellation test** (per A6).

26. **Update `package.json`'s `test` script.** `"test": "node --test --test-concurrency=1 tests/*.test.mjs"` — sequential to avoid cache-write races. The pre-test build (`napi build` + `tsc`) is **not** added to `npm test` automatically; the runbook (D27 verification) and the developer call `npm run build` before `npm test`. Same convention as today.

### Phase D — Build, commit, cross-check

27. **Build the macOS prebuilt binary.** Run `npm run build:napi` (already configured: `napi build --release --platform --no-js --dts napi-binding.d.ts --features napi`). Verify `cadmus.darwin-arm64.node` is produced at the repo root. Run `npm run build:ts`. Verify `index.js`, `index.d.ts`, `types.js`, `types.d.ts` exist.

28. **Run the test grid on macOS.**
    - `cargo test` (default features) — must pass, including the integration tests in `tests/public_api.rs` (gated **out** under `--features napi`, gated **in** here).
    - `cargo build --release --features napi` — must succeed without warnings.
    - `npm test` — all test files pass.
    - `npm pack --dry-run` — succeeds (no error) and lists exactly: `package.json`, `index.js`, `index.d.ts`, `cadmus.darwin-arm64.node`, `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md`. `package.json` is always included by npm regardless of the `files` allowlist (verified locally against this repo). The `cadmus.linux-x64-gnu.node` entry is in `package.json.files` but the file is absent on disk during the macOS-only window — npm silently omits absent allowlisted files (verified locally), so it does **not** appear in the listing and does **not** cause an error. The Linux follow-up plan adds the file and re-running `npm pack --dry-run` then includes it. **Forbidden in the listing:** `Cargo.toml`, `src/`, `tests/`, `fixtures/`, `target/`, `node_modules/`, `docs/`, `napi-binding.d.ts`, `types.js`, `types.d.ts` (unless step 18's verification required adding them — in which case they appear and that is the correct outcome).
    
29. **Commit `cadmus.darwin-arm64.node`** to the repository root. The file is in `package.json`'s `files` allowlist already.

## Verification

The plan is complete when **all of the following pass on macOS** (`aarch64-apple-darwin`):

- `cargo build --release --features napi` succeeds, no warnings.
- `cargo test` (without `--features napi`) — green, including the existing `tests/public_api.rs` integration tests.
- `cargo test --features napi` — gated to skip `tests/public_api.rs` per its `#![cfg(...)]`; the remaining unit tests pass.
- `npm run build` succeeds (both `napi build` and `tsc`).
- `npm test` — all test files pass under `--test-concurrency=1`. Specifically:
    - `tests/version.test.mjs`: 1 test, `version()` shape.
    - `tests/wav_helper.test.mjs`: 1 test, helper produces a transcribable WAV.
    - `tests/catalog.test.mjs`: ≥6 assertions (17 entries, family counts, descriptions, multilingual, `findModel`, `loadModel` error paths).
    - `tests/transcribe.test.mjs`: 2 tests (handle path, one-shot path).
    - `tests/lifecycle.test.mjs`: 3 tests (free-after-free, free-during-inflight, concurrent).
    - `tests/download.test.mjs`: 1 test (happy path with progress).
- `npm pack --dry-run` succeeds and the listing matches step 28's expected set exactly: `package.json`, `index.js`, `index.d.ts`, `cadmus.darwin-arm64.node`, `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md` (plus optional `types.js`/`types.d.ts` only if step 18's verification required them). `cadmus.linux-x64-gnu.node` is absent from the listing during the macOS-only window — that is the expected handoff state.
- `cargo package --list` (D27 sanity-check) emits only the Rust allowlist — no `package.json`, `index.ts`, `tests/*.mjs`, `cadmus.*.node`, or `node_modules/`.
- Ensure repository is clean of `target/`-tracked artefacts and `node_modules/` is `.gitignore`d (already true at HEAD; verify `git status` is clean except for the intended changes plus `cadmus.darwin-arm64.node`).

The Linux half of these checks is **explicitly deferred** to PLAN_linux_followup. No verification step in this plan touches Linux. The committed `cadmus.darwin-arm64.node` plus the Linux allowlist entry that already exists in `package.json` is the handover contract.

<plan_ready>docs/PLAN_napi_surface.md</plan_ready>
