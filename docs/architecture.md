# Cadmus — Architecture

The How. Target vision plus inventory at v1.0.0.

For *what* Cadmus is and the contract it exposes, see [definition.md](definition.md).

---

## 1. Technology Stack

| Layer | Choice | Rationale |
|---|---|---|
| Inference engine | CTranslate2 (C++17) | Fast, memory-efficient transformer inference; int8 quantisation; the engine behind faster-whisper |
| Engine binding | `ct2rs` (Rust, pinned to `=0.9.18`) | Mature MIT-licensed Rust wrapper around CTranslate2 with a `whisper` feature that ships mel-spectrogram and tokenizer support. Bundles CTranslate2 statically — we do not drive its build ourselves |
| BLAS backend (Linux) | Intel oneMKL via `intel-onemkl-prebuild` | Statically linked, no system install. Fastest CPU path on x86_64. Combined with ct2rs features `dnnl` and `openmp-runtime-comp` |
| BLAS backend (macOS arm64) | Apple Accelerate + ruy | Apple Accelerate ships with macOS — no extra install. ruy provides ARM matmul primitives |
| Audio decoding | symphonia (Rust) | Pure Rust decoder/demuxer with magic-byte format detection. Decodes to interleaved float samples at the source's native rate and channel count. Eliminates FFmpeg/PyAV. Feature set: `mp3`, `wav`, `flac`, `pcm`, `mkv`, `isomp4`, `aac` — covers MP3/WAV/FLAC plus the two browser `MediaRecorder` defaults (WebM/Opus container via `mkv`; MP4/AAC-LC via `isomp4` + `aac`) |
| Opus decoding | `unsafe-libopus` (Rust) | symphonia 0.5.4 ships an empty Opus decoder stub; we route Opus packets demuxed from the WebM container to `unsafe-libopus` — a `c2rust` transpile of libopus 1.3.1. Pure-Rust build (no C toolchain), BSD-3-Clause. All `unsafe` is isolated in `src/opus.rs` behind a safe wrapper |
| Resampling | rubato (Rust) | Sinc-based resampler from arbitrary input rate to Whisper's required 16 kHz. Pure Rust |
| Channel downmix | in-house (Rust) | Stereo/multi-channel → mono via float averaging. Trivial enough to live in `decode.rs` rather than pulling another crate |
| HTTP downloader | `ureq` with rustls + platform-verifier | Pure-Rust TLS, no system OpenSSL. Used by `storage.rs` to fetch HuggingFace model files |
| Node.js bridge | napi-rs (3.x) | Single `.node` artifact per platform; `AsyncTask` pattern offloads blocking inference to the libuv threadpool. Lives in the same Cargo crate behind a `napi` feature flag |

### Why CTranslate2, not whisper.cpp

CTranslate2 is faster on CPU (typical 1.5–3× at comparable quantisation) and produces smaller models (int8 ~30 % smaller than GGML q5_1). It is the inference engine behind the production-grade `faster-whisper` ecosystem. The downside — additional BLAS dependency — is mitigated by static linking on x86 and by Apple Accelerate on arm64 macOS.

### Why ct2rs, not a hand-written FFI layer

`ct2rs` is MIT-licensed and provides a `Whisper` struct whose `generate(samples: &[f32], language, timestamp, options) -> Result<Vec<String>>` method consumes 16 kHz mono float samples directly and returns text — mel-spectrogram, tokenizer, and decoding loop are internal. Building this surface ourselves would mean writing a C bridge over CTranslate2's C++-only API, implementing the Whisper log-mel-spectrogram, integrating a tokenizer, writing the encoder/decoder beam-search loop, and handling timestamp token parsing. `ct2rs` provides all of this. We add only the Cadmus-specific surface: audio pipeline, model management, error mapping, segment construction from timestamp tokens.

### Why symphonia, not FFmpeg bindings

Pure Rust eliminates the dominant deployment failure: a missing or version-mismatched system library. Magic-byte format detection means the API never asks the caller "what format is this?". The fixture-based test strategy is only viable because the decoder is itself part of the build artifact.

symphonia covers decode and format detection only. Resampling and channel downmix are explicit separate stages — `rubato` for the former, in-house averaging for the latter.

### Why a sync core, not async

The `cadmus` crate exposes blocking functions. It does not depend on tokio, async-std, or any other executor.

The reason is separation of concerns: blocking inference is the truth, and offload is a deployment decision that belongs to the caller. The Node bridge offloads to the libuv threadpool via napi-rs `AsyncTask`. Async Rust callers wrap calls in `tokio::task::spawn_blocking` or their runtime's equivalent. Pulling a runtime choice into the core would force every consumer to pay for a decision they may not want.

---

## 2. Repository Structure

A single Cargo crate at the repository root. `[lib] crate-type = ["cdylib", "lib"]` lets one source tree produce both the rlib (for Rust consumers via a git dependency) and the cdylib (for napi-rs via npm). The napi bridge is gated behind a `napi` feature flag — Rust consumers never compile any napi code.

```
/
├── Cargo.toml                      # single crate; [package].include allowlist;
│                                   #   [lib] crate-type = ["cdylib", "lib"];
│                                   #   [features] napi = ["dep:napi", "dep:napi-derive"];
│                                   #   per-target ct2rs feature subsets (macOS / Linux / Windows)
├── Cargo.lock                      # committed
├── build.rs                        # gates napi_build::setup() on CARGO_FEATURE_NAPI
├── package.json                    # @ai-inquisitor/cadmus; files allowlist; type: module
├── tsconfig.json                   # outDir: ".", emits index.{js,d.ts} + types.{js,d.ts} at root
├── index.ts                        # platform dispatch + re-exports the napi class
├── types.ts                        # hand-written TS types
├── index.js / index.d.ts           # tsc-emitted, gitignored
├── types.js / types.d.ts           # tsc-emitted, gitignored
├── napi-binding.d.ts               # napi-rs auto-generated; internal
├── LICENSE                         # MIT, Copyright (c) 2026 Martin Schlott
├── LICENSE-THIRD-PARTY             # symphonia (MPL-2.0), libopus via unsafe-libopus (BSD-3-Clause)
├── README.md
├── cadmus.darwin-arm64.node        # prebuilt; committed; built locally via scripts/release.mjs
├── cadmus.linux-x64-gnu.node       # prebuilt; committed; built on GitHub-hosted runner (ubuntu-latest)
├── cadmus.win32-x64-msvc.node      # prebuilt; committed; built on GitHub-hosted runner (windows-latest)
├── scripts/
│   └── release.mjs                 # local release driver: builds darwin-arm64, pushes, triggers CI
├── src/
│   ├── lib.rs                      # public Rust API + #[cfg(feature = "napi")] bridge re-exports
│   ├── api.rs                      # Cadmus, CadmusModel, ModelRef, options structs, transcribe one-shot
│   ├── catalog.rs                  # ModelSpec / FileSpec types + default_models() (6 multilingual entries)
│   ├── decode.rs                   # symphonia + rubato + downmix → Vec<f32> @ 16 kHz mono;
│   │                               #   Opus packets routed to src/opus.rs (R1 pre-skip honoured)
│   ├── error.rs                    # CadmusError + AudioError + InferenceError variants
│   ├── inference.rs                # InferenceHandle (D4), Whisper <|t|> segment parser,
│   │                               #   detect_language_from_chunks helper
│   ├── napi.rs                     # napi-rs bridge (compiled only with --features napi)
│   ├── opus.rs                     # safe wrapper over unsafe-libopus; OpusHead parser,
│   │                               #   pre-skip helper; only `unsafe` block in the crate
│   └── storage.rs                  # URL-driven downloader (ureq + rustls for http(s);
│                                   #   percent-decoded file:// for local sources), find_model,
│                                   #   ensure_present_files
├── fixtures/
│   ├── eins-zwei-drei.mp3          # ≈ 2.9 s synthesized German numerals @ 22 050 Hz (master)
│   ├── eins-zwei-drei.wav          # same recording, PCM-16 @ 44 100 Hz
│   ├── eins-zwei-drei.flac         # same recording, FLAC @ 48 000 Hz
│   ├── eins-zwei-drei.webm         # ffmpeg-derived, WebM/Opus @ 48 000 Hz mono
│   └── eins-zwei-drei.m4a          # ffmpeg-derived, MP4/AAC-LC @ 44 100 Hz mono
├── tests/
│   ├── _helpers/                   # JS test helpers (e.g. wav.mjs::padWavWithSilence)
│   ├── public_api.rs               # Rust integration tests (rlib path; gated off under --features napi)
│   ├── catalog.test.mjs            # node --test
│   ├── download.test.mjs           # node --test
│   ├── file_url.test.mjs           # node --test (custom ModelSpec via file://)
│   ├── lifecycle.test.mjs          # node --test
│   ├── transcribe.test.mjs         # node --test
│   ├── version.test.mjs            # node --test
│   └── wav_helper.test.mjs         # node --test
├── docs/                           # definition.md, architecture.md, kanbans, archive/
└── target/                         # gitignored
```

Two artifacts ship from this single root:

- **Rust source tarball** (`cargo package`) — `Cargo.toml` declares an explicit `[package].include` allowlist (`/Cargo.toml`, `/Cargo.lock`, `/build.rs`, `/src/**/*.rs`, `/tests/**/*.rs`, `/fixtures/**`, `/LICENSE`, `/LICENSE-THIRD-PARTY`, `/README.md`). Patterns are root-anchored (leading `/`) — without anchoring, gitignore-glob semantics would pull `node_modules/**/LICENSE` into the tarball. Everything else (`package.json`, `index.ts`, `*.node`, etc.) is excluded.
- **npm tarball** — `package.json.files` lists exactly `index.js`, `index.d.ts`, `types.js`, `types.d.ts`, all three `.node` files, `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md`. Rust source is excluded.

Net result: the Rust consumer pulls source via a git dependency. The npm consumer pulls the prebuilt binaries plus a tiny TS/JS surface. Neither artifact ships the other ecosystem's noise. Verification runs locally via `cargo test` / `npm test` and, for the Linux and Windows build legs, via the `Release` workflow (`.github/workflows/release.yml`).

There is **no** workspace, no separate `cadmus-node/` crate, no `npm/` subdirectory, no whisper.cpp submodule, no own `build.rs` driving cmake-rs, no FFI module. CTranslate2's build is owned by `ct2rs`'s build script — it is invoked transitively by `cargo build`.

---

## 3. Data Flow

```
caller bytes (Buffer / &[u8])
        │
        ▼
   symphonia decoder        [Rust]      format detect → decode
        │                                samples at native rate, native channel count
        ▼
   channel downmix          [Rust]      multi-channel → mono via float averaging
        │
        ▼
   rubato resampler         [Rust]      native rate → 16 kHz
        │
        ▼
   Vec<f32> @ 16 kHz mono, range [-1, 1]
        │
        ▼
   ct2rs::Whisper::generate [Rust → C++]   internal: log-mel → encoder → decoder → tokens
        │                                   returns Vec<String> with timestamp tokens
        ▼
   segment parser           [Rust]      Whisper <|t|> tokens → Segment[]
        │
        ▼
   TranscriptResult                     returned synchronously from the core
        │
        ▼
   napi-rs AsyncTask boundary           (Node bridge only)
        │
        ▼
   Promise<TranscriptResult>            (Node caller)
```

If the source is already mono at 16 kHz, the downmix and resample stages are no-ops. ct2rs handles internal batching when many short calls arrive.

---

## 4. Threading Model

The core crate is **synchronous and blocking**. `load_model`, `transcribe`, and `download_model` are plain `fn`, not `async fn`. Inference, model load, and HTTP download all block the calling thread for the duration of the operation. The crate has no executor dependency.

Offload responsibility lives at the boundary:

- **Node bridge.** `src/napi.rs` exposes each operation as a napi-rs `AsyncTask`. The task's `compute` method runs on a libuv worker thread and calls the core's blocking function directly. The JS-visible function returns a `Promise` that resolves when the worker finishes. The Node event loop is never blocked.
- **Rust async callers.** Wrap calls in `tokio::task::spawn_blocking` (or the equivalent for async-std, smol, etc.).
- **Rust sync callers.** Call directly. No wrapping needed.

`ct2rs::ffi::Whisper` is declared `unsafe impl Send + Sync` (`ct2rs/src/sys/whisper.rs` in the pinned 0.9.18). Concurrent `Whisper::generate` calls are therefore safe at the type level, and Cadmus runs them lock-free against `Arc<Whisper>` clones — no external mutex on the inference path.

If a future ct2rs upgrade ever removes the `Send + Sync` impls (concept R1), the fallback is `Arc<Mutex<Whisper>>`, which would serialise calls but preserve the public contract from `definition.md §5` ("Concurrent `transcribe()` on the same context is safe"). A poisoned mutex (in either the present `InferenceHandle` outer guard or a hypothetical inner one) surfaces as `CadmusError::Poisoned`.

---

## 5. Memory Model

`load_model` constructs a `ct2rs::Whisper` via `Whisper::new(model_dir, config)`. ct2rs holds the underlying CTranslate2 model on the native heap; this allocation lives outside V8's GC.

The internal `InferenceHandle` shape (`src/inference.rs`) is:

```rust
pub(crate) struct InferenceHandle {
    inner: Mutex<Option<Arc<Whisper>>>,
    freed: AtomicBool,
}
```

The outer `Mutex<Option<Arc<Whisper>>>` exists solely so that `free()` can atomically swap the owning `Arc` out of the slot. Its critical section spans only the `freed`-check plus an `Arc::clone` (or `take()` in `free()`); it never wraps the call to `Whisper::generate`. The actual inference runs lock-free on the cloned `Arc`. The `freed` `AtomicBool` is the cheap fast-path check that lets new `transcribe` calls reject without touching the mutex when the handle has already been freed.

`CadmusModel` (the public Rust type, `src/api.rs`) wraps an `Arc<InferenceHandle>` so multiple in-flight `AsyncTask` workers on the JS bridge each hold their own clone for the duration of inference.

### free() vs. in-flight transcriptions

`free()` is non-blocking and does not invalidate work already in flight:

1. `free()` checks `freed`; if already set, it is a no-op.
2. Otherwise it sets `freed = true`, takes the mutex briefly to `take()` the inner `Option<Arc<Whisper>>`, and drops its own `Arc`-clone of the inner instance. New `transcribe()` calls observe `freed` and synchronously return `AlreadyFreed`.
3. `AsyncTask`s already running on libuv workers continue to hold their `Arc`-clones and complete normally — their `Promise` resolves with the inferred result.
4. The native `Whisper` is actually dropped (releasing CTranslate2 memory) when the last `Arc` clone — held by the final in-flight task — goes out of scope.

This is reference-counted deferred release. Consequences:

- `free()` returns immediately; the JS event loop is never blocked.
- An in-flight `transcribe()` Promise created **before** `free()` resolves normally even though `free()` was called after the call started. This is a deliberate value-over-abort choice: discarding an in-progress transcription is more wasteful than letting it finish.
- New `transcribe()` calls observed **after** `free()` always fail with `AlreadyFreed`, regardless of whether older tasks are still finishing.
- No use-after-free is possible: the native instance is released only after all references to it are gone.

On the **Rust side**, dropping a `CadmusModel` without calling `free()` is fine — `Drop` runs deterministically at scope exit and releases the inner instance once all `Arc`-clones are gone. The explicit `free()` is offered for parity with the JS API.

On the **JS side**, `free()` is mandatory: V8's GC is non-deterministic, and we do not register a finalizer. A `CadmusModel` that becomes unreachable in JS without `free()` having been called leaks the native instance for the lifetime of the process. Adding a finalizer is tracked in `docs/backlog.kanban.md`.

The contract is verified by three Rust unit tests in `src/inference.rs` (`transcribe_after_free_returns_already_freed`, `free_during_inflight_completes_normally`, `concurrent_transcribe_succeeds`) and re-verified across the napi/AsyncTask boundary by `tests/lifecycle.test.mjs`.

---

## 6. Build Pipeline

`cargo build --features napi` triggers, transitively:

1. `cadmus` depends on `ct2rs` with the `whisper` feature plus the platform-conditional CPU-only feature subset (see §7).
2. `ct2rs`'s build script (`build.rs`) configures and builds CTranslate2 via CMake, links the chosen BLAS backend statically (or against Apple Accelerate on macOS arm64), and produces a static library that ct2rs links into the Rust artifact.
3. `cadmus` is compiled against ct2rs.
4. With `--features napi`, the napi bridge in `src/napi.rs` is compiled in and `build.rs` invokes `napi_build::setup()` (gated on `CARGO_FEATURE_NAPI`).

`cargo build` (without `--features napi`) produces only the rlib and never invokes napi-rs.

The npm artifact is produced by `napi build --release --platform --no-js --dts napi-binding.d.ts --features napi`, which emits a single platform-specific `cadmus.<platform>.node` and a TypeScript declaration file (`napi-binding.d.ts`) for the auto-generated napi surface. The JS dispatcher is hand-written in `index.ts` (loaded via `createRequire`, dispatching on `process.platform` + `process.arch`); `tsc` emits `index.js`/`index.d.ts` and `types.js`/`types.d.ts` to the repository root.

CMake and a C++ toolchain are required at build time on the developer's machine. Consumers of the prebuilt npm binaries do not need them.

### Release Pipeline

The `Release` workflow (`.github/workflows/release.yml`) is the publish path, triggered by `npm run release` / `release:minor` / `release:major`.

`scripts/release.mjs` drives the local half before the workflow fires. It enforces three pre-flight invariants (clean working tree, on `main`, in sync with `origin/main`), then runs `napi build --release` to produce `cadmus.darwin-arm64.node` on the Product Owner's Apple-Silicon Mac, commits the binary if it changed, and pushes to `main`. Only then does it call `gh workflow run release.yml`. The macOS build is intentionally local to avoid the `macos-latest` 10× billing multiplier on GitHub-hosted runners.

The workflow builds only the two remaining legs: `cadmus.linux-x64-gnu.node` (Ubuntu runner) and `cadmus.win32-x64-msvc.node` (Windows runner). After both build legs complete, the `publish` job checks out the latest `main` (which already contains the locally-pushed `darwin-arm64` binary), bumps the npm version, commits all three binaries together with the version bump, tags, and publishes to npm with provenance.

---

## 7. Platform Targets

| Rust triple | npm package suffix | Build host | ct2rs backend features |
|---|---|---|---|
| `aarch64-apple-darwin` | `-darwin-arm64` | developer's macOS (Apple Silicon), via `scripts/release.mjs` | `whisper`, `accelerate`, `ruy` |
| `x86_64-unknown-linux-gnu` | `-linux-x64-gnu` | GitHub-hosted runner (`ubuntu-latest`) | `whisper`, `mkl`, `dnnl`, `openmp-runtime-comp` |
| `x86_64-pc-windows-msvc` | `-win32-x64-msvc` | GitHub-hosted runner (`windows-latest`) | `whisper`, `dnnl` |

These feature sets are a **CPU-only subset** of what ct2rs's per-platform default features include. Cadmus disables `default-features` on the `ct2rs` dependency and enables only the features listed above. ct2rs's own defaults additionally include `cuda`, `cudnn`, and `cuda-dynamic-loading`; those are deliberately excluded to honour the v1 "no CUDA, no GPU" promise from [definition.md §3](definition.md). Re-introducing GPU features is a deliberate post-v1 decision, not an accidental result of accepting upstream defaults.

The Cargo manifest expresses this with per-target dependency syntax (Pattern B from `docs/archive/PLAN_skeleton.md`):

```toml
[target.'cfg(target_os = "macos")'.dependencies]
ct2rs = { version = "=0.9.18", default-features = false, features = ["whisper", "accelerate", "ruy"] }

[target.'cfg(target_os = "linux")'.dependencies]
ct2rs = { version = "=0.9.18", default-features = false, features = ["whisper", "mkl", "dnnl", "openmp-runtime-comp"] }

[target.'cfg(target_os = "windows")'.dependencies]
ct2rs = { version = "=0.9.18", default-features = false, features = ["whisper", "dnnl"] }
```

Linux-arm64 and macOS-x64 are deferred — see `docs/backlog.kanban.md`.

---

## 8. Test Strategy

### 8.1 The Fixtures

`fixtures/eins-zwei-drei.{mp3,wav,flac,webm,m4a}` — ≈ 2.9 s of synthesized German numerals ("eins, zwei, drei, vier, fünf") in five containers (MP3, WAV PCM-16, FLAC, WebM/Opus, MP4/AAC-LC) at three distinct sample rates (22 050 Hz from MP3; 44 100 Hz shared by WAV and m4a; 48 000 Hz shared by FLAC and webm), all derived from the same MP3 master via ffmpeg so cross-format decoded length agrees within ~2 048 samples. The webm and m4a fixtures match the two browser `MediaRecorder` defaults (Chromium/Firefox emit WebM/Opus; Safari emits MP4/AAC). The three rates ensure rubato's resampler is exercised on every test run regardless of which fixture is loaded. Checked into the repository.

The end-to-end smoke test downloads the float16 CT2 `tiny` model from `ctranslate2-4you/whisper-tiny-ct2-float16` (~75 MB), transcribes the MP3 fixture, and asserts the result contains the expected 1/2/3 markers in either spoken (eins/zwei/drei) or digit form. This exercises symphonia decoding, downmix, rubato resampling, ct2rs's mel + tokenizer + inference path, our segment parser, and (in the Node leg) napi-rs marshalling.

### 8.2 Layers

**Rust unit tests** (`src/**/*.rs`, run by `cargo test [--features napi]`):
- `decode`: mono passthrough, stereo cancellation, MP3/WAV/FLAC/WebM-Opus/MP4-AAC fixtures, 48 k → 16 k resample, fixture length consistency across all five formats, corrupt-audio decode error. AAC-LC channel count is inferred from the first decoded packet's `AudioSpec` because symphonia's `isomp4` demuxer leaves `CodecParameters.channels = None`.
- `opus`: OpusHead parsing (pre-skip + channels extracted at the right offsets), mapping-family-non-zero rejected with `Decode`, pre-skip helper drops exactly `pre_skip * channels` samples.
- `inference`: segment-token parser (control tokens, malformed tokens, no-timestamps, multi-chunk, multi-segment, UTF-8), language-detection-from-chunks helpers, end-to-end `eins_zwei_drei` (downloads tiny via `storage`, decodes, infers, asserts), `eins_zwei_drei_via_webm` (same path through the WebM/Opus fixture, proves Opus reaches ct2rs intact), three D4 lifecycle tests (`transcribe_after_free`, `free_during_inflight`, `concurrent_transcribe`).
- `storage`: `download_tiny_smoke`, `ensure_present_distinguishes_states`, cancel-before-call, cancel-mid-stream against a local mock server, progress callback against a local mock server, `percent_decode_basic`, `file_url_to_path_unix` (or `_windows_drive_letter` under `cfg(windows)`), `fetch_one_file_url_copies_local_file`.
- `napi` (only with `--features napi`): error-code coverage, `ModelInfo`/`family` round-trip.
- `version_returns_three_string_fields`.

**Rust integration tests** (`tests/public_api.rs`, run by `cargo test` *without* `--features napi`): exercise the public Rust API surface end-to-end — `Cadmus::new` cache creation and IO failure, the 6 default `ModelSpec` entries returned by `default_models()`, `UnknownModel` against `download_model` / `load_model` / `find_model`, empty-`models` config validity, tiny round-trip via the `Cadmus` handle, one-shot `transcribe(audio, &Path, opts)`, `language == None` accepted-deviation assertion. The file is gated `#![cfg(not(feature = "napi"))]` because integration tests link against the rlib, and the rlib compiled with `--features napi` references N-API runtime symbols that only exist inside Node's process — those would fail to resolve in a standalone test binary. Two Rust test modes therefore exist and both are part of release verification:

- `cargo test --features napi` — runs the unit tests inside `src/` (37 at v2.0.0). The integration file resolves to zero tests.
- `cargo test` (no features) — runs the unit tests (35 at v2.0.0) *and* the integration tests in `tests/public_api.rs` (8 at v2.0.0) against the public Rust surface.

The Release Runbook (`docs/archive/CONCEPT_v1_buildout.md`) treats both as required before publishing.

**Node.js integration tests** (`tests/*.test.mjs`, run by `npm test` → `node --test --test-concurrency=1`):
- `version.test.mjs` — `version()` returns three string fields.
- `wav_helper.test.mjs` — `tests/_helpers/wav.mjs::padWavWithSilence` produces a transcribable WAV.
- `catalog.test.mjs` — `defaultModels()` returns 6 multilingual whisper entries with the expected names and URLs; `listAvailableModels()` mirrors that list when constructed with `models: defaultModels()`; populated metadata; `findModel('nonexistent')` returns `null`; `loadModel({ name: 'nonexistent' })` rejects with `code === 'UnknownModel'`; `loadModel({ name, path })` and `loadModel({})` reject with `code === 'InvalidArgument'`; empty-`models` config yields an empty list.
- `file_url.test.mjs` — registers a custom `ModelSpec` whose files use `file://` URLs (including a percent-encoded path) and asserts `downloadModel` copies the source byte-for-byte into the cache.
- `transcribe.test.mjs` — handle path: `loadModel({ name: 'tiny' }).transcribe(mp3, { language: 'de' })` returns segments with the eins/zwei/drei markers, then `model.free()` plus a fresh `transcribe` rejects with `code === 'AlreadyFreed'`. One-shot path: `transcribe(mp3, modelPath, { language: 'de' })` returns the same content.
- `lifecycle.test.mjs` — three lifecycle invariants across the napi/AsyncTask boundary: free-after-free idempotency, free-during-inflight (synthesised long WAV, in-flight Promise resolves while next call rejects with `AlreadyFreed`), concurrent `Promise.all([transcribe, transcribe])`.
- `download.test.mjs` — happy-path `downloadModel('tiny', { onProgress })` against a fresh `mkdtempSync` cache: callback fires with monotonic `received` and constant `total`; `findModel('tiny')` after returns the same directory.

The Rust D4 invariants (free-during-inflight, concurrent transcribe) are also covered directly in `src/inference.rs` unit tests; the JS-side variants verify napi marshalling preserves them across the libuv boundary. Vitest is not used.

### 8.3 Explicitly Not Tested

Word error rate. Model accuracy is CTranslate2's and Whisper's domain. Cadmus tests that the pipeline delivers samples to ct2rs and surfaces results correctly.

---

## 9. Public Surface — Concrete Signatures

The conceptual surface is defined in [definition.md §4](definition.md). This section binds those concepts to language-specific signatures.

### 9.1 Rust crate

```rust
use cadmus::{
    default_models, transcribe, version,
    Cadmus, CadmusConfig, CadmusModel,
    FileSpec, ModelRef, ModelInfo, ModelFamily, ModelSpec,
    LoadModelOptions, TranscribeOptions, DownloadModelOptions,
    TranscriptResult, Segment,
    CadmusError, ComputeType, Version,
};
use std::path::PathBuf;

// Construct the handle once with explicit cache + catalog config.
let cadmus = Cadmus::new(CadmusConfig {
    model_cache: PathBuf::from("/var/cache/myapp/whisper"),
    models: cadmus::default_models(),
})?;

// Catalog inspection (returns what the consumer configured).
let models: Vec<ModelInfo> = cadmus.list_available_models();
if let Some(_t) = models.iter().find(|m| m.name == "base" && !m.cached) {
    cadmus.download_model("base", DownloadModelOptions::default())?;
}

// Resolve and load.
let model: CadmusModel = cadmus.load_model(
    ModelRef::Name("base"),
    LoadModelOptions::default(),
)?;
let result: TranscriptResult = model.transcribe(&audio_bytes, TranscribeOptions::default())?;
model.free();   // optional in Rust; Drop also works.

// One-shot — does not need the handle, takes an explicit path (not a ModelRef).
let result = cadmus::transcribe(
    &audio_bytes,
    std::path::Path::new("/abs/path/to/model"),
    TranscribeOptions::default(),
)?;

// Free-standing.
let v: Version = cadmus::version();
```

All fallible operations return `Result<T, CadmusError>`. No `async`, no `.await` — see §4.

#### Option structs

```rust
#[derive(Default, Clone, Copy)]
pub enum ComputeType {
    #[default] Auto,    // ct2rs chooses based on model
    Int8,
    Int8Float16,
    Float16,
    Float32,
}

#[derive(Default)]
pub struct LoadModelOptions {
    pub threads:      Option<u32>,    // None → logical CPU count
    pub compute_type: ComputeType,    // default: Auto
}

#[derive(Default)]
pub struct TranscribeOptions {
    pub language:  Option<String>,    // BCP-47; None → ct2rs internal detection
    pub beam_size: Option<u32>,       // None → ct2rs default
    // `threads` intentionally absent — accepted deviation in docs/bug.kanban.md
}

#[derive(Default)]
pub struct DownloadModelOptions {
    pub on_progress: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,  // (received, total)
    pub cancel:      Option<Arc<AtomicBool>>,                       // polled in the download loop
}

pub struct Version {
    pub cadmus:      String,
    pub ct2rs:       String,
    pub ctranslate2: String,
}

pub enum ModelRef<'a> {
    Name(&'a str),
    Path(&'a std::path::Path),
}
// Plus From<&str>, From<String>, From<&Path>, From<PathBuf> for ergonomic call sites.

pub struct CadmusConfig {
    pub model_cache: std::path::PathBuf,
    pub models:      Vec<ModelSpec>,    // pass `default_models()` for the 6 built-in defaults
}

#[derive(Clone)]
pub struct ModelSpec {
    pub name:         String,
    pub description:  String,
    pub size_bytes:   u64,
    pub family:       ModelFamily,
    pub multilingual: bool,
    pub files:        Vec<FileSpec>,
}

#[derive(Clone)]
pub struct FileSpec {
    pub filename: String,
    pub url:      String,   // http(s):// or file:// (percent-decoded)
}

pub fn default_models() -> Vec<ModelSpec>;
```

`DownloadModelOptions::cancel` is cooperative: the download loop checks the flag between chunks. There is no preemptive cancellation. The JS side's `AbortSignal` maps to setting this flag.

### 9.2 npm package (`@ai-inquisitor/cadmus`)

```typescript
import {
  Cadmus, CadmusModel, defaultModels, transcribe, version,
  CadmusConfig, CadmusError, ComputeType,
  DownloadModelOptions, LoadModelOptions, TranscribeOptions,
  FileSpec, ModelFamily, ModelInfo, ModelRef, ModelSpec,
  Segment, TranscriptResult, Version,
} from '@ai-inquisitor/cadmus';

// Cadmus is exported as a constructor. `new Cadmus(config)` is
// synchronous and may throw a `CadmusError` (e.g. `code: 'Io'` if the
// cache directory cannot be created).
declare const Cadmus: new (config: CadmusConfig) => CadmusInstance;

interface CadmusInstance {
  listAvailableModels(): ModelInfo[]
  findModel(name: string): string | null
  downloadModel(name: string, options?: DownloadModelOptions): Promise<string>
  loadModel(modelRef: ModelRef, options?: LoadModelOptions): Promise<CadmusModel>
}

interface CadmusConfig {
  modelCache: string             // explicit cache directory
  models: ModelSpec[]            // explicit catalog; pass `defaultModels()` for the 6 built-in defaults
}

interface ModelSpec {
  name: string
  description: string
  sizeBytes: number
  family: ModelFamily
  multilingual: boolean
  files: FileSpec[]
}

interface FileSpec {
  filename: string
  url: string                    // https://, http://, or file:// (percent-decoded)
}

function defaultModels(): ModelSpec[]

type ModelRef =                   // discriminated union
  | { name: string }              // resolved against the configured cache
  | { path: string }              // direct path to a model directory

interface CadmusModel {
  transcribe(audio: Buffer, options?: TranscribeOptions): Promise<TranscriptResult>
  free(): void
}

function transcribe(
  audio: Buffer,
  modelPath: string,              // path string, not ModelRef
  options?: TranscribeOptions,
): Promise<TranscriptResult>

function version(): Version

interface Version {
  cadmus: string
  ct2rs: string
  ctranslate2: string
}

interface LoadModelOptions {
  threads?: number                 // default: ct2rs picks
  computeType?: ComputeType        // default: 'auto'
}
type ComputeType = 'auto' | 'int8' | 'int8_float16' | 'float16' | 'float32'

interface TranscribeOptions {
  language?: string                // BCP-47; absent → ct2rs detection
  beamSize?: number
  // `threads` intentionally absent — accepted deviation in docs/bug.kanban.md
}

interface DownloadModelOptions {
  onProgress?: (received: number, total: number) => void
  signal?: AbortSignal
}

interface TranscriptResult {
  text: string
  language: string
  segments: Segment[]
}

interface Segment {
  start: number                    // seconds
  end: number
  text: string
}

type ModelFamily = 'whisper' | 'distil_whisper'

interface ModelInfo {
  name: string
  description: string
  sizeBytes: number
  family: ModelFamily
  multilingual: boolean
  cached: boolean                  // computed at call time
  files: string[]                  // filenames only; URLs live on the underlying ModelSpec
}

// Type-only narrowing — runtime is a plain Error with `code` set.
interface CadmusError extends Error {
  code: 'Load' | 'Decode' | 'Resample' | 'Inference'
      | 'Poisoned' | 'AlreadyFreed' | 'UnknownModel'
      | 'Download' | 'Io' | 'InvalidArgument'
}
```

The JS error contract carries the variant name in `err.code`. Synchronous throws (`InvalidArgument`, `AlreadyFreed`, `UnknownModel` via `loadModel({ name })`) propagate the typed code via `JsError<String>::throw_into`; AsyncTask rejections (`Load`, `Decode`, `Resample`, `Inference`, `Download`, `Io`, `Poisoned`) propagate the typed code by building the JS Error directly with `napi_create_error` in `Task::reject` and packing it into `napi::Error::maybe_raw`.

**Layout:** `index.ts` does platform dispatch (`darwin-arm64` and `linux-x64-gnu`) and re-exports the napi-rs class verbatim. Hand-written types live in `types.ts`. The auto-generated `napi-binding.d.ts` is internal; consumers see `index.d.ts` (which re-exports types from `./types.js`).
