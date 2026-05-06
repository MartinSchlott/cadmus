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

**Execution model.** The Rust core API is **blocking**: functions return their result directly. Async Rust callers wrap calls in their runtime's blocking-task primitive (`tokio::task::spawn_blocking` or equivalent). The Node bridge offloads each call to the libuv threadpool via napi-rs `AsyncTask` and returns a `Promise`. The core crate carries no executor dependency — runtime choice is the caller's, not the library's.

### 4.1 Operations

| Operation | Purpose |
|---|---|
| `load_model` / `loadModel` | Load a CTranslate2 Whisper model **directory** into memory; returns a stateful context |
| `transcribe` (on context) | Decode audio bytes and run inference; returns a transcript result |
| `transcribe` (one-shot) | Convenience: load → transcribe → free. For scripts and tests, not high-throughput callers |
| `free` (on context) | Release the underlying inference instance. Mandatory on the JS side; on the Rust side `Drop` runs automatically and `free()` is also available |
| `list_available_models` / `listAvailableModels` | Static catalogue of known CTranslate2 Whisper models with size and description |
| `download_model` / `downloadModel` | Fetch a known CTranslate2 Whisper model from the official faster-whisper repositories on Hugging Face (`Systran/faster-whisper-*`) with optional progress + cancellation |
| `find_model` / `findModel` | Locate a model directory across explicit paths, env var, and standard cache dir |
| `version` | CTranslate2, ct2rs, and cadmus version strings compiled into the binary |

### 4.2 Data Types

`TranscriptResult` — full transcript text, detected/specified language code, and per-segment detail.

`Segment` — start time, end time, text. Times are in seconds. Boundaries come from Whisper's timestamp tokens, parsed out of the model output.

`ModelInfo` — model name (e.g. `tiny`, `base`, `small`, `medium`, `large-v3`), approximate download size in bytes, one-line description, expected file list inside the model directory.

`LoadModelOptions` — thread count override (defaults to logical CPU count), compute type (e.g. `int8`, `float16`, `float32`; defaults to model's native).

`TranscribeOptions` — language (BCP-47, or `auto` for ct2rs's language detection), beam size, per-call thread count override.

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

The npm side surfaces these as `Error` instances with a `code` field carrying the variant name.

## 5. Behavioral Invariants

Things callers must rely on. Things implementers must not break.

**`free()` is mandatory on the JS side.** A context that goes out of scope in JS without `free()` leaks the native inference instance for the lifetime of the process. There is no V8 finalizer-based release. On the Rust side, `Drop` runs automatically at scope exit; calling `.free()` explicitly is also valid and equivalent.

**`transcribe()` after `free()` throws synchronously** with the `AlreadyFreed` error variant. It does not return a rejected promise — the failure is observable before any async work begins.

**`free()` does not abort in-flight transcriptions.** A `transcribe()` Promise created *before* `free()` resolves normally with its result; `free()` is non-blocking and the underlying instance is released only after all in-flight calls finish. New `transcribe()` calls submitted *after* `free()` always throw `AlreadyFreed`. This is a deliberate value-over-abort choice — see [architecture.md §5](architecture.md) for the mechanism (reference-counted deferred release).

**Audio format is detected from content, not from filename or caller hints.** A `.mp3` file containing WAV data transcribes correctly. Truly corrupt audio raises `Decode`.

**`TranscriptResult.text` is segments joined with no separator beyond what the model emits.** Segments may carry leading whitespace from Whisper's tokenizer. `text.trim()` is always safe.

**`TranscriptResult.language` echoes intent.** If `options.language` was set explicitly, the result repeats it. If `auto` was used, the result carries ct2rs's language detection.

**Segment times come from Whisper's `<|t|>` timestamp tokens**, parsed by Cadmus from the model output. Granularity is segment-level, typically 30-second chunks subdivided by silence and punctuation. Word-level timestamps are out of scope for v1.

**`downloadModel` does not verify integrity.** No checksum. A truncated download surfaces later as a `Load` error when the consumer tries to load the directory.

**`findModel` returns the first match.** Search order is: explicit `searchPaths`, then `CADMUS_MODEL_DIR` env var, then `~/.cache/cadmus/models/`. Duplicate names: first wins, deterministically.

**Concurrent `transcribe()` on the same context is safe.** ct2rs/CTranslate2 manage internal batching across replicas. Whether external synchronization is required is verified during the first implementation plan; if needed, `CadmusModel` adds an internal lock without changing the public contract.

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
