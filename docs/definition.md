# Cadmus — Definition

The What and Why. Target vision; no inventory yet (no code exists).

---

## 1. Problem

Speech-to-text in Node.js/TypeScript currently forces a choice between two unacceptable compromises:

- **Remote APIs** (Whisper API, Google STT, AWS Transcribe): network dependency, per-call cost, latency, privacy exposure.
- **Python wrappers** (faster-whisper, openai-whisper via subprocess): Python runtime requirement, virtualenv management, subprocess fragility, opaque error surfaces.

Local-first applications — Electron desktop apps, on-prem servers, privacy-sensitive tooling — cannot accept either.

## 2. Solution

Cadmus is **one implementation distributed as two artifacts**:

- `cadmus` (crates.io) — Rust library wrapping CTranslate2 (via the `ct2rs` crate) for Whisper inference, plus a pure-Rust audio pipeline (symphonia + rubato). Self-contained, blocking, no FFI escape hatch needed.
- `@ai-inquisitor/cadmus` (npm) — napi-rs bridge over the Rust crate. Translation layer only. Zero JS runtime dependencies. Prebuilt binaries per platform.

The Rust crate is the primary artifact. The npm package is a consumer of the crate, not an alternative path. **All logic lives in the crate**; the bridge maps types and offloads blocking work to the libuv threadpool — nothing else.

## 3. Product Promise

These are constraints stated as features. Violating them breaks the product premise.

| Constraint | Why it matters |
|---|---|
| No Python on the host | Eliminates the dominant fragility class for local STT in JS ecosystems |
| No FFmpeg or system audio libs | symphonia (decode) + rubato (resample) + in-house downmix do everything in pure Rust |
| No CUDA, no GPU (v1) | CPU inference keeps the binary portable; GPU is a later concern |
| Zero npm runtime deps | The `.node` binary is self-contained — no transitive supply chain |
| BLAS/Accelerate is bundled, not installed | Linux/Windows binaries embed Intel oneMKL statically (via `ct2rs`'s `intel-onemkl-prebuild`); macOS uses Apple Accelerate, which ships with the OS. Consumers never run a separate install for math libraries |
| Prebuilt binaries | Consumers run `npm install`; no Rust toolchain, no C++ compiler, no CMake required |
| TypeScript-first, ESM, Node ≥ 22 | The npm side is a first-class TS API, not a JS library with `.d.ts` afterthoughts |
| Format detected from bytes | Caller never specifies format; magic-byte detection in symphonia |
| Testable without hardware | Bundled audio fixture enables full end-to-end CI without microphones |

## 4. Surface — Concepts

The same concepts exist in Rust and JavaScript with idiomatic naming. Exact signatures live in [architecture.md](architecture.md). This section defines *what exists*, not *how it spells*.

> **Plan 5 deltas vs. this section** — flagged inline below with `[Plan 5]` markers. The Concept ([CONCEPT_v1_buildout.md](CONCEPT_v1_buildout.md)) overrides definition.md in three places (D11/D12 for cache & `find_model`, D14/D15 for the catalog & `ModelInfo`, D18 for `ModelRef`). Two `severity: accepted` deviations are recorded in [bug.kanban.md](bug.kanban.md). Full reconciliation happens at Concept Closeout — this section is left as the v1-pre-Concept narrative for traceability.
>
> **Plan 6 deltas vs. this section** — flagged inline below with `[Plan 6]` markers. PLAN_napi_surface materialised the napi-rs bridge and the root TypeScript surface (`@ai-inquisitor/cadmus`): `Cadmus`/`CadmusModel` JS classes, `transcribe()` one-shot, `version()`, plus `DownloadModelOptions` / `LoadModelOptions` / `TranscribeOptions` / `ModelRef` / `CadmusError` exposed as TS types. The JS error contract (`err.code` carries the variant name) is now end-to-end live: synchronous throws (`InvalidArgument`, `AlreadyFreed`, `UnknownModel`) and async-task rejections (`Load`, `Decode`, `Resample`, `Inference`, `Download`, `Io`, `Poisoned`) all surface their codes. Concept Closeout reconciles the inventory.

**Execution model.** The Rust core API is **blocking**: functions return their result directly. Async Rust callers wrap calls in their runtime's blocking-task primitive (`tokio::task::spawn_blocking` or equivalent). The Node bridge offloads each call to the libuv threadpool via napi-rs `AsyncTask` and returns a `Promise`. The core crate carries no executor dependency — runtime choice is the caller's, not the library's.

### 4.1 Operations

| Operation | Purpose |
|---|---|
| `load_model` / `loadModel` | Load a CTranslate2 Whisper model **directory** into memory; returns a stateful context |
| `transcribe` (on context) | Decode audio bytes and run inference; returns a transcript result |
| `transcribe` (one-shot) | Convenience: load → transcribe → free. For scripts and tests, not high-throughput callers |
| `free` (on context) | Release the underlying inference instance. Mandatory on the JS side; on the Rust side `Drop` runs automatically and `free()` is also available |
| `list_available_models` / `listAvailableModels` | Static catalogue of known CTranslate2 Whisper models with size and description **[Plan 5: now a method on the `Cadmus` handle (D12); 17 fixed entries (D14)]** |
| `download_model` / `downloadModel` | Fetch a known CTranslate2 Whisper model from the official faster-whisper repositories on Hugging Face (`Systran/faster-whisper-*`) with optional progress + cancellation **[Plan 5: now a method on the `Cadmus` handle (D12); destination is the configured cache, not a per-call directory]** |
| `find_model` / `findModel` | Locate a model directory across explicit paths, env var, and standard cache dir **[Plan 5: D11 supersedes — now `cadmus.find_model(name)`, cache-relative only; no env var, no platform magic paths, no `searchPaths` argument]** |
| `version` | CTranslate2, ct2rs, and cadmus version strings compiled into the binary |

**[Plan 5: new operation]** `Cadmus::new(CadmusConfig { model_cache })` constructs the handle that hosts `list_available_models` / `find_model` / `download_model` / `load_model` (D12). The free functions `transcribe(audio, &Path, opts)` (one-shot) and `version()` remain free.

### 4.2 Data Types

`TranscriptResult` — full transcript text, detected/specified language code, and per-segment detail.

`Segment` — start time, end time, text. Times are in seconds. Boundaries come from Whisper's timestamp tokens, parsed out of the model output.

`ModelInfo` — model name (e.g. `tiny`, `base`, `small`, `medium`, `large-v3`), approximate download size in bytes, one-line description, expected file list inside the model directory. **[Plan 5: per D15 the surfaced shape is `name`, `description`, `size_bytes`, `family` (Whisper / DistilWhisper), `multilingual`, `cached` (computed at call time per D19), `repo`, `files`.]**

`ModelRef` **[Plan 5, D18]** — `Name(String)` for catalog entries (resolved via the configured cache) or `Path(PathBuf)` for arbitrary directories. `From<&str>`, `From<String>`, `From<&Path>`, `From<PathBuf>` for ergonomic call sites.

`LoadModelOptions` — thread count override (defaults to logical CPU count), compute type (e.g. `int8`, `float16`, `float32`; defaults to model's native; **Plan 5/D16: default is `Auto`**).

`TranscribeOptions` — language (BCP-47, or `auto` for ct2rs's language detection), beam size, per-call thread count override. **[Plan 5: `threads` is dropped — `severity: accepted` deviation in [bug.kanban.md](bug.kanban.md) ("`TranscribeOptions::threads` not implemented"). ct2rs 0.9.18 has no per-call thread surface; `LoadModelOptions::threads` is the only thread knob.]**

`DownloadModelOptions` — progress callback (Rust: `Box<dyn Fn(u64, u64) + Send + Sync>`; JS: `(received, total) => void`); cooperative cancellation (Rust: `Arc<AtomicBool>` polled inside the download loop; JS: `AbortSignal`). Cancellation is cooperative on both sides — there is no preemptive interruption of inference or decode.

### 4.3 Errors

A single error type with discriminated variants:

| Variant | Cause |
|---|---|
| `Load` | Model directory missing, incomplete, or rejected by ct2rs/CTranslate2 at init |
| `Decode` | Audio bytes are corrupt or in an unrecognised format |
| `Resample` | rubato or downmix stage failed (extremely rare; malformed sample rates) |
| `Inference` | ct2rs returned a failure from `Whisper::generate` |
| `Poisoned` | Internal lock poisoned by a panic on another thread; context is unusable |
| `AlreadyFreed` | Operation called on a context after `free()` |
| `UnknownModel` | **[Plan 5]** Catalog-name lookup failed: name is not one of the 17 entries (D14) |
| `Download` | **[Plan 5]** HuggingFace download failed (cancelled, HTTP error, network, IO). The `DownloadError` four-variant detail is collapsed into one string for surface narrowness |
| `Io` | **[Plan 5]** Filesystem error on the cache directory (cannot create, cannot read) |

The npm side surfaces these as `Error` instances with a `code` field carrying the variant name. **[Plan 6: implemented.]** Synchronous throws (`AlreadyFreed`, `UnknownModel` via `loadModel({ name })`, `InvalidArgument` for ModelRef shape and unknown `computeType`) propagate the typed code via `JsError<String>::throw_into` plus a `PendingException` sentinel. AsyncTask rejections (`Load`, `Decode`, `Resample`, `Inference`, `Download`, `Io`, `Poisoned`) propagate the typed code by building the JS Error directly with `napi_create_error` in `Task::reject` and packing it into the `napi::Error::maybe_raw` slot, so the framework's deferred-reject path forwards our error verbatim.

## 5. Behavioral Invariants

Things callers must rely on. Things implementers must not break.

**`free()` is mandatory on the JS side.** A context that goes out of scope in JS without `free()` leaks the native inference instance for the lifetime of the process. There is no V8 finalizer-based release. On the Rust side, `Drop` runs automatically at scope exit; calling `.free()` explicitly is also valid and equivalent.

**`transcribe()` after `free()` throws synchronously** with the `AlreadyFreed` error variant. It does not return a rejected promise — the failure is observable before any async work begins. **[Plan 6: enforced by a mirror `freed` flag on the napi-side `CadmusModel` checked before `AsyncTask` construction; `tests/lifecycle.test.mjs` covers free-after-free.]**

**`free()` does not abort in-flight transcriptions.** A `transcribe()` Promise created *before* `free()` resolves normally with its result; `free()` is non-blocking and the underlying instance is released only after all in-flight calls finish. New `transcribe()` calls submitted *after* `free()` always throw `AlreadyFreed`. This is a deliberate value-over-abort choice — see [architecture.md §5](architecture.md) for the mechanism (reference-counted deferred release). **[Plan 6: re-verified across the AsyncTask boundary — `tests/lifecycle.test.mjs` runs a 30 s synthesised WAV, calls `free()` mid-flight, and asserts the in-flight Promise resolves with non-empty segments while the next call rejects with `code === 'AlreadyFreed'`.]**

**Audio format is detected from content, not from filename or caller hints.** A `.mp3` file containing WAV data transcribes correctly. Truly corrupt audio raises `Decode`.

**`TranscriptResult.text` is segments joined with no separator beyond what the model emits.** Segments may carry leading whitespace from Whisper's tokenizer. `text.trim()` is always safe.

**`TranscriptResult.language` echoes intent.** If `options.language` was set explicitly, the result repeats it. If `auto` was used, the result carries ct2rs's language detection. **[Plan 5: when `language == None`, the result is currently `""` — ct2rs 0.9.18 runs detection internally but discards the detected token before returning chunks. `severity: accepted` deviation in [bug.kanban.md](bug.kanban.md) ("Detected language not surfaced"); upstream fix tracked in [backlog.kanban.md](backlog.kanban.md). The explicit-language round-trip case is unaffected.]**

**Segment times come from Whisper's `<|t|>` timestamp tokens**, parsed by Cadmus from the model output. Granularity is segment-level, typically 30-second chunks subdivided by silence and punctuation. Word-level timestamps are out of scope for v1.

**`downloadModel` does not verify integrity.** No checksum. A truncated download surfaces later as a `Load` error when the consumer tries to load the directory.

**`findModel` returns the first match.** Search order is: explicit `searchPaths`, then `CADMUS_MODEL_DIR` env var, then `~/.cache/cadmus/models/`. Duplicate names: first wins, deterministically. **[Plan 5: D11 supersedes — `cadmus.find_model(name)` looks up only inside the configured `model_cache`, returns `Some(dir)` iff every catalog file is present with size > 0 (D19), `None` otherwise. No env, no magic paths, no `searchPaths` argument.]**

**Concurrent `transcribe()` on the same context is safe.** ct2rs/CTranslate2 manage internal batching across replicas. Whether external synchronization is required is verified during the first implementation plan; if needed, `CadmusModel` adds an internal lock without changing the public contract. **[Plan 6: re-verified across the AsyncTask boundary by `tests/lifecycle.test.mjs` — `Promise.all([transcribe, transcribe])` resolves both with valid segments.]**

**`downloadModel` progress is monotonic against a constant total.** A single `downloadModel(name, { onProgress })` call delivers `(received, total)` events where `received` is non-decreasing and `total` stays equal to the catalog's `sizeBytes` for the model across every call. **[Plan 6: enforced in the napi bridge — the underlying `storage::download` reports per-file progress with per-file totals; the bridge accumulates committed-file bytes and clamps against the catalog total before forwarding to the JS callback.]**

**Default `threads` equals logical CPU count.** Test environments running multiple contexts simultaneously must lower this explicitly to avoid memory pressure.

## 6. Out of Scope (v1)

Stated explicitly so future contributors do not assume otherwise:

- GPU inference (CUDA, Metal, Vulkan)
- Streaming transcription / real-time partial results
- Speaker diarisation
- Word-level timestamps (segment-level only via Whisper timestamp tokens)
- Model integrity verification (checksums, signatures)
- Auto-`free` via V8 finalizer on the JS side
- Word error rate guarantees — accuracy is CTranslate2 + Whisper's responsibility, not Cadmus's

## 7. Success Criteria

The product is successful when:

1. A Rust application adds `cadmus` to `Cargo.toml` and transcribes audio bytes without any system dependency beyond a C++ toolchain and CMake at build time, plus Apple Accelerate (where the OS provides it) at runtime.
2. A Node.js or Electron application runs `npm install @ai-inquisitor/cadmus` on Linux x64, macOS arm64, or Windows x64 and transcribes audio without a Rust toolchain, Python, FFmpeg, or a separate BLAS install.
3. CI transcribes the bundled fixture (`fixtures/eins-zwei-drei.mp3`) on every push and asserts the result contains the expected words — across all three platforms.
4. The npm `.node` binary loads in Electron renderer or main process without additional native module configuration beyond standard napi-rs conventions.
