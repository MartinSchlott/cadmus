# Cadmus

**Whisper transcription for Rust and Node.js — CTranslate2 inside, no Python, no FFmpeg, no GPU. One crate, two artifacts.**

Cadmus wraps CTranslate2 (the engine behind faster-whisper) via the `ct2rs` crate, adds a pure-Rust audio pipeline (symphonia + rubato), and ships the result twice: as the `cadmus` crate for Rust callers (a git dependency — not yet on crates.io), and as `@ai-inquisitor/cadmus` on npm for Node and Electron. Same logic, prebuilt `.node` binary, zero JS runtime dependencies.

Local STT in JavaScript usually means a Python sidecar, an FFmpeg dependency chain, or a GPU. Cadmus is none of those. The engine is statically linked, the audio decoder is in the binary, CPU is enough.

```typescript
import { Cadmus } from '@ai-inquisitor/cadmus'

const cadmus = new Cadmus({ modelCache: '/var/cache/myapp/whisper' })
await cadmus.downloadModel('tiny')

const model  = await cadmus.loadModel({ name: 'tiny' })
const result = await model.transcribe(audioBytes, { language: 'de' })
console.log(result.text)
model.free()
```

Same shape in Rust:

```rust
use cadmus::{Cadmus, CadmusConfig, ModelRef, TranscribeOptions};
use std::path::PathBuf;

let cadmus = Cadmus::new(CadmusConfig {
    model_cache: PathBuf::from("/var/cache/myapp/whisper"),
})?;
cadmus.download_model("tiny", Default::default())?;

let model  = cadmus.load_model(ModelRef::Name("tiny"), Default::default())?;
let result = model.transcribe(&audio_bytes, TranscribeOptions {
    language: Some("de".into()),
    ..Default::default()
})?;
println!("{}", result.text);
```

## What it does

**One implementation, two artifacts.** A single Cargo crate at the repository root with `[lib] crate-type = ["cdylib", "lib"]` and a `napi` feature flag. Rust consumers add `cadmus` as a git dependency (not yet on crates.io) and never compile any napi code. Node consumers `npm install @ai-inquisitor/cadmus` and get a prebuilt `.node` binary plus a TypeScript surface — no Rust toolchain, no compiler, no CMake.

**Synchronous Rust core, async Node bridge.** Every operation in the core is a blocking `fn`. The crate has no executor dependency — async Rust callers wrap calls in `tokio::task::spawn_blocking` (or their runtime's equivalent). The Node bridge offloads each call to the libuv threadpool via napi-rs `AsyncTask` and returns a `Promise`. The Node event loop is never blocked.

**Pure-Rust audio pipeline.** symphonia decodes (mp3, wav, flac, mp4/aac, webm) with magic-byte format detection — the caller never specifies the format. WebM/Opus from browser `MediaRecorder` decodes through a pure-Rust libopus wrapper. In-house downmix collapses multi-channel to mono. rubato resamples to Whisper's 16 kHz. No FFmpeg. No system audio libs. The pipeline is part of the build artifact, so the checked-in fixtures round-trip end-to-end with no external tooling.

**17-entry catalog.** Twelve canonical Whisper sizes (`tiny` through `large-v3-turbo`, plus the `.en` English-only variants) and five Distil-Whisper entries. Every entry carries `description`, `size_bytes`, `family`, `multilingual`, `cached`, `repo`, `files`. `cached` is computed at call time: directory exists *and* every catalog file is present with non-zero size — stricter than "directory exists".

**Explicit, required model cache.** `Cadmus::new(CadmusConfig { model_cache })` takes a path. No environment variables. No platform-specific magic paths. No fallback search list. `find_model("tiny")` looks in the configured cache and only there.

**`download_model` with progress and cancel.** Pulls from the matching `Systran/faster-whisper-*` (Whisper) or `Systran/faster-distil-whisper-*` (Distil) HuggingFace repository. `preprocessor_config.json` is sourced from the matching `openai/whisper-*` repo per ct2rs convention. Progress callback delivers monotonic `(received, total)` against the catalog total. Cooperative cancellation via `Arc<AtomicBool>` (Rust) or `AbortSignal` (JS).

**Reference-counted deferred `free()`.** `model.free()` is mandatory on the JS side and optional in Rust (`Drop` runs anyway). `free()` does **not** abort in-flight transcriptions: a `transcribe()` Promise created before `free()` resolves with its result; the underlying CTranslate2 instance is dropped only after the last in-flight call finishes. New `transcribe()` calls after `free()` always throw `AlreadyFreed`. Value-over-abort by design.

**Concurrent `transcribe()` is safe.** `ct2rs::ffi::Whisper` is `Send + Sync`; `Whisper::generate` runs lock-free against `Arc<Whisper>` clones. Two parallel `model.transcribe(audio)` calls both succeed.

**Typed errors with `code` on the JS side.** Rust returns `CadmusError` with discriminated variants. JS gets `Error` instances with `err.code` set to the variant name — `'Load'`, `'Decode'`, `'Resample'`, `'Inference'`, `'Poisoned'`, `'AlreadyFreed'`, `'UnknownModel'`, `'Download'`, `'Io'`, `'InvalidArgument'`. Synchronous throws (`InvalidArgument` for malformed `ModelRef`, `AlreadyFreed`, `UnknownModel` via `loadModel({ name })`) and async-task rejections both carry the typed code.

## Quick taste

```typescript
import { Cadmus, transcribe, version } from '@ai-inquisitor/cadmus'
import { readFileSync } from 'node:fs'

console.log(version()) // { cadmus: '1.0.0', ct2rs: '...', ctranslate2: '...' }

const cadmus = new Cadmus({ modelCache: process.env.CADMUS_CACHE! })

// Catalog inspection
for (const m of cadmus.listAvailableModels()) {
  console.log(`${m.name.padEnd(22)} ${(m.sizeBytes / 1e6).toFixed(0).padStart(5)} MB  cached=${m.cached}`)
}

// Download with progress
await cadmus.downloadModel('base', {
  onProgress: (received, total) => process.stderr.write(`\r${received}/${total}`),
})

// Persistent model, multiple calls, then free
const model  = await cadmus.loadModel({ name: 'base' })
const audio  = readFileSync('meeting.mp3')
const result = await model.transcribe(audio, { language: 'en', beamSize: 5 })

console.log(result.text)
console.log(result.segments.map(s => `[${s.start.toFixed(1)}–${s.end.toFixed(1)}] ${s.text}`).join('\n'))
model.free()

// One-shot — no handle, just a path
const path   = cadmus.findModel('base')!
const oneOff = await transcribe(audio, path, { language: 'en' })
```

```rust
use cadmus::{Cadmus, CadmusConfig, DownloadModelOptions, LoadModelOptions, ModelRef, TranscribeOptions};
use std::sync::{Arc, atomic::AtomicBool};

let cancel = Arc::new(AtomicBool::new(false));

cadmus.download_model("large-v3", DownloadModelOptions {
    on_progress: Some(Box::new(|recv, total| eprintln!("{recv}/{total}"))),
    cancel:      Some(cancel.clone()),
})?;

let model = cadmus.load_model(
    ModelRef::Name("large-v3"),
    LoadModelOptions { threads: Some(8), ..Default::default() },
)?;

let result = model.transcribe(&audio, TranscribeOptions {
    language: None,                          // ct2rs detects internally
    beam_size: Some(5),
})?;
```

## Platforms

Cadmus ships prebuilt `.node` binaries for three platforms, all built and published by the `Release` GitHub Actions workflow:

- **macOS arm64** (`aarch64-apple-darwin`) — Apple Accelerate + ruy
- **Linux x86_64** (`x86_64-unknown-linux-gnu`) — oneMKL + DNNL + compiler OpenMP
- **Windows x86_64** (`x86_64-pc-windows-msvc`) — oneDNN, no OpenMP (self-contained, reduced intra-op parallelism)

GPU, Linux-arm64, and macOS-x64 are deferred and tracked in the [backlog](docs/backlog.kanban.md).

## Build from source

Most consumers don't need this. `npm install @ai-inquisitor/cadmus` ships the prebuilt `.node`; a `cadmus` git dependency ships the rlib source.

Building yourself needs a C++ toolchain, CMake, Rust stable, and Node ≥ 22:

```
cargo build --release --features napi
npm run build
cargo test
cargo test --features napi
npm test
```

The first `cargo build --features napi` triggers ct2rs's CMake build of CTranslate2 plus the platform's BLAS backend (5–25 min on a fresh host). Subsequent builds are warm.

`cargo test` (no features) and `cargo test --features napi` are both part of release verification — the integration tests in `tests/public_api.rs` are gated `#![cfg(not(feature = "napi"))]` because the napi-flavoured rlib references N-API symbols that only resolve inside Node. See [`docs/architecture.md §8`](docs/architecture.md) for the full test layout.

Releases are automated. The `Release` workflow ([`.github/workflows/release.yml`](.github/workflows/release.yml)) builds the `.node` for all three platforms on GitHub-hosted runners, bumps the npm version, commits the binaries, tags, and runs `npm publish --provenance`. Trigger it from the Actions UI or with `npm run release` / `npm run release:minor` / `npm run release:major`. The original manual six-step runbook is kept for reference in [`docs/archive/CONCEPT_v1_buildout.md`](docs/archive/CONCEPT_v1_buildout.md).

## Out of Scope (v1)

GPU inference. Streaming / real-time partials. Word-level timestamps. Model integrity checksums. V8 finalizers (JS-side `free()` is mandatory). HTTP Range / resumable downloads. Word error rate guarantees — that's CTranslate2 and Whisper, not Cadmus.

All deferred work is tracked in [`docs/backlog.kanban.md`](docs/backlog.kanban.md).

## License

MIT (`LICENSE`). symphonia is MPL-2.0 — file-scoped copyleft, attribution in [`LICENSE-THIRD-PARTY`](LICENSE-THIRD-PARTY). Does not infect Cadmus's own code.

---

*A `free()` that aborts in-flight work is faster to write and wrong by default. The reference-counted version is twenty more lines and the only one that does not throw away results the caller already paid for.* — Claude Opus 4.7

*Kannst Du mich jetzt hören?* — AI-Inquisitor

---

## LLM Reference

Cadmus: a Whisper-transcription library shipped as a single Cargo crate (`cadmus`, consumed as a git dependency — not yet on crates.io) and a napi-rs Node binding (`@ai-inquisitor/cadmus`, npm) — same source, `napi` cargo feature flag toggles between rlib (Rust consumers) and cdylib (`.node` for Node consumers). Inference via `ct2rs 0.9.18` (which bundles CTranslate2 statically); audio pipeline pure-Rust (symphonia + in-house downmix + rubato). v1.0.0, MIT.

**Architecture — why these choices:** Synchronous Rust core, async at the boundary. The crate has no executor dependency. Async Rust callers wrap in `tokio::task::spawn_blocking`; the Node bridge wraps each call in `napi::AsyncTask` and runs `compute()` on a libuv worker. This keeps runtime choice with the caller and the Node event loop unblocked. Single Cargo crate (`[lib] crate-type = ["cdylib", "lib"]`, `napi = ["dep:napi", "dep:napi-derive"]`) instead of a workspace — Rust consumers never compile any napi code; the same source produces both artifacts. CTranslate2 + Whisper rather than whisper.cpp because faster on CPU, smaller int8 models, production-tested via faster-whisper. ct2rs rather than hand-written FFI because it already wraps mel-spectrogram + tokenizer + decoder loop.

**Memory model — `InferenceHandle` (`src/inference.rs`):** `pub(crate) struct InferenceHandle { inner: Mutex<Option<Arc<Whisper>>>, freed: AtomicBool }`. The outer `Mutex` exists solely so `free()` can swap the owning `Arc` out atomically; its critical section spans only `freed`-check + `Arc::clone` (or `take()` in `free()`), never the call to `Whisper::generate`. Inference runs lock-free on the cloned `Arc`. `freed` is the cheap fast-path that lets new calls reject without touching the mutex. `free()` is non-blocking; in-flight `transcribe()` calls hold their own `Arc`-clones and complete normally; the native `Whisper` is dropped when the last `Arc` clone goes out of scope (reference-counted deferred release). Concurrent `Whisper::generate` is safe because `ct2rs::ffi::Whisper` is `unsafe impl Send + Sync` (`ct2rs/src/sys/whisper.rs` in the pinned 0.9.18). Verified by three Rust unit tests in `src/inference.rs` (`transcribe_after_free_returns_already_freed`, `free_during_inflight_completes_normally`, `concurrent_transcribe_succeeds`) and re-verified across the napi/AsyncTask boundary by `tests/lifecycle.test.mjs`.

**Audio pipeline (`src/decode.rs`):** Caller bytes → symphonia (decode + magic-byte format detection — caller never specifies format) → in-house downmix (multi-channel → mono via float averaging) → rubato (sinc resampler from native rate to 16 kHz) → `Vec<f32>` at 16 kHz mono in `[-1, 1]`. If the source is already 16 kHz mono, downmix and resample are no-ops. Corrupt input surfaces as `CadmusError::Decode`; pathological sample rates as `CadmusError::Resample`. Five checked-in fixtures (`fixtures/eins-zwei-drei.{mp3,wav,flac,webm,m4a}`) exercise three sample rates (22 050, 44 100, 48 000 Hz) so resample is hit on every test run.

**Catalog (`src/catalog.rs`):** Static, hard-coded — no network, no JSON file, no runtime updates. Catalog updates ship with Cadmus releases. 17 entries: 12 canonical Whisper (`tiny`, `tiny.en`, `base`, `base.en`, `small`, `small.en`, `medium`, `medium.en`, `large-v1`, `large-v2`, `large-v3`, `large-v3-turbo`) + 5 Distil-Whisper (`distil-small.en`, `distil-medium.en`, `distil-large-v2`, `distil-large-v3`, `distil-large-v3.5`). `ModelInfo`: `name`, `description`, `size_bytes`, `family` (`Whisper` | `DistilWhisper`), `multilingual` (`false` for `.en` and Distil-EN-only), `cached` (computed at call time: directory exists *and* every catalog file present with size > 0), `repo` (e.g. `Systran/faster-whisper-base`), `files`. Internal `FileSpec` carries per-file `(repo, file)` pairs because CT2-converted Faster-Whisper repos do not ship `preprocessor_config.json` — that file is sourced per-model from the matching `openai/whisper-*` repo, the canonical workflow per ct2rs's `examples/whisper.rs`.

**Storage (`src/storage.rs`):** `ureq` (rustls + platform-verifier — pure-Rust TLS, no system OpenSSL) downloads each `FileSpec` into the configured `model_cache`. Progress callback fires per-chunk; the napi bridge accumulates committed-file bytes and clamps against the catalog's `size_bytes` total before forwarding to JS so JS sees monotonic `(received, total)` against a constant total. Cooperative cancellation polled between chunks (`Arc<AtomicBool>` Rust, `AbortSignal` JS). No HTTP Range / resume — interrupted downloads delete the partial and restart on next call (tracked in `docs/backlog.kanban.md`). `find_model` is cache-relative and strict: returns `Some(dir)` iff every catalog file is present with size > 0; otherwise `None`. No env vars, no magic paths, no fallback search list.

**Error surface (`src/error.rs`):** `CadmusError` variants — `Load`, `Decode`, `Resample`, `Inference`, `Poisoned`, `AlreadyFreed`, `UnknownModel`, `Download`, `Io`, `InvalidArgument`. JS-side: every variant surfaces as a plain `Error` with `code: <VariantName>`. **Synchronous throws** (the call returns/throws on the JS thread, not via Promise rejection): `InvalidArgument` for malformed `ModelRef` (both fields set or neither) and unknown `computeType`; `AlreadyFreed` for `transcribe()` after `free()` (mirror `freed` flag on the napi `CadmusModel` checked before `AsyncTask` construction); `UnknownModel` for `loadModel({ name })` against a name not in the catalog. Mechanism: `JsError<String>::throw_into` plus a `PendingException` sentinel. **Async-task rejections**: `Load`, `Decode`, `Resample`, `Inference`, `Download`, `Io`, `Poisoned` propagate the typed code by building the JS Error directly with `napi_create_error` in `Task::reject` and packing it into `napi::Error::maybe_raw` so the framework's deferred-reject path forwards the error verbatim.

**Public API surface (`src/api.rs`):** Rust crate exports — `Cadmus`, `CadmusConfig`, `CadmusModel`, `ModelRef` (with `From<&str>`/`From<String>`/`From<&Path>`/`From<PathBuf>`), `ModelInfo`, `ModelFamily`, `LoadModelOptions`, `TranscribeOptions`, `DownloadModelOptions`, `TranscriptResult`, `Segment`, `CadmusError`, `ComputeType`, `Version`, `transcribe` (free function, takes `&Path` not `ModelRef`), `version` (free function). Constructor: `Cadmus::new(CadmusConfig { model_cache: PathBuf })?` — synchronous, fallible (creates the cache directory if absent; returns `Err(CadmusError::Io)` if blocked). Methods on `Cadmus`: `list_available_models() -> Vec<ModelInfo>`, `find_model(&str) -> Option<PathBuf>`, `download_model(&str, DownloadModelOptions) -> Result<PathBuf, CadmusError>`, `load_model(impl Into<ModelRef>, LoadModelOptions) -> Result<CadmusModel, CadmusError>`. Methods on `CadmusModel`: `transcribe(&[u8], TranscribeOptions) -> Result<TranscriptResult, CadmusError>`, `free()`. npm package exports — `Cadmus` (constructor: `new Cadmus(config)`, **synchronous**, may throw `CadmusError`), `CadmusModel` (type alias for `NativeCadmusModel`), `transcribe` (free function, async), `version` (free function, sync), plus type re-exports `CadmusConfig`, `CadmusError`, `ComputeType`, `DownloadModelOptions`, `LoadModelOptions`, `ModelFamily`, `ModelInfo`, `ModelRef`, `Segment`, `TranscribeOptions`, `TranscriptResult`, `Version`. **There is no `createCadmus` factory.** `cadmus.downloadModel`, `cadmus.loadModel`, `model.transcribe`, free `transcribe` are async (`Promise<...>`); `cadmus.listAvailableModels`, `cadmus.findModel`, `model.free`, `version` are sync.

**`Cadmus` factory pattern (D11/D12/D18):** Catalog inspection, model resolution, downloading, and loading are methods on a `Cadmus` handle constructed with an explicit cache directory. Two operations remain free functions because they need no cache: `version()` and the one-shot `transcribe(audio, modelPath, opts)` — and the one-shot takes a path, not a `ModelRef`, because catalog-name resolution requires a `Cadmus` handle. `ModelRef` is `{ name: string } | { path: string }` (TS) or the matching enum (Rust); both fields set or neither set throws `InvalidArgument`. There is no environment-variable fallback, no platform-specific magic path, no fallback search list.

**Cargo / npm packaging (D27):** Two artifacts ship from the single repository root, each with its own allowlist so neither tarball bleeds the other ecosystem's noise. `Cargo.toml [package].include` lists root-anchored patterns `/Cargo.toml`, `/Cargo.lock`, `/build.rs`, `/src/**/*.rs`, `/tests/**/*.rs`, `/fixtures/**`, `/LICENSE`, `/LICENSE-THIRD-PARTY`, `/README.md`. Anchoring matters: unanchored `LICENSE` would otherwise pull `node_modules/**/LICENSE` into the published crate. `package.json.files` lists `index.js`, `index.d.ts`, `types.js`, `types.d.ts`, `cadmus.darwin-arm64.node`, `cadmus.linux-x64-gnu.node`, `cadmus.win32-x64-msvc.node`, `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md`. Verification is `cargo package --list` plus `npm pack --dry-run` before each publish.

**Build pipeline:** `cargo build` produces only the rlib. `cargo build --features napi` compiles `src/napi.rs` and runs `napi_build::setup()` from `build.rs` (gated on `CARGO_FEATURE_NAPI` — env-var check, not `cfg(feature)`, because Cargo does not propagate package features into build scripts as `cfg` flags). `napi build --release --platform --no-js --dts napi-binding.d.ts --features napi` emits `cadmus.<platform>.node` plus the napi-rs declaration file; `tsc` emits `index.{js,d.ts}` + `types.{js,d.ts}` to the repository root. The `--no-js` flag is mandatory: without it, napi-cli emits its own `index.js` dispatcher that collides with the hand-written one. ct2rs's build script invokes CMake to build CTranslate2 + the platform's BLAS backend (5–25 min on a fresh host). Per-platform ct2rs feature subset (cargo per-target dependencies): macOS `whisper`, `accelerate`, `ruy`; Linux `whisper`, `mkl`, `dnnl`, `openmp-runtime-comp`; Windows `whisper`, `dnnl` (no OpenMP — `openmp-runtime-intel` needs `libiomp5` and `openmp-runtime-comp` links GCC's `gomp`, neither available on MSVC; the Windows `.node` is built with `OPENMP_RUNTIME=NONE`). CUDA / cuDNN / cuda-dynamic-loading explicitly excluded.

**Test layout — two Rust modes plus Node:** `cargo test --features napi` runs the 34 unit tests inside `src/` (decode × 10, inference × 13 incl. end-to-end + D4, opus × 3, storage × 5, napi × 2, lib × 1); the integration file `tests/public_api.rs` resolves to zero tests in this mode. `cargo test` (no features) runs the same unit tests *plus* the 7 integration tests in `tests/public_api.rs` against the public Rust surface — the file is gated `#![cfg(not(feature = "napi"))]` because the napi-flavoured rlib references N-API symbols that only resolve inside Node's process. **Both cargo modes are part of release verification.** `npm test` runs `node --test --test-concurrency=1 tests/*.test.mjs` (16 cases): version, catalog (17 entries with populated metadata), download, lifecycle (free-after-free + free-during-inflight + concurrent), transcribe (handle path + one-shot), wav helper.

**Runtime dependencies — Rust:** `ct2rs =0.9.18`, `symphonia =0.5.4` (mp3 + wav + flac + pcm + mkv + isomp4 + aac features), `unsafe-libopus =0.2.0` (pure-Rust libopus for WebM/Opus), `rubato =0.16.2`, `ureq =3.3.0` (rustls + platform-verifier), `napi 3.8.6` + `napi-derive 3.5.5` (optional, behind `napi` feature). Build-deps: `napi-build 2.3.1`. **Runtime dependencies — npm:** zero. devDependencies: `@napi-rs/cli ^3.6.2`, `@types/node ^25.6.0`, `typescript ^6.0.3`. Node ≥ 22, ESM. MSRV: current stable Rust at release time (edition 2024).

**Invariants — things that will bite you if you assume otherwise:**

`free()` is mandatory on the JS side. There is no V8 finalizer. A `CadmusModel` that becomes unreachable in JS without `free()` having been called leaks the native instance for the lifetime of the process. `Drop` on the Rust side runs automatically at scope exit and is equivalent to `free()`.

`free()` does not abort in-flight transcriptions. A `transcribe()` Promise created **before** `free()` resolves with its result; new `transcribe()` calls submitted **after** `free()` always throw `AlreadyFreed`. The native instance is released only after the last in-flight call finishes. This is value-over-abort by design — discarding an in-progress transcription is more wasteful than letting it finish.

`transcribe()` after `free()` throws synchronously, not as a rejected promise. Failure is observable before any async work begins. Mirror `freed` flag on the napi-side `CadmusModel` checked before `AsyncTask` construction.

Audio format is detected from content, not from filename or caller hints. A `.mp3` file containing WAV data transcribes correctly. Truly corrupt audio raises `Decode`.

`TranscriptResult.text` is segments joined with no separator beyond what the model emits. Segments may carry leading whitespace from Whisper's tokenizer. `text.trim()` is always safe.

`TranscriptResult.language` echoes intent when explicit, returns `""` when omitted. ct2rs 0.9.18 runs language detection internally but discards the detected token before returning chunks, so when `TranscribeOptions::language == None` the result carries an empty string rather than the detected language. Documented as `severity: accepted` in `docs/bug.kanban.md`; upstream tracking in `docs/backlog.kanban.md`. The explicit-language round-trip case (the common one) is unaffected.

Segment times come from Whisper's `<|t|>` timestamp tokens, parsed by Cadmus from the model output. Granularity is segment-level, typically 30-second chunks subdivided by silence and punctuation. Word-level timestamps are out of scope for v1.

`download_model` does not verify integrity. No checksum. A truncated download surfaces later as `Load` when the consumer tries to load the directory.

`find_model` is cache-relative and strict. Search target is the configured `model_cache` directory only; returns `Some(dir)` iff the directory exists *and* every entry from `ModelInfo::files` is present with non-zero size; `None` otherwise.

Concurrent `transcribe()` on the same context is safe. `ct2rs::ffi::Whisper` is `Send + Sync`; `Whisper::generate` runs lock-free against `Arc<Whisper>` clones. Both Rust unit tests and JS-side `Promise.all` round-trip tests assert this.

`download_model` progress is monotonic against a constant total. The bridge accumulates committed-file bytes and clamps against the catalog total before forwarding to the JS callback.

Default `threads` equals logical CPU count. Test environments running multiple contexts simultaneously must lower this explicitly via `LoadModelOptions::threads` to avoid memory pressure.

`TranscribeOptions::threads` is intentionally absent. ct2rs 0.9.18 has no per-call thread override — threading lives on `Config::num_threads_per_replica`, set when `Whisper::new` is called and fixed for the life of the instance. `LoadModelOptions::threads` is the only thread knob. Documented as `severity: accepted` in `docs/bug.kanban.md`.

Cadmus ships prebuilt `.node` binaries for three platforms — macOS arm64, Linux x86_64, and Windows x86_64. Each is built by the `Release` workflow on its native GitHub-hosted runner; cross-compilation is not used. Linux-arm64, macOS-x64, and GPU variants remain out of scope and are tracked in `docs/backlog.kanban.md`.

Releases run through GitHub Actions ([`.github/workflows/release.yml`](.github/workflows/release.yml)): a manual `workflow_dispatch` builds all three binaries, bumps the version, commits the binaries, tags, and publishes to npm with provenance. The per-platform ct2rs feature subsets in `Cargo.toml`, the `package.json.files` allowlist, and the `index.ts` platform dispatch are the three places a new target must be wired.

Single Cargo crate, no workspace. Rust consumers add `cadmus` as a git dependency (not yet on crates.io) and never compile napi-rs. Node consumers `npm install @ai-inquisitor/cadmus` and never compile Rust. The Rust source tarball (`cargo package`) contains Rust source only; the npm tarball contains the prebuilt `.node` binaries plus a tiny TS/JS surface — see the packaging-allowlist invariant above.
