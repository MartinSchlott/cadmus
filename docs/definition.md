# Cadmus — Definition

The What and Why. Target vision plus inventory at v1.0.0 — the surface
described here is the surface that ships.

For *how* it is built, see [architecture.md](architecture.md).

---

## 1. Problem

Speech-to-text in Node.js/TypeScript currently forces a choice between two unacceptable compromises:

- **Remote APIs** (Whisper API, Google STT, AWS Transcribe): network dependency, per-call cost, latency, privacy exposure.
- **Python wrappers** (faster-whisper, openai-whisper via subprocess): Python runtime requirement, virtualenv management, subprocess fragility, opaque error surfaces.

Local-first applications — Electron desktop apps, on-prem servers, privacy-sensitive tooling — cannot accept either.

## 2. Solution

Cadmus is **one implementation distributed as two artifacts**:

- `cadmus` (Rust crate — consumed as a git dependency, not yet published to crates.io) — Rust library wrapping CTranslate2 (via the `ct2rs` crate) for Whisper inference, plus a pure-Rust audio pipeline (symphonia + rubato). Self-contained, blocking, no FFI escape hatch needed.
- `@ai-inquisitor/cadmus` (npm) — napi-rs bridge over the Rust crate, in the same single Cargo crate behind a `napi` feature flag. Translation layer only. Zero JS runtime dependencies. Prebuilt binaries committed to the repository per platform.

The Rust crate is the primary artifact. The npm package is a consumer of the crate, not an alternative path. **All logic lives in the crate**; the bridge maps types and offloads blocking work to the libuv threadpool — nothing else.

## 3. Product Promise

These are constraints stated as features. Violating them breaks the product premise.

| Constraint | Why it matters |
|---|---|
| No Python on the host | Eliminates the dominant fragility class for local STT in JS ecosystems |
| No FFmpeg or system audio libs | symphonia (decode) + rubato (resample) + in-house downmix do everything in pure Rust |
| No CUDA, no GPU (v1) | CPU inference keeps the binary portable; GPU is a later concern |
| Zero npm runtime deps | The `.node` binary is self-contained — no transitive supply chain |
| BLAS/Accelerate is bundled, not installed | The Linux binary embeds Intel oneMKL statically (via `ct2rs`'s `intel-onemkl-prebuild`); macOS uses Apple Accelerate, which ships with the OS. Consumers never run a separate install for math libraries |
| Prebuilt binaries committed | Consumers run `npm install`; no Rust toolchain, no C++ compiler, no CMake required. The `.node` binaries live in the repository; releases ship them via the npm `files` allowlist |
| TypeScript-first, ESM, Node ≥ 22 | The npm side is a first-class TS API, not a JS library with `.d.ts` afterthoughts |
| Format detected from bytes | Caller never specifies format; magic-byte detection in symphonia |
| Testable without hardware | Bundled audio fixture enables full end-to-end verification without microphones |

## 4. Surface — Concepts

The same concepts exist in Rust and JavaScript with idiomatic naming. Exact signatures live in [architecture.md §9](architecture.md). This section defines *what exists*, not *how it spells*.

**Execution model.** The Rust core API is **blocking**: functions return their result directly. Async Rust callers wrap calls in their runtime's blocking-task primitive (`tokio::task::spawn_blocking` or equivalent). The Node bridge offloads each call to the libuv threadpool via napi-rs `AsyncTask` and returns a `Promise`. The core crate carries no executor dependency — runtime choice is the caller's, not the library's.

**Factory pattern.** Cadmus is constructed once as a stateful handle (`Cadmus`) holding the explicit model-cache directory. Catalog inspection, model resolution, downloading, and loading are methods on that handle. Two operations remain free functions because they need no cache: `version()` and the one-shot `transcribe(audio, modelPath, opts)` — the latter takes a path, not a `ModelRef`, since catalog-name resolution requires a `Cadmus` handle.

### 4.1 Operations

| Operation | Purpose |
|---|---|
| `Cadmus::new` / `new Cadmus(...)` | Construct the handle with `CadmusConfig { model_cache }`; creates the cache directory if absent. Rust returns `Result<Cadmus, CadmusError>`; JS throws `CadmusError` synchronously on failure |
| `cadmus.list_available_models` / `listAvailableModels` | Static catalog of 17 known CTranslate2 Whisper models with size, description, family flags, and per-call `cached` status |
| `cadmus.find_model` / `findModel` | Locate a catalog model inside the configured cache; returns the directory path if every catalog file is present with non-zero size, otherwise `None` / `null` |
| `cadmus.download_model` / `downloadModel` | Fetch a catalog model from its HuggingFace repository into the configured cache, with optional progress callback and cooperative cancellation |
| `cadmus.load_model` / `loadModel` | Load a CTranslate2 Whisper model into memory; accepts a `ModelRef` (catalog name resolved against the cache, or absolute path); returns a stateful `CadmusModel` |
| `model.transcribe` | Decode audio bytes and run inference; returns a `TranscriptResult` |
| `transcribe` (one-shot, free function) | Convenience: load → transcribe → free, for scripts and tests. Takes an absolute model directory path, not a `ModelRef` |
| `model.free` | Release the underlying inference instance. Mandatory on the JS side; on the Rust side `Drop` runs automatically and `free()` is also available |
| `version` (free function) | CTranslate2, ct2rs, and cadmus version strings compiled into the binary |

### 4.2 Data Types

`TranscriptResult` — full transcript text, language code, and per-segment detail.

`Segment` — start time, end time, text. Times are in seconds. Boundaries come from Whisper's timestamp tokens, parsed out of the model output.

`ModelInfo` — catalog entry shape:

| Field | Meaning |
|---|---|
| `name` | Catalog name, e.g. `tiny`, `base`, `large-v3`, `distil-large-v3.5` |
| `description` | One short sentence, GUI-displayable |
| `size_bytes` / `sizeBytes` | Approximate download size in bytes |
| `family` | `Whisper` or `DistilWhisper` |
| `multilingual` | `false` for `.en` and Distil-EN-only entries; `true` otherwise |
| `cached` | Computed at call time: every catalog file is present in the cache with size > 0 |
| `repo` | HuggingFace repository, e.g. `Systran/faster-whisper-base` |
| `files` | Expected files inside the model directory; per-file repo overrides are an implementation detail |

`ModelRef` — discriminated input for `load_model`:
- `Name` (Rust: `&str` / String) — catalog entry resolved against the configured cache
- `Path` (Rust: `&Path` / `PathBuf`) — direct path to a model directory

`LoadModelOptions` — thread count override (defaults to logical CPU count), compute type. The default `compute_type` is `Auto` — ct2rs picks based on the model. Documentation recommends `int8` for CPU users who want maximum throughput.

`TranscribeOptions` — language (BCP-47, or absent for ct2rs's internal language detection) and beam size. `threads` is intentionally not surfaced — see `docs/bug.kanban.md` ("`TranscribeOptions::threads` not implemented"); ct2rs 0.9.18 has no per-call thread override, only per-instance via `LoadModelOptions::threads`.

`DownloadModelOptions` — progress callback (Rust: `Box<dyn Fn(u64, u64) + Send + Sync>`; JS: `(received, total) => void`); cooperative cancellation (Rust: `Arc<AtomicBool>`; JS: `AbortSignal`). Cancellation is cooperative on both sides — there is no preemptive interruption of inference or decode.

### 4.3 Errors

A single error type with discriminated variants. The npm side surfaces these as `Error` instances with a `code` field carrying the variant name; synchronous throws and async rejections both propagate the typed code.

| Variant | Cause |
|---|---|
| `Load` | Model directory missing, incomplete, or rejected by ct2rs/CTranslate2 at init |
| `Decode` | Audio bytes are corrupt or in an unrecognised format |
| `Resample` | rubato or downmix stage failed (extremely rare; malformed sample rates) |
| `Inference` | ct2rs returned a failure from `Whisper::generate` |
| `Poisoned` | Internal lock poisoned by a panic on another thread; context is unusable |
| `AlreadyFreed` | Operation called on a context after `free()` |
| `UnknownModel` | Catalog-name lookup failed: name is not one of the 17 entries |
| `Download` | HuggingFace download failed (cancelled, HTTP error, network, IO). Detail is collapsed into one string for surface narrowness |
| `Io` | Filesystem error on the cache directory (cannot create, cannot read) |
| `InvalidArgument` | Malformed `ModelRef` (both `name` and `path` set, or neither), unknown `computeType`, or other shape violations at the napi boundary |

## 5. Behavioral Invariants

Things callers must rely on. Things implementers must not break.

**`free()` is mandatory on the JS side.** A context that goes out of scope in JS without `free()` leaks the native inference instance for the lifetime of the process. There is no V8 finalizer-based release. On the Rust side, `Drop` runs automatically at scope exit; calling `.free()` explicitly is also valid and equivalent.

**`transcribe()` after `free()` throws synchronously** with the `AlreadyFreed` error variant. It does not return a rejected promise — the failure is observable before any async work begins. Enforced by a mirror `freed` flag on the napi-side `CadmusModel` checked before `AsyncTask` construction.

**`free()` does not abort in-flight transcriptions.** A `transcribe()` Promise created *before* `free()` resolves normally with its result; `free()` is non-blocking and the underlying instance is released only after all in-flight calls finish. New `transcribe()` calls submitted *after* `free()` always throw `AlreadyFreed`. This is a deliberate value-over-abort choice — see [architecture.md §5](architecture.md) for the mechanism (reference-counted deferred release). Verified directly against the napi/AsyncTask boundary.

**Audio format is detected from content, not from filename or caller hints.** A `.mp3` file containing WAV data transcribes correctly. Truly corrupt audio raises `Decode`.

**`TranscriptResult.text` is segments joined with no separator beyond what the model emits.** Segments may carry leading whitespace from Whisper's tokenizer. `text.trim()` is always safe.

**`TranscriptResult.language` echoes intent.** If `options.language` was set explicitly, the result repeats it. If it was unset, the result currently carries an empty string — ct2rs 0.9.18 runs language detection internally but discards the detected token before returning chunks, so the detected language is unreachable from the public surface. Documented as a `severity: accepted` deviation in `docs/bug.kanban.md` ("Detected language not surfaced when `TranscribeOptions::language == None`"); upstream fix tracked in `docs/backlog.kanban.md` ("Surface ct2rs internally-detected language token"). The explicit-language round-trip case (the common one) is unaffected.

**Segment times come from Whisper's `<|t|>` timestamp tokens**, parsed by Cadmus from the model output. Granularity is segment-level, typically 30-second chunks subdivided by silence and punctuation. Word-level timestamps are out of scope for v1.

**`download_model` does not verify integrity.** No checksum. A truncated download surfaces later as a `Load` error when the consumer tries to load the directory. Resumable downloads (HTTP Range) are tracked in `docs/backlog.kanban.md`.

**`find_model` is cache-relative and strict.** Search target is the configured `model_cache` directory only. No environment-variable lookup, no platform-specific magic paths, no fallback search list. Returns `Some(dir)` iff the model directory exists *and* every entry from `ModelInfo::files` is present with non-zero size; `None` otherwise.

**Concurrent `transcribe()` on the same context is safe.** ct2rs/CTranslate2's `ffi::Whisper` is `Send + Sync` (verified directly in `ct2rs/src/sys/whisper.rs`); `Whisper::generate` runs lock-free against an `Arc<Whisper>` clone. Verified across the napi/AsyncTask boundary by `tests/lifecycle.test.mjs` and as a Rust unit test in `src/inference.rs`.

**`download_model` progress is monotonic against a constant total.** A single `download_model(name, { onProgress })` call delivers `(received, total)` events where `received` is non-decreasing and `total` stays equal to the catalog's `size_bytes` for the model across every call. The bridge accumulates committed-file bytes and clamps against the catalog total before forwarding to the JS callback.

**Default `threads` equals logical CPU count.** Test environments running multiple contexts simultaneously must lower this explicitly via `LoadModelOptions::threads` to avoid memory pressure.

## 6. Out of Scope (v1)

Stated explicitly so future contributors do not assume otherwise. All deferred items are tracked as cards in `docs/backlog.kanban.md`.

- Linux x86_64 build itself — v1.0.0 ships macOS arm64 only; Linux is wired in `Cargo.toml` / `package.json` / `index.ts` but the binary is not yet committed. Backlog: "Linux x86_64 follow-up build" (Open).
- Windows x86_64 build (`x86_64-pc-windows-msvc`).
- Linux-arm64 and macOS-x64 builds.
- GPU inference (CUDA, Metal, Vulkan).
- Streaming transcription / real-time partial results.
- Speaker diarisation.
- Word-level timestamps (segment-level only via Whisper timestamp tokens).
- Model integrity verification (checksums, signatures).
- HTTP Range / resume on `download_model`.
- Auto-`free` via V8 finalizer on the JS side.
- Word error rate guarantees — accuracy is CTranslate2 + Whisper's responsibility, not Cadmus's.
- GitHub Actions / CI matrix — v1 verification is local on each build host (see `architecture.md §6`).

## 7. Success Criteria

The product is successful when:

1. A Rust application adds `cadmus` to `Cargo.toml` and transcribes audio bytes without any system dependency beyond a C++ toolchain and CMake at build time, plus Apple Accelerate (where the OS provides it) at runtime.
2. A Node.js or Electron application runs `npm install @ai-inquisitor/cadmus` on macOS arm64 (and Linux x64 once the follow-up build lands) and transcribes audio without a Rust toolchain, Python, FFmpeg, or a separate BLAS install.
3. The bundled fixture (`fixtures/eins-zwei-drei.mp3`) transcribes locally on each supported build host and asserts the result contains the expected words. Verification is manual per the Release Runbook (`docs/archive/CONCEPT_v1_buildout.md` Release Runbook, retained for reference) — no CI matrix.
4. The npm `.node` binary loads in Electron renderer or main process without additional native module configuration beyond standard napi-rs conventions.
