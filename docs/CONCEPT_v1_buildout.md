# CONCEPT_v1_buildout — Cadmus 0.1.0

Living reference for the path from zero to a publishable Cadmus 0.1.0: Rust crate on crates.io, `@ai-inquisitor/cadmus` on npm, three target platforms, end-to-end fixture transcription green on every push.

For product surface see [definition.md](definition.md). For internal architecture see [architecture.md](architecture.md). This concept records cross-plan decisions, scope, plan sequencing, and architectural deltas that have not yet landed in the core docs.

---

## Decision Register

Numbered decisions. Once a plan starts, the decisions it builds on are frozen for that plan. Revisions require a new round through this concept.

### Engine and core

- **D1.** Inference engine: CTranslate2 via `ct2rs` (with `whisper` feature). Not whisper.cpp.
- **D2.** Core crate is **synchronous**. No `async fn`, no executor dependency, no `tokio` in the public API surface.
- **D3.** Node bridge offloads each operation via napi-rs `AsyncTask` to the libuv threadpool. Bridge does no business logic.
- **D4.** Memory model: `CadmusModel` holds the inner `ct2rs::Whisper` as `Arc<Whisper>` plus an atomic "freed" sentinel. No `Mutex` on the inference path: ct2rs explicitly declares the underlying FFI struct `Send + Sync` (`ct2rs/src/sys/whisper.rs:524–525`: `unsafe impl Send for ffi::Whisper`, `unsafe impl Sync for ffi::Whisper`), so concurrent `generate` calls are safe at the type level. `free()` is non-blocking and does not abort in-flight transcriptions — reference-counted deferred release. Value-over-abort.
- **D5.** Audio pipeline: `symphonia` (decode) → in-house downmix → `rubato` (resample) → `Vec<f32>` at 16 kHz mono in `[-1, 1]`. No FFmpeg.
- **D6.** Model format: directory (CTranslate2 layout, e.g. `Systran/faster-whisper-base/` containing `model.bin`, `config.json`, `tokenizer.json`, `vocabulary.txt`). The authoritative per-model file list — including `preprocessor_config.json` and any model-specific extras — is finalised in PLAN_model_helpers against the actual Hugging Face repositories and stored in `ModelInfo::files` (D15).

### Platform and build

- **D7.** v1 is CPU-only. CUDA/cuDNN/CUDA-dynamic-loading explicitly excluded from `ct2rs` feature set, even though they are part of ct2rs's per-platform defaults.
- **D8.** BLAS strategy:
  - `x86_64-unknown-linux-gnu`: oneMKL via `intel-onemkl-prebuild` (statically linked).
  - `aarch64-apple-darwin`: Apple Accelerate (system framework) + ruy.
  - `x86_64-pc-windows-msvc`: oneMKL via `intel-onemkl-prebuild` (statically linked).
- **D9.** Build prerequisites: C++ toolchain + CMake. Required on developer machines and CI; not required for npm consumers (prebuilt `.node`).
- **D17.** MSRV: current stable Rust at release time. We do not promise compatibility with older toolchains.

### API shape (delta from current architecture.md)

- **D11.** Model cache directory is **explicit and required**. No environment-variable defaults, no platform-specific magic paths. The caller provides a path when constructing a `Cadmus` handle.
- **D12.** **`Cadmus` factory pattern.** `loadModel` / `findModel` / `downloadModel` / `listAvailableModels` are no longer free functions — they become methods on a `Cadmus` handle constructed once with `CadmusConfig`. Handle holds the cache path and any other lib-wide config. Two functions remain free because they need no cache: `version()` and the one-shot `transcribe(audio, modelPath, opts)` — note the **path** parameter, not a `ModelRef`. A free function cannot resolve catalog names without a cache, so the one-shot accepts only an absolute path. Catalog-name resolution requires a `Cadmus` handle.
- **D18.** **`ModelRef` enum** for `Cadmus::load_model` accepts either a catalog name or an absolute path:
  ```rust
  pub enum ModelRef<'a> {
      Name(&'a str),       // resolved against the configured cache
      Path(&'a Path),      // direct path to a model directory
  }
  ```
  TS equivalent: `cadmus.loadModel({ name: 'base' })` or `cadmus.loadModel({ path: '/abs/path' })`. No heuristic dispatch.

  `ModelRef` is a `Cadmus`-scoped concept. The free one-shot `transcribe()` from D12 does **not** accept it — it takes a path directly, because no cache exists outside a handle.

### Catalog

- **D13.** Catalog is hard-coded static data inside the crate. No network calls, no JSON file shipping, no runtime catalog updates. Catalog updates ship with Cadmus releases.
- **D14.** Catalog covers **17 entries**:
  - **Whisper canonical (12):** `tiny`, `tiny.en`, `base`, `base.en`, `small`, `small.en`, `medium`, `medium.en`, `large-v1`, `large-v2`, `large-v3`, `large-v3-turbo`
  - **Distil-Whisper (5):** `distil-small.en`, `distil-medium.en`, `distil-large-v2`, `distil-large-v3`, `distil-large-v3.5`
- **D15.** `ModelInfo` extended:
  ```rust
  pub struct ModelInfo {
      pub name:         String,
      pub description:  String,    // GUI-displayable, one short sentence
      pub size_bytes:   u64,
      pub family:       ModelFamily,
      pub multilingual: bool,      // false for `.en` and Distil-EN-only entries
      pub cached:       bool,      // computed at call time against the configured cache
      pub repo:         String,    // e.g. 'Systran/faster-whisper-base'
      pub files:        Vec<String>,  // expected files inside the model directory
  }
  pub enum ModelFamily { Whisper, DistilWhisper }
  ```
- **D19.** `cached` detection: model directory exists AND every entry in `ModelInfo::files` is present with non-zero size. No checksum (out of scope per definition §6), but stricter than "directory exists".

### Defaults and licensing

- **D16.** Default `compute_type`: `Auto` (ct2rs picks based on the model). Documentation will recommend `int8` for CPU users who want maximum throughput.
- **D20.** License: MIT. symphonia is MPL-2.0 — file-scoped copyleft, attribution in `NOTICE`/`LICENSE-THIRD-PARTY`. Does not infect Cadmus's own code.
- **D21.** Versioning: pre-1.0 (0.x.y) — breaking changes allowed between minor versions. We declare 1.0 ad-hoc when the API has stabilized; no fixed criteria.

---

## Scope Boundaries

### In scope for v1

- Rust crate `cadmus` published on crates.io.
- npm package `@ai-inquisitor/cadmus` published with prebuilt platform binaries.
- Three platforms: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`.
- Full audio pipeline: symphonia + rubato + downmix → ct2rs.
- Catalog of 17 Whisper + Distil-Whisper entries with `cached`/`description`/etc.
- `Cadmus` factory handle with explicit model-cache configuration.
- End-to-end fixture transcription on every CI push, on every platform.
- `download_model` with progress callback and cooperative cancellation.

### Out of scope for v1 (deferred)

- GPU inference (CUDA/Metal/Vulkan) — D7.
- Streaming / real-time partial transcription.
- Word-level timestamps (segment-level only).
- Model integrity verification (checksums, signatures).
- V8 finalizer / GC-driven `free` on the JS side.
- Additional platforms: `aarch64-unknown-linux-gnu`, `aarch64-pc-windows-msvc`, `x86_64-apple-darwin`.
- Benchmarking suite as a public artifact.
- Documentation site / hosted docs.

---

## Plan Breakdown

Linear chain. Each plan completes (Implementation → Validation → Doc Update → Archive) before the next begins.

| # | Plan file | Scope | Done when |
|---|---|---|---|
| 1 | `PLAN_workspace_skeleton.md` | Cargo workspace with `cadmus` and `cadmus-node` (empty stubs), `npm/` package skeleton with TypeScript config, fixtures committed, GitHub Actions matrix building green on three platforms. No logic. | `cargo build` succeeds on all three platforms in CI; `npm pack` produces a valid (empty-but-typed) package. |
| 2 | `PLAN_audio_pipeline.md` | symphonia decode + downmix + rubato resample → `Vec<f32>` at 16 kHz mono in `[-1, 1]`. Public-but-internal Rust API: `decode_audio(bytes) -> Result<Vec<f32>, CadmusError>`. Fixture-based tests with WAV/MP3/FLAC variants. | Audio pipeline tests green; can decode the fixture and produce expected sample count. |
| 3 | `PLAN_inference_core.md` | `ct2rs` integration as **crate-internal** machinery only — no public loading API yet. Internal Whisper-handle wrapper implementing D4 directly (`Arc<Whisper>` + atomic freed sentinel; no mutex). Whisper `<\|t\|>` timestamp-token parser → `Segment[]`. Crate-internal fixture-transcription test under `#[cfg(test)]` — lives inside `cadmus/src/`, not in `tests/rust/`, because no public surface exists yet. | `cargo test -p cadmus` produces a transcript containing "eins" from the fixture, on all three platforms in CI. The `cadmus` crate exports nothing user-callable that loads or transcribes — those land in Plan 4. |
| 4 | `PLAN_model_helpers.md` | Public API surface in one shot: `Cadmus`, `CadmusConfig`, `CadmusModel`, `ModelRef` (D11/D12/D18). Static catalog (D14), `cadmus.list_available_models`, `cadmus.find_model`, `cadmus.download_model` with progress + cooperative cancel, `cached` detection (D19), `cadmus.load_model(ModelRef)`. Free one-shot `transcribe(audio, &Path, opts)` and free `version()`. Public integration test in `tests/rust/` exercising the full surface against the fixture. | Catalog tests green; `cadmus.download_model("tiny", ...)` populates a temp dir; `cadmus.list_available_models()` returns 17 entries with correct `cached` flags; `tests/rust/` end-to-end transcription via `Cadmus::load_model` green on three platforms. |
| 5 | `PLAN_node_bridge.md` | `cadmus-node` crate. AsyncTask wrappers around the sync core. Arc-based deferred-release memory model from D4. TypeScript wrapper (`npm/index.ts`, `npm/types.ts`) including the `ModelRef` discriminated union. Vitest suite covering version, catalog, find, load+transcribe, free-after-free, free-during-inflight, concurrent transcribe. | `npm test` green on all three platforms; Node end-to-end transcribes the fixture. |
| 6 | `PLAN_ci_distribution.md` | napi-rs platform-package layout. CI matrix builds prebuilt `.node` per target, packs platform packages, runs Vitest against the prebuilt binary. CI-side model cache via `actions/cache` keyed on model name. | `npm install` of a CI artifact on each platform produces a working binary; smoke test passes. |
| 7 | `PLAN_release_pipeline.md` | Tag-triggered workflow: publish `cadmus` to crates.io, publish `@ai-inquisitor/cadmus` and per-platform packages to npm. Version bump in `Cargo.toml` and all `package.json` files synchronized. | Pushing `v0.1.0` tag publishes crate and packages; consumer `cargo add cadmus` and `npm install @ai-inquisitor/cadmus` both work end-to-end. |

---

## Architecture Notes — Deltas from architecture.md

The current [architecture.md](architecture.md) describes free-function APIs (`cadmus::load_model(...)`, `cadmus::find_model(...)`, etc.). D11 and D12 change this to a factory-handle pattern. Until Concept Closeout, both documents will diverge on this point — the concept is the source of truth for the new design; architecture.md will be rewritten to match at Closeout.

### Rust delta (illustrative, not normative)

```rust
use cadmus::{Cadmus, CadmusConfig, CadmusModel, ModelRef, ModelInfo, TranscribeOptions, transcribe, version};

let cadmus = Cadmus::new(CadmusConfig {
    model_cache: PathBuf::from("/var/cache/myapp/whisper"),
})?;

let models: Vec<ModelInfo> = cadmus.list_available_models();
if let Some(t) = models.iter().find(|m| m.name == "base" && !m.cached) {
    cadmus.download_model("base", DownloadModelOptions::default())?;
}

let model: CadmusModel = cadmus.load_model(ModelRef::Name("base"), LoadModelOptions::default())?;
let result = model.transcribe(&audio_bytes, TranscribeOptions::default())?;
model.free();   // optional in Rust; Drop also works

// One-shot — does not need the handle, takes an explicit path (not a ModelRef)
let result = transcribe(&audio_bytes, Path::new("/abs/path"), TranscribeOptions::default())?;
```

### TypeScript delta (illustrative, not normative)

```typescript
import { createCadmus, transcribe, version } from '@ai-inquisitor/cadmus';

const cadmus = await createCadmus({ modelCache: '/var/cache/myapp/whisper' });

const models = cadmus.listAvailableModels();
const base = models.find(m => m.name === 'base');
if (base && !base.cached) {
  await cadmus.downloadModel('base');
}

const model = await cadmus.loadModel({ name: 'base' });
const result = await model.transcribe(audio);
model.free();

// One-shot — takes a path string directly
const result = await transcribe(audio, '/abs/path');
```

Plans determine the exact signatures; the snippets above sketch intent only.

---

## Risks (system-level)

- **R1.** A future ct2rs release removes the `Send + Sync` impls on `ffi::Whisper` (currently at `ct2rs/src/sys/whisper.rs:524–525`) → D4 no longer compiles. Mitigation: pin `ct2rs` to a specific minor version in `Cargo.toml`; PLAN_inference_core falls back to `Arc<Mutex<Whisper>>` if a future upgrade ever needs it.
- **R2.** `intel-onemkl-prebuild` does not provide a working static MKL for `windows-latest` MSVC at build time → fallback to OpenBLAS or vendored MKL. Mitigation: PLAN_ci_distribution adds a Windows-specific dry run early; if it fails, replace before publishing v0.1.0.
- **R3.** `Systran/faster-whisper-*` and `Systran/faster-distil-whisper-*` repos get renamed, restructured, or rate-limited by Hugging Face → `download_model` breaks for end users. Mitigation: catalog `repo` field is a string; a patch release adjusts the catalog. No hot-fix mechanism for already-installed copies — accepted.
- **R4.** napi-rs's `AsyncTask` semantics on Electron renderer surface a subtle UAF that the `Arc<Whisper>` deferred-release pattern misses → reviewer specifically validates the free-during-inflight test in PLAN_node_bridge before approval.
- **R5.** Apple Accelerate API surface changes between macOS releases break the `aarch64-apple-darwin` build → ct2rs's responsibility upstream; we follow ct2rs releases, no in-house mitigation.

---

## Affected Documents at Concept Closeout

- **definition.md** — §4 Operations and Data Types rewritten to reflect the `Cadmus` factory pattern, `ModelRef`, and the extended `ModelInfo`.
- **architecture.md** — §9 Public Surface signatures rewritten; §1–§7 reviewed and pruned of any whisper.cpp residue or pre-handle phrasing if present.
- **README.md** — created at Closeout (not before) summarising public usage in both Rust and TypeScript with a few realistic snippets. Optional; decided per Human approval at Closeout.

The Architect (Human-initiated, per Hard Rule 14) drives Closeout: announces planned changes, gets Human confirmation, performs the migration, archives this concept under `docs/archive/`.
