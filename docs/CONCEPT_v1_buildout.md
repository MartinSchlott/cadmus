# CONCEPT_v1_buildout — Cadmus 0.1.0

Living reference for the path from zero to a publishable Cadmus 0.1.0: Rust crate on crates.io, `@ai-inquisitor/cadmus` on npm, two target platforms, end-to-end fixture transcription green via `cargo test` and `node --test`.

For product surface see [definition.md](definition.md). For internal architecture see [architecture.md](architecture.md). This concept records cross-plan decisions, scope, plan sequencing, and architectural deltas that have not yet landed in the core docs.

---

## Decision Register

Numbered decisions. Once a plan starts, the decisions it builds on are frozen for that plan. Revisions require a new round through this concept.

### Engine and core

- **D1.** Inference engine: CTranslate2 via `ct2rs` (with `whisper` feature). Not whisper.cpp.
- **D2.** Core API is **synchronous**. No `async fn`, no executor dependency, no `tokio` in the public surface.
- **D3.** Node bridge offloads each operation via napi-rs `AsyncTask` to the libuv threadpool. Bridge does no business logic.
- **D4.** Memory model: `CadmusModel` holds the inner `ct2rs::Whisper` as `Arc<Whisper>` plus an atomic "freed" sentinel. No `Mutex` on the inference path: ct2rs explicitly declares the underlying FFI struct `Send + Sync` (`ct2rs/src/sys/whisper.rs:524–525`: `unsafe impl Send for ffi::Whisper`, `unsafe impl Sync for ffi::Whisper`), so concurrent `generate` calls are safe at the type level. `free()` is non-blocking and does not abort in-flight transcriptions — reference-counted deferred release. Value-over-abort.
- **D5.** Audio pipeline: `symphonia` (decode) → in-house downmix → `rubato` (resample) → `Vec<f32>` at 16 kHz mono in `[-1, 1]`. No FFmpeg.
- **D6.** Model format: directory (CTranslate2 layout, e.g. `Systran/faster-whisper-base/` containing `model.bin`, `config.json`, `tokenizer.json`, `vocabulary.txt`). The authoritative per-model file list — including `preprocessor_config.json` and any model-specific extras — is finalised in PLAN_model_helpers against the actual Hugging Face repositories and stored in `ModelInfo::files` (D15).

### Platform, packaging, and build

- **D7.** v1 is CPU-only. CUDA/cuDNN/CUDA-dynamic-loading explicitly excluded from `ct2rs` feature set, even though they are part of ct2rs's per-platform defaults.
- **D8.** Two target platforms in v1:
  - `aarch64-apple-darwin` — Apple Accelerate (system framework) + `ruy`.
  - `x86_64-unknown-linux-gnu` — oneMKL via `intel-onemkl-prebuild` (statically linked) + `dnnl` + `openmp-runtime-comp`.
- **D9.** Build prerequisites: C++ toolchain + CMake. Required on each build host; not required for npm consumers (prebuilt `.node` is committed).
- **D17.** MSRV: current stable Rust at release time. We do not promise compatibility with older toolchains. Edition `2024`.
- **D22.** **Single Cargo crate, no workspace.** napi exposed via a `napi` feature flag (`napi = ["dep:napi", "dep:napi-derive"]`). `[lib] crate-type = ["cdylib", "lib"]` lets one source tree produce both the rlib (for Rust consumers) and the cdylib (for napi-rs). Rust consumers do `cargo add cadmus` and never see napi — feature is opt-in.
- **D23.** **`package.json` at the repository root.** No `npm/` subdirectory. `index.ts`/`types.ts`/`index.js` live at the root alongside `Cargo.toml`. Same layout as the sibling `endymion` project.
- **D24.** **Prebuilt `.node` binaries are committed to the repository** as `cadmus.darwin-arm64.node` and `cadmus.linux-x64-gnu.node`. They are listed in `package.json`'s `files` array. `npm install @ai-inquisitor/cadmus` ships these pre-built — no consumer-side build.
- **D25.** **No CI, no GitHub Actions in v1.** No `.github/workflows/`. Verification is local: developer runs `cargo test` and `node --test` before each release. Building the Linux binary happens on the developer's Linux machine (separate host); the macOS binary on the Mac. Both `.node` files are committed before publish. A future migration to GitHub Actions remains possible — none of v1's decisions block it.
- **D26.** Test runner: Node's built-in `node --test`, not Vitest. One dev-dependency dropped relative to the earlier draft.
- **D27.** **Explicit packaging boundaries.** Single repo root produces two artifacts; each has its own whitelist so the crates.io tarball and the npm tarball never bleed into each other.

  **`Cargo.toml` declares `[package].include`** (allowlist — anything not listed is excluded from the published crate):
  ```toml
  include = [
      "Cargo.toml",
      "Cargo.lock",
      "build.rs",
      "src/**/*.rs",
      "tests/**/*.rs",
      "fixtures/**",
      "LICENSE",
      "LICENSE-THIRD-PARTY",
      "README.md",
  ]
  ```
  Everything else at the repo root — `package.json`, `tsconfig.json`, `index.ts`, `types.ts`, `index.js`, `dist/`, `cadmus.*.node`, `tests/**/*.mjs`, `node_modules/`, `docs/` — is excluded from `cargo publish` by virtue of not being listed.

  **`package.json` declares `files`** (allowlist — npm publishes only listed paths):
  ```json
  "files": [
    "index.js",
    "index.d.ts",
    "cadmus.darwin-arm64.node",
    "cadmus.linux-x64-gnu.node",
    "LICENSE",
    "LICENSE-THIRD-PARTY",
    "README.md"
  ]
  ```
  Rust source, `Cargo.toml`, `build.rs`, `tests/`, `fixtures/`, `docs/` are excluded from `npm publish`.

  Net result: the Rust consumer pulls ~tens of kilobytes of source via crates.io, well under the 10 MB limit. The npm consumer pulls the two prebuilt binaries plus a tiny TS/JS surface. Neither artifact ships the other ecosystem's noise.

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

- **Single Cargo crate** `cadmus` published on crates.io.
- **npm package** `@ai-inquisitor/cadmus` published with prebuilt `.node` binaries committed at the repo root.
- **Two platforms:** `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`.
- Full audio pipeline: symphonia + rubato + downmix → ct2rs.
- 17-entry catalog (Whisper + Distil-Whisper) with `cached` / `description` / etc.
- `Cadmus` factory handle with explicit model-cache configuration.
- Local fixture-transcription verification on each platform before release.
- `download_model` with progress callback and cooperative cancellation.

### Out of scope for v1 (deferred)

- GPU inference (CUDA/Metal/Vulkan) — D7.
- Streaming / real-time partial transcription.
- Word-level timestamps (segment-level only).
- Model integrity verification (checksums, signatures).
- V8 finalizer / GC-driven `free` on the JS side.
- **Windows (`x86_64-pc-windows-msvc`).** Needs a Windows host with MSVC and `intel-onemkl-prebuild` MSVC artifacts. Cross-compilation from macOS or Linux is not feasible for this stack. Deferred until a Windows build host is available.
- **GitHub Actions / CI matrix.** Verification is fully local in v1. Migration to CI later remains a clean addition — no v1 decision blocks it.
- Vitest as the JS test runner.
- Linux-arm64, macOS-x64.
- Benchmarking suite as a public artifact.
- Documentation site / hosted docs.

---

## Plan Breakdown

Linear chain. Each plan completes (Implementation → Validation → Doc Update → Archive) before the next begins.

| # | Plan file | Scope | Done when |
|---|---|---|---|
| 1 | `PLAN_skeleton.md` | Single Cargo crate `cadmus` with `[lib] crate-type = ["cdylib", "lib"]` and `napi` feature flag (D22). `package.json` at repo root with `napi` build script (D23). `LICENSE` (MIT). Edition `2024`. Stub crate exports a single `version()` function (Rust + napi-feature-gated). `tsconfig.json`, `index.ts`/`types.ts` skeletons. Fixture `fixtures/eins-zwei-drei.mp3` committed. ct2rs is **already a dependency** (Variante B from prior discussion) with the per-platform feature subset from D8 — so the build exercises CTranslate2's CMake build immediately, not only later. No logic beyond `version()`. | On macOS: `cargo build --release --features napi`, `napi build --release --platform`, `npm pack` all succeed. On Linux: same. Both `cadmus.<triple>.node` files exist locally after their respective platform's build. `cargo test` (no tests yet) and `node --test tests/` (one trivial version test) both green on each host. |
| 2 | `PLAN_audio_pipeline.md` | symphonia decode + downmix + rubato resample → `Vec<f32>` at 16 kHz mono in `[-1, 1]`. Crate-internal API (`pub(crate)` or visible only via `#[cfg(test)]`). Fixture-based tests with WAV/MP3/FLAC variants of the test phrase. | `cargo test -p cadmus` passes the audio pipeline tests; the fixture decodes to the expected sample count and rate on both platforms. |
| 3 | `PLAN_inference_core.md` | `ct2rs` integration as **crate-internal** machinery only — no public loading API yet. Internal Whisper-handle wrapper implementing D4 directly (`Arc<Whisper>` + atomic freed sentinel; no mutex). Whisper `<\|t\|>` timestamp-token parser → `Segment[]`. Crate-internal fixture-transcription test under `#[cfg(test)]` — lives inside `src/`, not in a separate `tests/` directory, because no public surface exists yet. | `cargo test` produces a transcript containing "eins" from the fixture, on both platforms. The crate exports nothing user-callable that loads or transcribes — those land in Plan 4. |
| 4 | `PLAN_model_helpers.md` | Public Rust API surface in one shot: `Cadmus`, `CadmusConfig`, `CadmusModel`, `ModelRef` (D11/D12/D18). Static catalog (D14), `cadmus.list_available_models`, `cadmus.find_model`, `cadmus.download_model` with progress + cooperative cancel, `cached` detection (D19), `cadmus.load_model(ModelRef)`. Free one-shot `transcribe(audio, &Path, opts)` and free `version()`. Public Rust integration test in `tests/` exercising the full surface against the fixture. | Catalog tests green; `cadmus.download_model("tiny", ...)` populates a temp dir; `cadmus.list_available_models()` returns 17 entries with correct `cached` flags; `tests/` end-to-end transcription via `Cadmus::load_model` green on both platforms. |
| 5 | `PLAN_napi_surface.md` | `napi`-feature-gated AsyncTask wrappers in the **same crate**. TypeScript wrapper at the repo root (`index.ts`, `types.ts`) including the `ModelRef` discriminated union. Replace the trivial Plan-1 `version()` JS test with a `node --test` suite covering: version, catalog, find, load+transcribe, free-after-free, free-during-inflight, concurrent transcribe. **Both `.node` binaries built locally** (developer runs `napi build` on macOS, then on Linux) and committed to the repo. | `npm test` (which runs `node --test tests/`) green on macOS using the committed `cadmus.darwin-arm64.node`, and green on Linux using the committed `cadmus.linux-x64-gnu.node`. `npm pack` produces a tarball that contains both binaries and works on a fresh `npm install`. |

---

## Architecture Notes — Deltas from architecture.md

The current [architecture.md](architecture.md) describes:
- a Cargo workspace with two crates (`cadmus` + `cadmus-node`),
- a separate `npm/` directory,
- a three-platform target list including Windows,
- GitHub Actions CI workflows,
- Vitest as the test runner,
- free-function APIs (`cadmus::load_model(...)`, etc).

This concept supersedes those: D22 (single crate + feature flag), D23 (root `package.json`), D24 (committed binaries), D25 (no CI), D26 (`node --test`), D8 (two platforms), D11/D12/D18 (`Cadmus` handle pattern). At Concept Closeout, architecture.md and definition.md are rewritten to match. Until then, this concept is the source of truth where they diverge.

### Repository layout (illustrative)

```
/
├── Cargo.toml                        # single crate, [lib] crate-type = ["cdylib", "lib"]
├── build.rs                          # napi-build (when napi feature enabled)
├── package.json                      # root; @ai-inquisitor/cadmus
├── tsconfig.json
├── index.ts                          # TS surface
├── types.ts
├── index.js                          # built or hand-written; re-exports the .node
├── LICENSE                           # MIT
├── LICENSE-THIRD-PARTY               # added in Plan 2 once symphonia (MPL-2.0) is in
├── cadmus.darwin-arm64.node          # prebuilt; committed; produced by `napi build` on Mac
├── cadmus.linux-x64-gnu.node         # prebuilt; committed; produced by `napi build` on Linux
├── src/
│   ├── lib.rs                        # public Rust API + #[cfg(feature = "napi")] re-exports
│   ├── napi.rs                       # napi bridge (only compiled with --features napi)
│   ├── model.rs, transcribe.rs, decode.rs, segments.rs, error.rs, helpers/
├── fixtures/
│   └── eins-zwei-drei.mp3
├── tests/                            # Rust integration tests + node --test suite
│   ├── *.rs
│   └── *.mjs
├── docs/                             # this concept, definition.md, architecture.md
└── target/                           # gitignored
```

The `cadmus-node/` crate from the current architecture.md goes away. Its responsibilities collapse into `src/napi.rs` behind `#[cfg(feature = "napi")]`.

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

## Release Runbook

No separate Plan 7. Releases are a short manual sequence. Documented here so it lives next to the decisions that shape it.

When releasing v0.X.Y:

1. Bump `version` in `Cargo.toml` and `package.json`. Commit.
2. **On macOS** (`aarch64-apple-darwin`): `napi build --release --platform`. Verify `npm test` green. Commit the produced `cadmus.darwin-arm64.node`.
3. **On Linux** (`x86_64-unknown-linux-gnu`): pull the version-bump commit. `napi build --release --platform`. Verify `npm test` green. Commit the produced `cadmus.linux-x64-gnu.node`.
4. Push the branch (with both binary commits) and tag `v0.X.Y`.
5. **Verify packaging boundaries** (D27) before any publish:
   - `cargo package --list` — output must contain only Rust source, `Cargo.toml`/`Cargo.lock`, `build.rs`, `tests/**/*.rs`, `fixtures/**`, and licence/README files. No `.node` files, no `package.json`, no `index.ts`, no `tests/**/*.mjs`.
   - `npm pack --dry-run` — output must contain only `index.js`, `index.d.ts`, both `.node` files, and licence/README. No `Cargo.toml`, no `src/`, no `tests/`, no `fixtures/`.
6. From any host: `cargo publish` (the source is platform-independent; the rlib that crates.io ships is built per-consumer).
7. From any host: `npm publish` (the prebuilt binaries are in the `files` array — both shipped together).

If a future v0.X.Y+1 needs to migrate to GitHub Actions: the same six steps become a workflow file. No source change required.

---

## Risks (system-level)

- **R1.** A future ct2rs release removes the `Send + Sync` impls on `ffi::Whisper` (currently at `ct2rs/src/sys/whisper.rs:524–525`) → D4 no longer compiles. Mitigation: pin `ct2rs` to a specific minor version in `Cargo.toml`; PLAN_inference_core falls back to `Arc<Mutex<Whisper>>` if a future upgrade ever needs it.
- **R2.** `Systran/faster-whisper-*` and `Systran/faster-distil-whisper-*` repos get renamed, restructured, or rate-limited by Hugging Face → `download_model` breaks for end users. Mitigation: catalog `repo` field is a string; a patch release adjusts the catalog. No hot-fix mechanism for already-installed copies — accepted.
- **R3.** napi-rs's `AsyncTask` semantics on Electron renderer surface a subtle UAF that the `Arc<Whisper>` deferred-release pattern misses → reviewer specifically validates the free-during-inflight test in PLAN_napi_surface before approval.
- **R4.** Apple Accelerate API surface changes between macOS releases break the `aarch64-apple-darwin` build → ct2rs's responsibility upstream; we follow ct2rs releases, no in-house mitigation.
- **R5.** No CI matrix means regressions on the not-currently-developing platform stay invisible until the next release-time build. Mitigation: PLAN_napi_surface mandates a green test run on **both** platforms before any tag is pushed; the Release Runbook codifies this. Discipline, not automation. If discipline slips often, that is the trigger to introduce GitHub Actions.

---

## Affected Documents at Concept Closeout

- **definition.md** — §3 Product Promise (drop Windows-specific BLAS line); §4 Operations and Data Types rewritten to reflect the `Cadmus` factory pattern, `ModelRef`, and the extended `ModelInfo`; §7 Success Criteria updated to two platforms, no CI.
- **architecture.md** — §2 Repository Structure rewritten to single crate + root `package.json`; §6 Build Pipeline rewritten to local builds; §7 Platform Targets reduced to two; §8 Test Strategy switched from Vitest to `node --test`; §9 Public Surface signatures rewritten.
- **README.md** — created at Closeout (not before) summarising public usage in both Rust and TypeScript with a few realistic snippets, plus a build-from-source section pointing at the runbook.

The Architect (Human-initiated, per Hard Rule 14) drives Closeout: announces planned changes, gets Human confirmation, performs the migration, archives this concept under `docs/archive/`.
