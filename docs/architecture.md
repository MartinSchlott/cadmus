# Cadmus — Architecture

The How. Target vision; no inventory yet (no code exists).

For *what* Cadmus is and the contract it exposes, see [definition.md](definition.md).

---

## 1. Technology Stack

| Layer | Choice | Rationale |
|---|---|---|
| Inference engine | CTranslate2 (C++17) | Fast, memory-efficient transformer inference; int8 quantisation; the engine behind faster-whisper |
| Engine binding | `ct2rs` (Rust) | Mature MIT-licensed Rust wrapper around CTranslate2 with a `whisper` feature that ships mel-spectrogram and tokenizer support. Bundles CTranslate2 statically — we do not drive its build ourselves |
| BLAS backend (Linux/Windows) | Intel oneMKL via `intel-onemkl-prebuild` | Statically linked, no system install. Fastest CPU path on x86_64 |
| BLAS backend (macOS arm64) | Apple Accelerate + ruy | Apple Accelerate ships with macOS — no extra install. ruy provides ARM matmul primitives |
| Audio decoding | symphonia (Rust) | Pure Rust decoder/demuxer with magic-byte format detection. Decodes to interleaved float samples at the source's native rate and channel count. Eliminates FFmpeg/PyAV |
| Resampling | rubato (Rust) | Sinc-based resampler from arbitrary input rate to Whisper's required 16 kHz. Pure Rust |
| Channel downmix | in-house (Rust) | Stereo/multi-channel → mono via float averaging. Trivial enough to live in `decode.rs` rather than pulling another crate |
| Node.js bridge | napi-rs | Single `.node` artifact per platform; `AsyncTask` pattern offloads blocking inference to the libuv threadpool |

### Why CTranslate2, not whisper.cpp

CTranslate2 is faster on CPU (typical 1.5–3× at comparable quantisation) and produces smaller models (int8 ~30 % smaller than GGML q5_1). It is the inference engine behind the production-grade `faster-whisper` ecosystem. The downside — additional BLAS dependency — is mitigated by static linking on x86 and by Apple Accelerate on arm64 macOS.

### Why ct2rs, not a hand-written FFI layer

`ct2rs` is MIT-licensed and provides a `Whisper` struct whose `generate(samples: &[f32], language, timestamp, options) -> Result<Vec<String>>` method consumes 16 kHz mono float samples directly and returns text — mel-spectrogram, tokenizer, and decoding loop are internal. Building this surface ourselves would mean:

- writing a C bridge over CTranslate2's C++-only API,
- implementing the Whisper log-mel-spectrogram (FFT + mel filterbank),
- integrating a Whisper tokenizer,
- writing the encoder/decoder beam-search loop,
- handling timestamp token parsing.

`ct2rs` provides all of this. We add only the Cadmus-specific surface: audio pipeline, model management, error mapping, segment construction from timestamp tokens.

### Why symphonia, not FFmpeg bindings

Pure Rust eliminates the dominant deployment failure: a missing or version-mismatched system library. Magic-byte format detection means the API never asks the caller "what format is this?". The fixture-based test strategy (decoding a checked-in MP3 in CI) is only viable because the decoder is itself part of the build artifact.

symphonia covers decode and format detection only. Resampling and channel downmix are explicit separate stages — `rubato` for the former, in-house averaging for the latter.

### Why a sync core, not async

The `cadmus` crate exposes blocking functions. It does not depend on tokio, async-std, or any other executor.

The reason is separation of concerns: blocking inference is the truth, and offload is a deployment decision that belongs to the caller. The Node bridge offloads to the libuv threadpool via napi-rs `AsyncTask`. Async Rust callers wrap calls in `tokio::task::spawn_blocking` or their runtime's equivalent. Pulling a runtime choice into the core would force every consumer to pay for a decision they may not want.

---

## 2. Repository Structure

A Cargo workspace. Two Rust crates, one npm package, one fixture.

```
/
├── Cargo.toml                    # workspace: members = ["cadmus", "cadmus-node"]
│
├── cadmus/                       # Rust crate → crates.io
│   ├── Cargo.toml                # depends on ct2rs (with whisper feature),
│   │                             #   symphonia, rubato, reqwest, etc.
│   └── src/
│       ├── lib.rs                # public API re-exports
│       ├── model.rs              # CadmusModel: holds ct2rs::Whisper instance
│       ├── transcribe.rs         # decode → resample → ct2rs::generate → segments
│       ├── decode.rs             # symphonia + rubato + downmix → Vec<f32> @ 16 kHz mono
│       ├── segments.rs           # Whisper timestamp-token parser → Segment[]
│       ├── error.rs              # CadmusError
│       └── helpers/
│           ├── catalogue.rs      # static model catalogue
│           ├── download.rs       # HTTP fetch with progress + cooperative cancel
│           └── find.rs           # filesystem search (model directories)
│
├── cadmus-node/                  # napi-rs bridge → npm
│   ├── Cargo.toml                # depends on cadmus, napi, napi-derive
│   └── src/
│       └── lib.rs                # translation only; no logic.
│                                 # Each operation is a napi AsyncTask wrapping the sync core.
│
├── npm/                          # consumer-facing TS package
│   ├── index.ts
│   ├── types.ts
│   └── package.json              # @ai-inquisitor/cadmus
│
├── fixtures/
│   ├── eins-zwei-drei.mp3        # ≈ 2.9 s synthesized German numerals @ 22 050 Hz
│   ├── eins-zwei-drei.wav        # same recording, PCM-16 @ 44 100 Hz
│   └── eins-zwei-drei.flac       # same recording, FLAC @ 48 000 Hz
│
├── tests/
│   ├── rust/                     # uses cadmus crate directly
│   └── node/                     # uses npm package via Vitest
│
└── .github/workflows/
    ├── build.yml                 # build + test on linux-x64, darwin-arm64, win32-x64
    └── publish.yml               # crates.io + npm on tag
```

There is **no** whisper.cpp submodule, no own `build.rs` driving cmake-rs, no FFI module. CTranslate2's build is owned by `ct2rs`'s build script — it is invoked transitively by `cargo build`.

The `cadmus-node` crate exists because napi-rs procedural macros conflict with publishing the `cadmus` crate as a clean library. Separation keeps the crates.io artifact napi-free.

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

- **Node bridge.** `cadmus-node` exposes each operation as a napi-rs `AsyncTask`. The task's `compute` method runs on a libuv worker thread and calls the core's blocking function directly. The JS-visible function returns a `Promise` that resolves when the worker finishes. The Node event loop is never blocked.
- **Rust async callers.** Wrap calls in `tokio::task::spawn_blocking` (or the equivalent for async-std, smol, etc.).
- **Rust sync callers.** Call directly. No wrapping needed.

`ct2rs::Whisper` exposes batch and replica counters (`num_active_batches`, `num_replicas`), which suggests internal concurrency is supported. Whether `ct2rs::Whisper` is `Send + Sync` and tolerates concurrent `generate` calls without external synchronisation is **verified during the first implementation plan**. The plan must inspect ct2rs's source or run a contention test before deciding. Two valid outcomes:

- ct2rs handles concurrency internally → `CadmusModel` holds the instance directly, multiple `transcribe()` calls run in parallel
- ct2rs requires external serialisation → `CadmusModel` wraps the instance in a `std::sync::Mutex`, calls serialise

Either way, the public contract from definition.md §5 ("Concurrent `transcribe()` on the same context is safe") holds. A poisoned mutex (if used) surfaces as `CadmusError::Poisoned`.

---

## 5. Memory Model

`load_model` constructs a `ct2rs::Whisper` via `Whisper::new(model_dir, config)`. ct2rs holds the underlying CTranslate2 model on the native heap; this allocation lives outside V8's GC.

`CadmusModel` holds the inner instance as `Arc<Whisper>` plus an `AtomicBool` "freed" sentinel (exact concrete type — `Arc<Whisper>`, `Arc<RwLock<Option<Whisper>>>`, etc. — is chosen during the first implementation plan and depends on whether ct2rs's `Whisper` is `Send + Sync`; see §4). The `Arc` lets each in-flight `AsyncTask` on a libuv worker thread hold its own clone for the duration of inference. The sentinel guards new entries to the API.

### free() vs. in-flight transcriptions

`free()` is non-blocking and does not invalidate work already in flight:

1. `free()` checks the sentinel; if already set, it is a no-op.
2. Otherwise it sets the sentinel atomically and drops its own `Arc`-clone of the inner instance. New `transcribe()` calls observe the sentinel and synchronously throw `AlreadyFreed`.
3. AsyncTasks already running on libuv workers continue to hold their `Arc`-clones and complete normally — their `Promise` resolves with the inferred result.
4. The native `Whisper` is actually dropped (releasing CTranslate2 memory) when the last `Arc` clone — held by the final in-flight task — goes out of scope.

This is reference-counted deferred release. Consequences:

- `free()` returns immediately; the JS event loop is never blocked.
- An in-flight `transcribe()` Promise created **before** `free()` resolves normally even though `free()` was called after the call started. This is a deliberate value-over-abort choice: discarding an in-progress transcription is more wasteful than letting it finish.
- New `transcribe()` calls observed **after** `free()` always fail with `AlreadyFreed`, regardless of whether older tasks are still finishing.
- No use-after-free is possible: the native instance is released only after all references to it are gone.

On the **Rust side**, dropping a `CadmusModel` without calling `free()` is fine — `Drop` runs deterministically at scope exit and releases the inner instance once all `Arc`-clones are gone. The explicit `free()` is offered for parity with the JS API.

On the **JS side**, `free()` is mandatory: V8's GC is non-deterministic, and we do not register a finalizer. A `CadmusModel` that becomes unreachable in JS without `free()` having been called leaks the native instance for the lifetime of the process.

---

## 6. Build Pipeline

`cargo build -p cadmus-node` triggers, transitively:

1. `cadmus-node` depends on `cadmus`.
2. `cadmus` depends on `ct2rs` with the `whisper` feature and a platform-appropriate backend feature set.
3. `ct2rs`'s build script (`build.rs`) configures and builds CTranslate2 via CMake, links the chosen BLAS backend statically (or against Apple Accelerate on macOS arm64), and produces a static library that ct2rs links into the Rust artifact.
4. `cadmus` is compiled against ct2rs.
5. `cadmus-node` uses napi-rs to produce a single platform-specific `cadmus.node` binary.

The `cadmus` crate is also buildable standalone (`cargo build -p cadmus`) for Rust-only consumers — it produces an `rlib` and never invokes napi-rs.

No dynamic linking to system libraries beyond what Node.js itself requires, plus Apple Accelerate on macOS (system framework, always present).

CMake is required at build time on the developer's machine and on CI runners. Consumers of the prebuilt npm binaries do not need it.

---

## 7. Platform Targets

| Rust triple | npm package suffix | CI runner | ct2rs backend features |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | `-linux-x64-gnu` | `ubuntu-latest` | `mkl`, `dnnl`, `openmp-runtime-comp` |
| `aarch64-apple-darwin` | `-darwin-arm64` | `macos-latest` | `accelerate`, `ruy` |
| `x86_64-pc-windows-msvc` | `-win32-x64-msvc` | `windows-latest` | `mkl`, `dnnl`, `openmp-runtime-intel` |

These feature sets are a **CPU-only subset** of what ct2rs's per-platform default features include. Cadmus disables `default-features` on the `ct2rs` dependency and enables only the features listed above. ct2rs's actual defaults additionally include `cuda`, `cudnn`, and `cuda-dynamic-loading` on Linux and Windows; those are deliberately excluded to honour the v1 "no CUDA, no GPU" promise from [definition.md §3](definition.md). Re-introducing GPU features is a deliberate post-v1 decision, not an accidental result of accepting upstream defaults.

### Windows-specific notes

CMake and Visual Studio Build Tools 2022 are present on `windows-latest` runners; no extra setup. ct2rs's build script handles MSVC CRT linkage and oneMKL static linking — Cadmus does not configure cmake-rs directly.

---

## 8. Test Strategy

### 8.1 The Fixtures

`fixtures/eins-zwei-drei.{mp3,wav,flac}` — ≈ 2.9 s of synthesized German numerals ("eins, zwei, drei, vier, fünf") in three containers at three sample rates (MP3 22 050 Hz, WAV PCM-16 44 100 Hz, FLAC 48 000 Hz), all derived from the same master so cross-format decoded length agrees within ~2 048 samples. The three rates ensure rubato's resampler is exercised on every test run regardless of which fixture is loaded. Checked into the repository.

Every CI run downloads `Systran/faster-whisper-tiny` (the smallest CTranslate2 Whisper model, ~75 MB), transcribes the MP3 fixture, and asserts the result contains the expected words ("eins", "zwei", "drei" survive even if the longer numerals are missed by the tiny model). This is the end-to-end smoke test: it exercises symphonia decoding, downmix, rubato resampling, ct2rs's mel + tokenizer + inference path, our segment parser, and (in the Node leg) napi-rs marshalling on every push.

### 8.2 Layers

**Rust unit tests** (`tests/rust/`):
- `decode`: WAV/MP3/FLAC fixtures → correct sample count and rate after the full decode → downmix → resample chain
- `segments`: synthetic Whisper output strings with `<|t|>` tokens → expected `Segment[]`
- `download`: mock HTTP, verify directory structure written, progress callback invoked, cancellation flag respected

**Node.js integration tests** (`tests/node/`, Vitest):
- `version()` shape
- `listAvailableModels()` non-empty with required fields
- `findModel` finds a temp-dir model directory
- `loadModel` + `transcribe(fixture)` returns text containing "eins"
- `free()` after `free()` throws typed error
- Two concurrent `transcribe()` calls on one context produce correct results without corruption

### 8.3 Explicitly Not Tested

Word error rate. Model accuracy is CTranslate2's and Whisper's domain. Cadmus tests that the pipeline delivers samples to ct2rs and surfaces results correctly.

---

## 9. Public Surface — Concrete Signatures

The conceptual surface is defined in [definition.md §4](definition.md). This section binds those concepts to language-specific signatures.

### 9.1 Rust crate

```rust
use cadmus::{
    load_model, transcribe, find_model, download_model, list_available_models, version,
    CadmusModel, LoadModelOptions, TranscribeOptions, TranscriptResult, Segment,
    ModelInfo, DownloadModelOptions, CadmusError, Version, ComputeType,
};

// Stateful — synchronous, blocking
let model:  CadmusModel      = cadmus::load_model("/path/to/faster-whisper-base", LoadModelOptions::default())?;
let result: TranscriptResult = model.transcribe(&audio_bytes, TranscribeOptions::default())?;
model.free();   // optional in Rust — Drop handles it

// One-shot
let result = cadmus::transcribe(&audio_bytes, "/path/to/faster-whisper-base", TranscribeOptions::default())?;

// Helpers
let models: Vec<ModelInfo> = cadmus::list_available_models();
let path:   String         = cadmus::download_model("base", "/tmp/models", DownloadModelOptions::default())?;
let found:  Option<String> = cadmus::find_model("base", None);
let v:      Version        = cadmus::version();
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
    pub language:  Option<String>,    // BCP-47; None → "auto"
    pub beam_size: Option<u32>,       // None → ct2rs default
    pub threads:   Option<u32>,       // None → use model's default
}

#[derive(Default)]
pub struct DownloadModelOptions {
    pub on_progress: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,  // (received, total)
    pub cancel:      Option<Arc<AtomicBool>>,                       // polled in the download loop
}

pub struct Version {
    pub ctranslate2: String,
    pub ct2rs:       String,
    pub cadmus:      String,
}
```

`DownloadModelOptions::cancel` is cooperative: the download loop checks the flag between chunks. There is no preemptive cancellation. The JS side's `AbortSignal` maps to setting this flag.

### 9.2 npm package (`@ai-inquisitor/cadmus`)

```typescript
function loadModel(modelPath: string, options?: LoadModelOptions): Promise<CadmusModel>

interface LoadModelOptions {
  threads?: number              // default: logical CPU count
  computeType?: 'auto' | 'int8' | 'int8_float16' | 'float16' | 'float32'   // default: 'auto'
}

interface CadmusModel {
  transcribe(audio: Buffer, options?: TranscribeOptions): Promise<TranscriptResult>
  free(): void
}

function transcribe(
  audio: Buffer,
  modelPath: string,
  options?: TranscribeOptions
): Promise<TranscriptResult>

interface TranscribeOptions {
  language?: string             // BCP-47 ('de', 'en', ...) or 'auto'
  beamSize?: number
  threads?: number
}

interface TranscriptResult {
  text:     string
  language: string
  segments: Segment[]
}

interface Segment {
  start: number   // seconds
  end:   number   // seconds
  text:  string
}

function listAvailableModels(): ModelInfo[]

interface ModelInfo {
  name:        string
  sizeBytes:   number
  description: string
  files:       string[]   // expected files inside the model directory
}

function downloadModel(
  name:    string,
  destDir: string,
  options?: DownloadModelOptions
): Promise<string>     // resolves to the absolute model directory path

interface DownloadModelOptions {
  onProgress?: (bytesReceived: number, totalBytes: number) => void
  signal?:     AbortSignal
}

function findModel(name: string, searchPaths?: string[]): string | null

function version(): { ctranslate2: string; ct2rs: string; cadmus: string }
```

**Full export list:** `loadModel`, `CadmusModel`, `LoadModelOptions`, `transcribe`, `TranscribeOptions`, `TranscriptResult`, `Segment`, `listAvailableModels`, `ModelInfo`, `downloadModel`, `DownloadModelOptions`, `findModel`, `version`. Nothing else. Internal Rust types and CTranslate2 types are not re-exported.
