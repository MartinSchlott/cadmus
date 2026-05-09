# PLAN_public_api — Public Rust surface for Cadmus

Plan #5 of [CONCEPT_v1_buildout](CONCEPT_v1_buildout.md). Promotes the crate-internal machinery from Plans 2–4 into the public Rust API: a `Cadmus` factory handle (D11/D12), the `ModelRef` enum (D18), the full 17-entry catalog (D14/D15) with `cached` detection (D19), public `CadmusModel`, public option/result types, the public `CadmusError`, and the free one-shot `transcribe(audio, &Path, opts)`. End-to-end coverage by an integration test under `tests/`.

Reference: [CONCEPT_v1_buildout.md](CONCEPT_v1_buildout.md), in particular **D11** (explicit cache, no env / magic paths), **D12** (`Cadmus` factory pattern; free `transcribe()` takes a path), **D14** (17 entries), **D15** (`ModelInfo` shape), **D16** (default `compute_type = Auto`), **D18** (`ModelRef`), **D19** (`cached` detection), **D20** (license), and the Plan 5 row of the Plan Breakdown. Also relevant: definition.md §4 (operations + data types + errors), with Concept-level overrides for `find_model` (no env / no magic paths) and for `Cadmus`-handle methods.

---

## Context & Goal

After Plan 4 the crate has:

- `decode::decode_to_pcm16k(&[u8]) -> Result<Vec<f32>, AudioError>` — full audio pipeline (Plan 2).
- `storage::{download, ensure_present, FileSpec, ModelEntry, TINY}` — HuggingFace downloader plus the single-entry `tiny` model used by Plan 4's tests (Plan 3).
- `inference::{InferenceHandle, InferenceError, Segment, parse_segments}` — model handle implementing D4 plus the segment parser (Plan 4).
- A single public function `cadmus::version() -> Version`.

Plan 5 turns this into a usable Rust library. After this plan a Rust caller can:

```rust
use cadmus::{Cadmus, CadmusConfig, ModelRef, LoadModelOptions, TranscribeOptions, transcribe};

let cadmus = Cadmus::new(CadmusConfig { model_cache: "/var/cache/cadmus".into() })?;

let models = cadmus.list_available_models();             // 17 entries
if !models.iter().any(|m| m.name == "tiny" && m.cached) {
    cadmus.download_model("tiny", Default::default())?;
}

let model = cadmus.load_model(ModelRef::from("tiny"), LoadModelOptions::default())?;
let result = model.transcribe(&audio_bytes, TranscribeOptions::default())?;
println!("{}", result.text);
model.free();   // optional in Rust; Drop also works

// One-shot — takes a path, no cache resolution.
let result = transcribe(&audio_bytes, Path::new("/abs/path"), TranscribeOptions::default())?;
```

This plan is the first one with a meaningful **public** surface. Everything until now has been crate-internal scaffolding.

### What this plan does

- Adds two new modules: `src/catalog.rs` (catalog data, `ModelInfo`, `ModelFamily`) and `src/error.rs` (public `CadmusError`).
- Adds a third new module `src/api.rs` with `Cadmus`, `CadmusConfig`, `CadmusModel`, `ModelRef`, the public option/result types, and the free `transcribe()` function.
- Migrates the 17-entry catalog data into `catalog.rs`. `storage::TINY` becomes a thin alias pointing at the same `&'static [FileSpec]` slice as the catalog's tiny entry — Plan 4's tests do not change.
- Extends `InferenceHandle` (still `pub(crate)`) with `new_with_config` and `transcribe_with_options` so the public layer can map `LoadModelOptions` / `TranscribeOptions` onto `ct2rs::Config` / `ct2rs::WhisperOptions`, and so it can surface the language code Whisper emits as a control token in its output. The Plan-4 wrappers (`new`, `transcribe`) remain as thin defaults so the existing Plan-4 D4 tests continue to compile unmodified.
- Implements **language detection** by parsing Whisper's `<|xx|>` language control token out of the generate-output chunks. When `TranscribeOptions::language == None`, ct2rs's `Whisper::generate(samples, None, ..., options)` already runs detection internally (`ct2rs/src/whisper.rs:132-147` in 0.9.18) and emits the detected code as the first control token of the first chunk; Plan 5 captures it and surfaces it as `TranscriptResult.language`. No new ct2rs API call needed.
- Adds `tests/public_api.rs` — Rust integration test exercising the full surface against the fixture, idempotent on the existing `target/cadmus-test-cache/tiny/` cache from Plan 3.
- Creates `docs/bug.kanban.md` (does not exist yet — first use, per CLAUDE.md "created on first use") with one `severity: accepted` card for the dropped `TranscribeOptions::threads` field. This is a conscious deviation from `definition.md §4.2` recorded as an accepted bug, not as future-work backlog (the only feasible implementation in ct2rs 0.9.18 — tearing down and rebuilding the Whisper instance per call — is unacceptably costly; we accept the deviation).
- Adds a flag note in `definition.md` (and a brief note in this plan) that §4.3's error variant list is extended in this plan; the formal definition.md rewrite belongs to Concept Closeout, not Plan 5.

### What this plan does **not** do

- napi bridge. Plan 6 (`PLAN_napi_surface`) wraps everything in `AsyncTask`s. No `#[cfg(feature = "napi")]` code is added or modified by this plan beyond what Plan 1 already committed (the trivial `version()` re-export).
- TypeScript surface. Plan 6 / 7. `index.ts` and `types.ts` are untouched.
- Linux build. Plan 7. macOS-only verification per the concept's Linux-deferral override.
- Word-level timestamps, integrity verification, GPU. Out of scope per Concept.
- A standalone `find_model(name, searchPaths)` free function. D11/D12: `find_model` is a method on `Cadmus`, scoped to the configured cache. No env-var lookup, no platform magic paths.
- Doc rewrites. `definition.md` and `architecture.md` get their full pass at Concept Closeout (§Affected Documents at Concept Closeout). Plan 5 creates `docs/bug.kanban.md` and notes inline pointers where surface diverges from `definition.md`; the formal text rewrite waits.

## Breaking Changes

**None for downstream consumers** (there are no public consumers of v0.5.0 — the published crate is still surface-`version()`-only; no users will encounter API churn).

**One internal-surface change** (intentional, no `pub` impact): `InferenceHandle` gains two new methods (`new_with_config`, `transcribe_with_options`). The existing `InferenceHandle::new(model_dir)` and `InferenceHandle::transcribe(samples, language)` from Plan 4 are kept as thin wrappers so Plan 4's tests do not need to change.

**Concept supersedes definition.md** in two places that this plan exercises:
- `find_model` accepts only a name (cache-relative lookup) — definition.md §5's "explicit `searchPaths`, then `CADMUS_MODEL_DIR` env var, then `~/.cache/cadmus/models/`" is invalidated by D11. The Coder updates `definition.md` only at Concept Closeout; this plan implements the new contract.
- `CadmusError` gains `Download`, `UnknownModel`, and `Io` variants beyond the six listed in definition.md §4.3. Same Closeout reconciliation.

**One conscious target-vision deviation**, recorded as a `severity: accepted` card in the new `docs/bug.kanban.md`:
- `TranscribeOptions::threads` (definition.md §4.2) is **not** implemented. ct2rs 0.9.18 has no per-call thread override — threading is a Config-level setting (`num_threads_per_replica`), set at model load time. The only feasible per-call workaround would tear down and rebuild the underlying `Whisper` instance per call, which is wildly more expensive than the inference itself. We accept the deviation. `LoadModelOptions::threads` remains the only thread knob.

**No** Cargo dependency changes. **No** `package.json` changes. **No** napi changes. **No** changes to `cadmus.darwin-arm64.node`.

## Reference Patterns

- **`src/inference.rs`** (Plan 4) — same shape Plan 5 follows in its new modules: `pub` types where surface-bound, `pub(crate)` where internal, `#[cfg(test)] mod tests` appended. The `Mutex<Option<Arc<Whisper>>>` pattern for D4 is preserved verbatim through `Arc<InferenceHandle>` ownership inside `CadmusModel`.
- **`src/storage.rs`** (Plan 3) — pattern for declaring `&'static [FileSpec]` data as top-level statics; the catalog re-uses this idiom for all 17 entries.
- **ct2rs whisper example** (`ct2rs/examples/whisper.rs`) — canonical `Whisper::new(model_dir, Config { ... })` + `whisper.generate(samples, language, true, &WhisperOptions { beam_size, ... })`. The plan maps user-facing options onto these fields directly.
- **definition.md §4 + §9.1** — the Rust signature sketch is illustrative; D11/D12/D14/D15/D18 in the Concept supersede it where they differ.

## Dependencies

**None added.** Everything in this plan compiles against what is already in `Cargo.toml` after Plan 4: `ct2rs`, `symphonia`, `rubato`, `ureq`, `napi`/`napi-derive` (feature-gated, untouched here).

If `ct2rs::Config { compute_type, num_threads_per_replica, ... }` or `ct2rs::WhisperOptions { beam_size, ... }` field names differ at implementation time from what Step 5 codes (the Coder verified the names against `ct2rs 0.9.18` at plan-write time — see `~/.cargo/registry/src/.../ct2rs-0.9.18/src/sys/whisper.rs:44-81` and `.../src/sys/config.rs:328-345`), the Coder stops and reports per Hard Rule 11.

## Assumptions & Risks

- **A1.** ct2rs 0.9.18 exposes:
  - `pub struct Config { pub compute_type: ComputeType, pub num_threads_per_replica: usize, ... }` (`src/sys/config.rs:328`).
  - `pub use ComputeType` with constants `DEFAULT`, `AUTO`, `INT8`, `INT8_FLOAT16`, `FLOAT16`, `FLOAT32` (`src/sys/config.rs:393-410`).
  - `pub struct WhisperOptions { pub beam_size: usize, ... }` (`src/sys/whisper.rs:44`).
  - `Whisper::new(model_path: T, config: Config) -> Result<Self>` (`src/whisper.rs:66`) — note `config` is **owned**, not `&Config`.
  - `Whisper::generate(samples, language, timestamp, options: &WhisperOptions) -> Result<Vec<String>>` (consumed by Plan 4 already).
  Plan 5 uses these literally. If a future ct2rs minor renames any field, the Coder stops and reports.
- **A2.** All 17 model repos on HuggingFace are reachable at the names hard-coded in Step 3. Plan 4 has already exercised `Systran/faster-whisper-tiny` + `openai/whisper-tiny`; the other 15 entries are baked into the plan from plan-write-time HuggingFace lookup (2026-05-09). The integration test only ever pulls `tiny` at runtime — the other 16 entries are tested only for static catalog shape, not against live HF. R2 of the Concept already accepts the residual exposure (HF rename / restructure → patch release adjusts the catalog; Hard Rule 7 if a smoke test ever fails on an entry).
- **A3.** The integration test reuses the existing `target/cadmus-test-cache/tiny/` populated by Plan 3 / Plan 4. On a cold cache the test downloads ~75 MB once. No other model is downloaded by the test. The test does **not** verify all 17 catalog entries against live HF — that is impractical (multiple GB of bandwidth) and out of scope; it asserts the catalog's static shape (17 entries, all fields populated, no duplicate names) and the `tiny` round trip end-to-end.
- **A4.** Catalog `size_bytes` values in Step 3 are **approximate** by contract (definition.md §4.2: "approximate download size in bytes"). They are looked up from HuggingFace web UI on the plan-write date (2026-05-09) and baked into the plan as fixed integers. Single-MB drift across CT2 quantization revisions is accepted; the integration test does not assert on `size_bytes` numerically. If a future HF revision shifts a `model.bin` size by more than ~10%, that is a content-update concern (patch the catalog), not a contract violation.
- **A5.** Language detection works by parsing Whisper's `<|xx|>` language control token from the generate output. ct2rs's `Whisper::generate(samples, None, true, options)` runs detection internally and emits the detected code as the first control token in the first output chunk (`ct2rs/src/whisper.rs:132-147`). The new helper `inference::detect_language_from_chunks(&[String]) -> Option<String>` scans for the first `<|xx|>` token whose body is 2–3 ASCII-lowercase letters and is not a known non-language control token (`<|transcribe|>`, `<|translate|>`, `<|notimestamps|>` are >3 chars and excluded by length; numeric timestamp tokens are excluded by character class). If `TranscribeOptions::language == Some(lang)`, the value is echoed verbatim; if `None`, the detected token is used; if no token is present (extremely rare — would mean Whisper produced no output at all), the field falls back to an empty string.
- **R1.** `Cadmus::new` calls `fs::create_dir_all(model_cache)`. If the path is unwritable (permission denied, parent missing as a writable directory, file already exists at the path), `new` returns `CadmusError::Io(...)`. Mitigation: a dedicated integration test (`cadmus_new_io_error_when_cache_path_blocked` in Step 8) creates a regular file at a temp path and then tries to use that file path as `model_cache`; `fs::create_dir_all` fails with `NotADirectory` and the error surfaces as `CadmusError::Io`.
- **R2.** `CadmusModel::transcribe(&self, audio: &[u8], opts)` decodes the audio bytes inside the call (D5 pipeline). For long-running async callers this means the decode + resample stages run on the calling thread before inference begins. Definition.md §4 already commits to a synchronous, blocking core; the napi bridge in Plan 6 will offload the whole call to a libuv worker, so this is invisible at the public Node surface. Accepted.
- **R3.** The integration test in `tests/public_api.rs` runs in a separate cargo target (the integration-test crate) and does **not** see crate-internal items. It exercises only `pub` API. Build-time test of "the public surface compiles end-to-end against itself" is therefore the integration test's primary value, beyond its functional coverage.
- **R4.** `ModelInfo.size_bytes` values in the catalog are looked up from HuggingFace at plan-write time (web UI on each model's `model.bin` file size). They are **approximate** — CT2 quantization variations can shift sizes by single-digit MB between revisions. Documentation calls these "approximate download size" (matches definition.md §4.2 wording). The integration test does **not** assert on `size_bytes` numerically.
- **R5.** `tests/public_api.rs` performs a real model load and inference on the host machine. Runtime budget: ~3–8 s on `aarch64-apple-darwin` after the warm cache of Plans 3/4. Cold cache adds ~75 MB download (~30 s on a typical home connection).

One new `severity: accepted` card in the **new file** `docs/bug.kanban.md` (created by this plan; CLAUDE.md's "created on first use" applies here):

- **`TranscribeOptions::threads` not implemented** — definition.md §4.2 lists `threads: Option<u32>` on `TranscribeOptions`. ct2rs 0.9.18 has no per-call thread override; the only feasible workaround would tear down and rebuild the `Whisper` instance per call, which costs orders of magnitude more than the inference itself. The deviation is conscious. `LoadModelOptions::threads` remains the only thread knob.

No new `Open` backlog cards. No BREAKs. Linux deferred per concept override.

## Steps

Single phase, macOS-only execution per the concept's Linux-deferral override. Order: extend `InferenceHandle` → migrate catalog data → public types → `Cadmus` handle + `CadmusModel` + free `transcribe` → wire exports → tests → bug-card creation → verification.

### 1. Extend `InferenceHandle` with options-aware methods + language detection

In `src/inference.rs`, add (do not replace) two methods plus a new return type and a helper. The existing `new` and `transcribe` from Plan 4 stay as thin wrappers so Plan 4's D4 tests continue to compile unmodified.

```rust
/// Output of an inference run. Carries the parsed segments and, when
/// available, the language code Whisper emitted as a control token.
#[derive(Debug)]
pub(crate) struct InferenceOutput {
    pub segments:          Vec<Segment>,
    pub detected_language: Option<String>,
}

impl InferenceHandle {
    /// Construct with an explicit ct2rs Config. Plan 5 hands in a Config
    /// derived from LoadModelOptions; Plan 4's `new()` is now a thin
    /// wrapper that delegates with Config::default().
    pub(crate) fn new_with_config(
        model_dir: &Path,
        config: ct2rs::Config,
    ) -> Result<Self, InferenceError> {
        let whisper = Whisper::new(model_dir, config)
            .map_err(|e| InferenceError::Load(e.to_string()))?;
        Ok(Self {
            inner: Mutex::new(Some(Arc::new(whisper))),
            freed: AtomicBool::new(false),
        })
    }

    /// Transcribe with explicit WhisperOptions. Returns parsed segments
    /// plus the language code Whisper emitted as a control token (if any).
    /// Same D4 free-safety as `transcribe`: lock held only across
    /// freed-check + Arc::clone.
    pub(crate) fn transcribe_with_options(
        &self,
        samples: &[f32],
        language: Option<&str>,
        options: &WhisperOptions,
    ) -> Result<InferenceOutput, InferenceError> {
        if self.freed.load(Ordering::SeqCst) {
            return Err(InferenceError::AlreadyFreed);
        }
        let local: Arc<Whisper> = {
            let guard = self.inner.lock().map_err(|_| InferenceError::Poisoned)?;
            match guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => return Err(InferenceError::AlreadyFreed),
            }
        };
        let chunks = local
            .generate(samples, language, true, options)
            .map_err(|e| InferenceError::Generate(e.to_string()))?;
        let detected_language = detect_language_from_chunks(&chunks);
        let segments = parse_segments(&chunks);
        Ok(InferenceOutput { segments, detected_language })
    }
}

/// Scan the first chunk for Whisper's `<|xx|>` language control token.
/// Whisper emits the detected language as the first control token of the
/// first chunk when called with `language = None` (ct2rs runs detection
/// internally). The token body is a 2- or 3-character ISO 639-1/2 code,
/// e.g. `<|de|>`, `<|en|>`. Non-language control tokens (`<|transcribe|>`,
/// `<|translate|>`, `<|notimestamps|>`, `<|startoftranscript|>`,
/// `<|endoftext|>`) are all >3 characters and excluded by length;
/// timestamp tokens (`<|0.00|>`) contain `.` and digits and are excluded
/// by the all-ASCII-lowercase check.
pub(crate) fn detect_language_from_chunks(chunks: &[String]) -> Option<String> {
    let first = chunks.first()?;
    let bytes = first.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'|' {
            if let Some(end) = find_token_end(bytes, i + 2) {
                let tok = &first[i + 2..end];
                if is_language_token(tok) {
                    return Some(tok.to_string());
                }
                i = end + 2;
                continue;
            }
        }
        i += 1;
    }
    None
}

fn is_language_token(t: &str) -> bool {
    let len = t.len();
    if len < 2 || len > 3 {
        return false;
    }
    t.chars().all(|c| c.is_ascii_lowercase())
}
```

`find_token_end` already exists in `src/inference.rs` (Plan 4 segment parser); reuse it.

Refactor the existing `new` and `transcribe` to delegate:

```rust
impl InferenceHandle {
    pub(crate) fn new(model_dir: &Path) -> Result<Self, InferenceError> {
        Self::new_with_config(model_dir, Config::default())
    }

    pub(crate) fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
    ) -> Result<Vec<Segment>, InferenceError> {
        self.transcribe_with_options(samples, language, &WhisperOptions::default())
            .map(|out| out.segments)
    }
}
```

`Whisper::new` takes `Config` by value (`src/whisper.rs:66`) — `new_with_config` therefore takes `Config` by value too, not `&Config`. Plan 4's tests are unaffected: they call `InferenceHandle::new(&dir)` and `handle.transcribe(samples, Some("de"))`, both of which still exist with the same return types (`Result<Self>`, `Result<Vec<Segment>>`).

Add three unit tests for `detect_language_from_chunks` to the existing `#[cfg(test)] mod tests` block in `src/inference.rs`:

```rust
#[test]
fn detect_language_from_chunks_finds_two_letter_token() {
    let chunks = vec!["<|de|><|transcribe|><|0.00|> hallo welt<|2.50|>".to_string()];
    assert_eq!(detect_language_from_chunks(&chunks), Some("de".to_string()));
}

#[test]
fn detect_language_from_chunks_skips_control_and_timestamp_tokens() {
    // No language token in this chunk; only transcribe + timestamp tokens.
    let chunks = vec!["<|transcribe|><|0.00|> just text<|1.00|>".to_string()];
    assert_eq!(detect_language_from_chunks(&chunks), None);
}

#[test]
fn detect_language_from_chunks_empty_input_returns_none() {
    let chunks: Vec<String> = vec![];
    assert_eq!(detect_language_from_chunks(&chunks), None);
}
```

### 2. Move catalog data into `src/catalog.rs`

Create new file `src/catalog.rs`. Top-level: `pub` types `ModelInfo`, `ModelFamily`, plus `pub(crate)` types `CatalogEntry`, the static `CATALOG: &'static [CatalogEntry]`, and helper functions `model_entry(name) -> Option<&'static ModelEntry>` and `lookup(name) -> Option<&'static CatalogEntry>`.

```rust
use std::path::Path;

use crate::storage::{ensure_present, FileSpec, ModelEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFamily {
    Whisper,
    DistilWhisper,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name:         String,
    pub description:  String,
    pub size_bytes:   u64,
    pub family:       ModelFamily,
    pub multilingual: bool,
    pub cached:       bool,        // computed at call time
    pub repo:         String,      // primary HF repo for GUI display
    pub files:        Vec<String>, // filenames inside the model directory
}

pub(crate) struct CatalogEntry {
    pub name:         &'static str,
    pub description:  &'static str,
    pub size_bytes:   u64,
    pub family:       ModelFamily,
    pub multilingual: bool,
    pub repo:         &'static str,
    pub entry:        ModelEntry,   // full per-file (repo, file) map for download
}

impl CatalogEntry {
    pub(crate) fn to_info(&self, cache: &Path) -> ModelInfo {
        let dir = cache.join(self.name);
        let cached = ensure_present(&self.entry, &dir);
        let files: Vec<String> = self.entry.files.iter()
            .map(|f| f.file.to_string())
            .collect();
        ModelInfo {
            name:         self.name.to_string(),
            description:  self.description.to_string(),
            size_bytes:   self.size_bytes,
            family:       self.family.clone(),
            multilingual: self.multilingual,
            cached,
            repo:         self.repo.to_string(),
            files,
        }
    }
}

pub(crate) fn lookup(name: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.name == name)
}

pub(crate) fn model_entry(name: &str) -> Option<&'static ModelEntry> {
    lookup(name).map(|e| &e.entry)
}
```

Note: `CatalogEntry` carries an owned `ModelEntry` (which itself holds `&'static [FileSpec]`). Since `ModelEntry` is `Copy`-shaped (`pub(crate) struct ModelEntry { files: &'static [FileSpec] }`) it may need a `#[derive(Copy, Clone)]` or be accessed by reference (`&e.entry`). The Coder picks whichever compiles. `FileSpec` is already a plain `&'static str` pair and is implicitly `Copy`-able if the derive is added; Plan 3 did not derive it because nothing needed to. Adding `#[derive(Clone, Copy)]` to both `FileSpec` and `ModelEntry` in `src/storage.rs` is acceptable as a one-line internal-only change — these are crate-internal types.

### 3. Define the 17 catalog entries (data baked into the plan)

All 17 entries — file lists, repos, sizes, descriptions, and flags — are fixed below. The Coder copies this data verbatim into `src/catalog.rs`. No web lookups, no per-implementation verification of repo existence required. `size_bytes` values are HuggingFace `model.bin` byte counts at plan-write date 2026-05-09 and are **approximate** by contract (definition.md §4.2: "approximate download size in bytes"); single-MB drift across CT2 quantization revisions is accepted (A4).

If at runtime a HuggingFace repo turns out to have moved or restructured, the live download fails — Hard Rule 7 (plan infeasible → stop and report) applies; this is the Concept's accepted R2 exposure, not a plan defect.

**Per-model file lists.** Whisper canonical entries pull `model.bin`/`config.json`/`tokenizer.json`/`vocabulary.txt` from the matching `Systran/faster-whisper-*` repo and `preprocessor_config.json` from the matching `openai/whisper-*` repo (Plan 3's split). Distil entries pull all five files from `Systran/faster-distil-whisper-*` directly — those CT2 conversions ship `preprocessor_config.json` in-repo.

```rust
// Per-model file lists. pub(crate) so storage::TINY can alias.
pub(crate) static FILES_TINY: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-tiny",     file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-tiny",     file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-tiny",     file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-tiny",     file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-tiny",             file: "preprocessor_config.json" },
];
pub(crate) static FILES_TINY_EN: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-tiny.en",  file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-tiny.en",  file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-tiny.en",  file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-tiny.en",  file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-tiny.en",          file: "preprocessor_config.json" },
];
pub(crate) static FILES_BASE: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-base",     file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-base",     file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-base",     file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-base",     file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-base",             file: "preprocessor_config.json" },
];
pub(crate) static FILES_BASE_EN: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-base.en",  file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-base.en",  file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-base.en",  file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-base.en",  file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-base.en",          file: "preprocessor_config.json" },
];
pub(crate) static FILES_SMALL: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-small",    file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-small",    file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-small",    file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-small",    file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-small",            file: "preprocessor_config.json" },
];
pub(crate) static FILES_SMALL_EN: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-small.en", file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-small.en", file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-small.en", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-small.en", file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-small.en",         file: "preprocessor_config.json" },
];
pub(crate) static FILES_MEDIUM: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-medium",   file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-medium",   file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-medium",   file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-medium",   file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-medium",           file: "preprocessor_config.json" },
];
pub(crate) static FILES_MEDIUM_EN: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-medium.en", file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-medium.en", file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-medium.en", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-medium.en", file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-medium.en",         file: "preprocessor_config.json" },
];
pub(crate) static FILES_LARGE_V1: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-large-v1", file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-large-v1", file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-large-v1", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-large-v1", file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-large",            file: "preprocessor_config.json" },
];
pub(crate) static FILES_LARGE_V2: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-large-v2", file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-large-v2", file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-large-v2", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-large-v2", file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-large-v2",         file: "preprocessor_config.json" },
];
pub(crate) static FILES_LARGE_V3: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-large-v3", file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-large-v3", file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-large-v3", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-large-v3", file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-large-v3",         file: "preprocessor_config.json" },
];
pub(crate) static FILES_LARGE_V3_TURBO: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-whisper-large-v3-turbo", file: "model.bin" },
    FileSpec { repo: "Systran/faster-whisper-large-v3-turbo", file: "config.json" },
    FileSpec { repo: "Systran/faster-whisper-large-v3-turbo", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-whisper-large-v3-turbo", file: "vocabulary.txt" },
    FileSpec { repo: "openai/whisper-large-v3-turbo",         file: "preprocessor_config.json" },
];
pub(crate) static FILES_DISTIL_SMALL_EN: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-distil-whisper-small.en", file: "model.bin" },
    FileSpec { repo: "Systran/faster-distil-whisper-small.en", file: "config.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-small.en", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-small.en", file: "vocabulary.txt" },
    FileSpec { repo: "Systran/faster-distil-whisper-small.en", file: "preprocessor_config.json" },
];
pub(crate) static FILES_DISTIL_MEDIUM_EN: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-distil-whisper-medium.en", file: "model.bin" },
    FileSpec { repo: "Systran/faster-distil-whisper-medium.en", file: "config.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-medium.en", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-medium.en", file: "vocabulary.txt" },
    FileSpec { repo: "Systran/faster-distil-whisper-medium.en", file: "preprocessor_config.json" },
];
pub(crate) static FILES_DISTIL_LARGE_V2: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-distil-whisper-large-v2", file: "model.bin" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v2", file: "config.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v2", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v2", file: "vocabulary.txt" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v2", file: "preprocessor_config.json" },
];
pub(crate) static FILES_DISTIL_LARGE_V3: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3", file: "model.bin" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3", file: "config.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3", file: "vocabulary.txt" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3", file: "preprocessor_config.json" },
];
pub(crate) static FILES_DISTIL_LARGE_V3_5: &[FileSpec] = &[
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3.5", file: "model.bin" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3.5", file: "config.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3.5", file: "tokenizer.json" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3.5", file: "vocabulary.txt" },
    FileSpec { repo: "Systran/faster-distil-whisper-large-v3.5", file: "preprocessor_config.json" },
];

pub(crate) static CATALOG: &[CatalogEntry] = &[
    // ----- Whisper canonical (12) ---------------------------------------
    CatalogEntry {
        name: "tiny",
        description: "39M-parameter Whisper. Fastest; lowest accuracy. Multilingual.",
        size_bytes: 75_500_000,
        family: ModelFamily::Whisper,
        multilingual: true,
        repo: "Systran/faster-whisper-tiny",
        entry: ModelEntry { files: FILES_TINY },
    },
    CatalogEntry {
        name: "tiny.en",
        description: "39M-parameter Whisper. English-only; slightly higher accuracy on English than tiny.",
        size_bytes: 75_500_000,
        family: ModelFamily::Whisper,
        multilingual: false,
        repo: "Systran/faster-whisper-tiny.en",
        entry: ModelEntry { files: FILES_TINY_EN },
    },
    CatalogEntry {
        name: "base",
        description: "74M-parameter Whisper. Better accuracy than tiny; still fast. Multilingual.",
        size_bytes: 145_300_000,
        family: ModelFamily::Whisper,
        multilingual: true,
        repo: "Systran/faster-whisper-base",
        entry: ModelEntry { files: FILES_BASE },
    },
    CatalogEntry {
        name: "base.en",
        description: "74M-parameter Whisper. English-only; better English accuracy than base.",
        size_bytes: 145_300_000,
        family: ModelFamily::Whisper,
        multilingual: false,
        repo: "Systran/faster-whisper-base.en",
        entry: ModelEntry { files: FILES_BASE_EN },
    },
    CatalogEntry {
        name: "small",
        description: "244M-parameter Whisper. Common balance of speed and accuracy. Multilingual.",
        size_bytes: 483_500_000,
        family: ModelFamily::Whisper,
        multilingual: true,
        repo: "Systran/faster-whisper-small",
        entry: ModelEntry { files: FILES_SMALL },
    },
    CatalogEntry {
        name: "small.en",
        description: "244M-parameter Whisper. English-only; better English accuracy than small.",
        size_bytes: 483_500_000,
        family: ModelFamily::Whisper,
        multilingual: false,
        repo: "Systran/faster-whisper-small.en",
        entry: ModelEntry { files: FILES_SMALL_EN },
    },
    CatalogEntry {
        name: "medium",
        description: "769M-parameter Whisper. Substantially higher accuracy; slower. Multilingual.",
        size_bytes: 1_528_000_000,
        family: ModelFamily::Whisper,
        multilingual: true,
        repo: "Systran/faster-whisper-medium",
        entry: ModelEntry { files: FILES_MEDIUM },
    },
    CatalogEntry {
        name: "medium.en",
        description: "769M-parameter Whisper. English-only; high English accuracy.",
        size_bytes: 1_528_000_000,
        family: ModelFamily::Whisper,
        multilingual: false,
        repo: "Systran/faster-whisper-medium.en",
        entry: ModelEntry { files: FILES_MEDIUM_EN },
    },
    CatalogEntry {
        name: "large-v1",
        description: "1.55B-parameter Whisper, original release. Multilingual.",
        size_bytes: 3_087_000_000,
        family: ModelFamily::Whisper,
        multilingual: true,
        repo: "Systran/faster-whisper-large-v1",
        entry: ModelEntry { files: FILES_LARGE_V1 },
    },
    CatalogEntry {
        name: "large-v2",
        description: "1.55B-parameter Whisper, retrained for accuracy. Multilingual.",
        size_bytes: 3_087_000_000,
        family: ModelFamily::Whisper,
        multilingual: true,
        repo: "Systran/faster-whisper-large-v2",
        entry: ModelEntry { files: FILES_LARGE_V2 },
    },
    CatalogEntry {
        name: "large-v3",
        description: "1.55B-parameter Whisper, current generation. Multilingual.",
        size_bytes: 3_087_000_000,
        family: ModelFamily::Whisper,
        multilingual: true,
        repo: "Systran/faster-whisper-large-v3",
        entry: ModelEntry { files: FILES_LARGE_V3 },
    },
    CatalogEntry {
        name: "large-v3-turbo",
        description: "809M-parameter distilled-decoder Whisper. ~6× faster than large-v3 with comparable accuracy. Multilingual.",
        size_bytes: 1_620_000_000,
        family: ModelFamily::Whisper,
        multilingual: true,
        repo: "Systran/faster-whisper-large-v3-turbo",
        entry: ModelEntry { files: FILES_LARGE_V3_TURBO },
    },
    // ----- Distil-Whisper (5) -------------------------------------------
    CatalogEntry {
        name: "distil-small.en",
        description: "Distilled 166M-parameter Whisper-small. ~2× faster than small.en; English-only.",
        size_bytes: 332_000_000,
        family: ModelFamily::DistilWhisper,
        multilingual: false,
        repo: "Systran/faster-distil-whisper-small.en",
        entry: ModelEntry { files: FILES_DISTIL_SMALL_EN },
    },
    CatalogEntry {
        name: "distil-medium.en",
        description: "Distilled 394M-parameter Whisper-medium. ~6× faster than medium.en; English-only.",
        size_bytes: 776_000_000,
        family: ModelFamily::DistilWhisper,
        multilingual: false,
        repo: "Systran/faster-distil-whisper-medium.en",
        entry: ModelEntry { files: FILES_DISTIL_MEDIUM_EN },
    },
    CatalogEntry {
        name: "distil-large-v2",
        description: "Distilled 756M-parameter Whisper-large-v2. ~6× faster; English-only.",
        size_bytes: 1_510_000_000,
        family: ModelFamily::DistilWhisper,
        multilingual: false,
        repo: "Systran/faster-distil-whisper-large-v2",
        entry: ModelEntry { files: FILES_DISTIL_LARGE_V2 },
    },
    CatalogEntry {
        name: "distil-large-v3",
        description: "Distilled 756M-parameter Whisper-large-v3. ~6× faster than large-v3; English-only.",
        size_bytes: 1_534_000_000,
        family: ModelFamily::DistilWhisper,
        multilingual: false,
        repo: "Systran/faster-distil-whisper-large-v3",
        entry: ModelEntry { files: FILES_DISTIL_LARGE_V3 },
    },
    CatalogEntry {
        name: "distil-large-v3.5",
        description: "Distilled 756M-parameter Whisper-large-v3 (v3.5 retrain). ~6× faster than large-v3; English-only.",
        size_bytes: 1_544_000_000,
        family: ModelFamily::DistilWhisper,
        multilingual: false,
        repo: "Systran/faster-distil-whisper-large-v3.5",
        entry: ModelEntry { files: FILES_DISTIL_LARGE_V3_5 },
    },
];
```

The Coder copies this block verbatim. No HF lookups are required during implementation. The plan owns the contract; if a repo turns out to be missing or restructured at the first live download, that is Hard Rule 7 territory (the integration test only ever pulls `tiny`, so the other 16 are static-data-only at this plan's verification grid).

### 4. Migrate `storage::TINY` to the catalog's static slice

In `src/storage.rs`, replace the existing `pub(crate) const TINY: ModelEntry = ...` definition with a re-pointer at the catalog's tiny file list:

```rust
pub(crate) const TINY: ModelEntry = ModelEntry {
    files: crate::catalog::FILES_TINY,
};
```

`FILES_TINY` therefore becomes `pub(crate)` instead of file-private `static`. The intent: there is exactly one source of truth for the tiny file list (`catalog::FILES_TINY`); `storage::TINY` exists only as a backwards-compatible alias for Plan 4's tests, and its files slice is byte-identical to the catalog entry. Tests and `download_tiny_smoke` in `src/storage.rs` continue to compile and run.

If this circular reference (`storage` → `catalog::FILES_TINY` → `storage::FileSpec`) causes a build-order issue (it should not — both are just static data), the Coder may flip the ownership: keep `FILES_TINY` in `storage.rs` and have `catalog::CATALOG[0].entry.files` reference `storage::FILES_TINY`. Either direction satisfies "single source of truth"; the Coder picks whichever compiles cleaner.

### 5. Define the public option/result types and `CadmusError`

Create new file `src/error.rs`:

```rust
use crate::decode::AudioError;
use crate::inference::InferenceError;
use crate::storage::DownloadError;

#[derive(Debug)]
pub enum CadmusError {
    /// Model directory missing, incomplete, or rejected by ct2rs/CTranslate2 at init.
    Load(String),
    /// Audio bytes are corrupt or in an unrecognised format.
    Decode(String),
    /// rubato or downmix stage failed (extremely rare; malformed sample rates).
    Resample(String),
    /// ct2rs returned a failure from `Whisper::generate`.
    Inference(String),
    /// Internal lock poisoned by a panic on another thread; context is unusable.
    Poisoned,
    /// Operation called on a context after `free()`.
    AlreadyFreed,
    /// Catalog-name lookup failed: name is not one of the 17 entries.
    UnknownModel(String),
    /// HuggingFace download failed (cancelled, HTTP error, network, IO).
    Download(String),
    /// Filesystem error on the cache directory (cannot create, cannot read).
    Io(String),
}

impl std::fmt::Display for CadmusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(m)         => write!(f, "load: {m}"),
            Self::Decode(m)       => write!(f, "decode: {m}"),
            Self::Resample(m)     => write!(f, "resample: {m}"),
            Self::Inference(m)    => write!(f, "inference: {m}"),
            Self::Poisoned        => write!(f, "internal lock poisoned"),
            Self::AlreadyFreed    => write!(f, "model already freed"),
            Self::UnknownModel(n) => write!(f, "unknown model: {n}"),
            Self::Download(m)     => write!(f, "download: {m}"),
            Self::Io(m)           => write!(f, "io: {m}"),
        }
    }
}
impl std::error::Error for CadmusError {}

impl From<AudioError> for CadmusError {
    fn from(e: AudioError) -> Self {
        match e {
            AudioError::Decode(m)   => Self::Decode(m),
            AudioError::Resample(m) => Self::Resample(m),
        }
    }
}

impl From<InferenceError> for CadmusError {
    fn from(e: InferenceError) -> Self {
        match e {
            InferenceError::Load(m)     => Self::Load(m),
            InferenceError::Generate(m) => Self::Inference(m),
            InferenceError::Poisoned    => Self::Poisoned,
            InferenceError::AlreadyFreed => Self::AlreadyFreed,
        }
    }
}

impl From<DownloadError> for CadmusError {
    fn from(e: DownloadError) -> Self {
        // Preserve the variant name in the message so callers and tests can
        // discriminate (cancelled vs. http vs. network vs. io) without
        // exposing storage::DownloadError publicly.
        Self::Download(e.to_string())
    }
}
```

Note for the Reviewer: `CadmusError::Download` collapses Plan 3's four-variant `DownloadError` into one public string. The discussion phase chose this over a four-arm public enum to keep the public error surface narrow. If a future caller needs to discriminate cancel vs. http vs. network on the JS side, the variant string parses unambiguously (`"download: download cancelled"`, `"download: http 404: ..."`, etc.), or we can promote variants then.

### 6. Define `Cadmus`, `CadmusConfig`, `CadmusModel`, `ModelRef`, options, and the free `transcribe`

Create new file `src/api.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use ct2rs::{
    sys::{ComputeType as Ct2ComputeType, Config as Ct2Config},
    WhisperOptions,
};

use crate::catalog::{model_entry, ModelInfo, CATALOG};
use crate::decode::decode_to_pcm16k;
use crate::error::CadmusError;
use crate::inference::{InferenceHandle, InferenceOutput};
use crate::storage::{self, ensure_present};

/// Options controlling the model load. ct2rs Config-level settings: thread
/// count and compute type. Per-call options (beam_size, language) are in
/// `TranscribeOptions`.
#[derive(Default)]
pub struct LoadModelOptions {
    pub threads:      Option<u32>,    // None → ct2rs default (0 = auto)
    pub compute_type: ComputeType,    // default: Auto
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputeType {
    #[default] Auto,
    Int8,
    Int8Float16,
    Float16,
    Float32,
}

impl ComputeType {
    fn to_ct2(self) -> Ct2ComputeType {
        match self {
            Self::Auto        => Ct2ComputeType::AUTO,
            Self::Int8        => Ct2ComputeType::INT8,
            Self::Int8Float16 => Ct2ComputeType::INT8_FLOAT16,
            Self::Float16     => Ct2ComputeType::FLOAT16,
            Self::Float32     => Ct2ComputeType::FLOAT32,
        }
    }
}

#[derive(Default, Clone)]
pub struct TranscribeOptions {
    pub language:  Option<String>,    // BCP-47; None → ct2rs internal detection
    pub beam_size: Option<u32>,       // None → ct2rs default (5)
    // NOTE: definition.md §4.2 lists `threads` here; ct2rs has no per-call
    // override (threading is set on Config). Accepted deviation in
    // docs/bug.kanban.md.
}

#[derive(Default)]
pub struct DownloadModelOptions {
    pub on_progress: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
    pub cancel:      Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f32,
    pub end:   f32,
    pub text:  String,
}

#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text:     String,
    pub language: String,
    pub segments: Vec<Segment>,
}

pub enum ModelRef {
    Name(String),
    Path(PathBuf),
}

impl From<&str>     for ModelRef { fn from(s: &str)     -> Self { Self::Name(s.to_string()) } }
impl From<String>   for ModelRef { fn from(s: String)   -> Self { Self::Name(s) } }
impl From<&Path>    for ModelRef { fn from(p: &Path)    -> Self { Self::Path(p.to_path_buf()) } }
impl From<PathBuf>  for ModelRef { fn from(p: PathBuf)  -> Self { Self::Path(p) } }

pub struct CadmusConfig {
    pub model_cache: PathBuf,
}

pub struct Cadmus {
    cache: PathBuf,
}

impl Cadmus {
    pub fn new(config: CadmusConfig) -> Result<Self, CadmusError> {
        fs::create_dir_all(&config.model_cache).map_err(|e| {
            CadmusError::Io(format!("creating cache {:?}: {e}", config.model_cache))
        })?;
        Ok(Self { cache: config.model_cache })
    }

    pub fn list_available_models(&self) -> Vec<ModelInfo> {
        CATALOG.iter().map(|e| e.to_info(&self.cache)).collect()
    }

    pub fn find_model(&self, name: &str) -> Option<PathBuf> {
        let entry = model_entry(name)?;
        let dir = self.cache.join(name);
        ensure_present(entry, &dir).then_some(dir)
    }

    pub fn download_model(
        &self,
        name: &str,
        options: DownloadModelOptions,
    ) -> Result<PathBuf, CadmusError> {
        let entry = model_entry(name)
            .ok_or_else(|| CadmusError::UnknownModel(name.to_string()))?;
        let dir = self.cache.join(name);
        let cb_box = options.on_progress;
        let cb_ref: Option<&dyn Fn(u64, u64)> =
            cb_box.as_ref().map(|b| &**b as &dyn Fn(u64, u64));
        let cancel: Option<&AtomicBool> = options.cancel.as_deref();
        storage::download(entry, &dir, cb_ref, cancel).map_err(CadmusError::from)?;
        Ok(dir)
    }

    pub fn load_model(
        &self,
        model_ref: ModelRef,
        options: LoadModelOptions,
    ) -> Result<CadmusModel, CadmusError> {
        let dir = match model_ref {
            ModelRef::Name(name) => {
                let entry = model_entry(&name)
                    .ok_or_else(|| CadmusError::UnknownModel(name.clone()))?;
                let dir = self.cache.join(&name);
                if !ensure_present(entry, &dir) {
                    return Err(CadmusError::Load(format!(
                        "model {name:?} not present in cache {:?} — call download_model first",
                        self.cache
                    )));
                }
                dir
            }
            ModelRef::Path(p) => p,
        };
        load_inner(&dir, options)
    }
}

fn load_inner(dir: &Path, options: LoadModelOptions) -> Result<CadmusModel, CadmusError> {
    let mut config = Ct2Config::default();
    config.compute_type = options.compute_type.to_ct2();
    if let Some(t) = options.threads {
        config.num_threads_per_replica = t as usize;
    }
    let handle = InferenceHandle::new_with_config(dir, config).map_err(CadmusError::from)?;
    Ok(CadmusModel { handle: Arc::new(handle) })
}

pub struct CadmusModel {
    handle: Arc<InferenceHandle>,
}

impl CadmusModel {
    pub fn transcribe(
        &self,
        audio: &[u8],
        options: TranscribeOptions,
    ) -> Result<TranscriptResult, CadmusError> {
        transcribe_with_handle(&self.handle, audio, options)
    }

    pub fn free(&self) {
        self.handle.free();
    }
}

fn transcribe_with_handle(
    handle: &InferenceHandle,
    audio: &[u8],
    options: TranscribeOptions,
) -> Result<TranscriptResult, CadmusError> {
    let samples = decode_to_pcm16k(audio).map_err(CadmusError::from)?;
    let mut wopts = WhisperOptions::default();
    if let Some(b) = options.beam_size {
        wopts.beam_size = b as usize;
    }
    let out: InferenceOutput = handle
        .transcribe_with_options(&samples, options.language.as_deref(), &wopts)
        .map_err(CadmusError::from)?;
    let segments: Vec<Segment> = out.segments.into_iter().map(|s| Segment {
        start: s.start,
        end:   s.end,
        text:  s.text,
    }).collect();
    let text: String = segments.iter().map(|s| s.text.as_str()).collect();
    // language: explicit input wins; otherwise the code Whisper emitted;
    // otherwise empty string (rare — would mean no output at all).
    let language = options.language
        .clone()
        .or(out.detected_language)
        .unwrap_or_default();
    Ok(TranscriptResult { text, language, segments })
}

/// One-shot: load → transcribe → drop. Takes a path (D12 — no cache
/// resolution outside a Cadmus handle).
pub fn transcribe(
    audio: &[u8],
    model_path: &Path,
    options: TranscribeOptions,
) -> Result<TranscriptResult, CadmusError> {
    let handle = InferenceHandle::new(model_path).map_err(CadmusError::from)?;
    transcribe_with_handle(&handle, audio, options)
}
```

Notes for the Coder:

- `ct2rs::sys::ComputeType` constants are SCREAMING_SNAKE_CASE (`AUTO`, `INT8`, …); the public `cadmus::ComputeType` uses idiomatic CamelCase. The `to_ct2` mapping is the only place this conversion lives.
- `ct2rs::sys::Config` is `pub use`d at the crate root in some ct2rs versions and only via `sys::` in others — the Coder picks whichever the `0.9.18` source actually exposes (verify against `~/.cargo/registry/src/.../ct2rs-0.9.18/src/lib.rs` at impl time).
- `Whisper::new(model_path, config)` takes `Config` by value, so `load_inner` constructs and consumes a new `Ct2Config` per load. Cheap.
- `TranscriptResult.text` is segments joined with no separator (definition.md §5: "segments may carry leading whitespace from Whisper's tokenizer; `text.trim()` is always safe"). The `.collect()` call concatenates the segment texts in order.
- `CadmusModel.free` takes `&self` (matching `InferenceHandle::free`'s signature). `Arc<InferenceHandle>::Drop` runs once the last reference dies.

### 7. Wire public exports in `src/lib.rs`

Replace the body of `src/lib.rs` so it exposes the new modules and the public surface:

```rust
mod api;
mod catalog;
mod decode;
mod error;
mod inference;
mod storage;

pub use api::{
    transcribe,
    Cadmus, CadmusConfig, CadmusModel,
    ComputeType,
    DownloadModelOptions,
    LoadModelOptions,
    ModelRef,
    Segment,
    TranscribeOptions,
    TranscriptResult,
};
pub use catalog::{ModelFamily, ModelInfo};
pub use error::CadmusError;

pub struct Version {
    pub cadmus:      String,
    pub ct2rs:       String,
    pub ctranslate2: String,
}

pub fn version() -> Version {
    Version {
        cadmus:      env!("CARGO_PKG_VERSION").to_string(),
        ct2rs:       env!("CADMUS_DEP_CT2RS_VERSION").to_string(),
        ctranslate2: env!("CADMUS_DEP_CTRANSLATE2_VERSION").to_string(),
    }
}

#[cfg(feature = "napi")]
mod napi_bridge {
    // Untouched. Plan 6 expands this with the full surface.
    use napi_derive::napi;

    #[napi(object)]
    pub struct VersionJs {
        pub cadmus: String,
        #[napi(js_name = "ct2rs")]
        pub ct2rs: String,
        pub ctranslate2: String,
    }

    #[napi]
    pub fn version() -> VersionJs {
        let v = super::version();
        VersionJs {
            cadmus:      v.cadmus,
            ct2rs:       v.ct2rs,
            ctranslate2: v.ctranslate2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_returns_three_string_fields() {
        let v = version();
        assert_eq!(v.cadmus, env!("CARGO_PKG_VERSION"));
        let _: String = v.ct2rs;
        let _: String = v.ctranslate2;
    }
}
```

Note: the existing `version_returns_three_string_fields` test loses its `assert!(v.cadmus.starts_with("0.5.0"))` line because Plan 5 will bump `Cargo.toml`'s `version` to `0.6.0` at Doc Update / Archive time (Concept's pre-1.0 versioning, D21 — minor for any breaking change, including the new public surface). The looser shape assertion above survives the bump. The Coder does **not** bump the version during Plan 5 implementation; that happens at the release-runbook step after Doc Update.

### 8. Add the public integration test `tests/public_api.rs`

Create new file `tests/public_api.rs`. Integration tests in cargo see only the public crate surface — this file therefore doubles as a compile-time check that `pub use` exports cover everything callers need.

```rust
use std::fs;
use std::path::PathBuf;

use cadmus::{
    transcribe,
    Cadmus, CadmusConfig, CadmusError,
    DownloadModelOptions, LoadModelOptions, ModelFamily, ModelRef,
    TranscribeOptions, TranscriptResult,
};

fn shared_cache() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/cadmus-test-cache")
}

fn fixture_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/eins-zwei-drei.mp3");
    fs::read(&path).unwrap_or_else(|_| panic!("fixture missing: {path:?}"))
}

fn assert_eins_zwei_drei(joined: &str) {
    let lower = joined.to_lowercase();
    let one   = lower.contains("eins") || lower.contains("1");
    let two   = lower.contains("zwei") || lower.contains("2");
    let three = lower.contains("drei") || lower.contains("3");
    assert!(one && two && three, "transcript missing 1/2/3 markers: {joined:?}");
}

#[test]
fn cadmus_new_creates_cache_dir() {
    let temp = std::env::temp_dir().join(format!(
        "cadmus-public-api-cache-{}", std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp);
    let cadmus = Cadmus::new(CadmusConfig { model_cache: temp.clone() })
        .expect("new should create the cache dir");
    assert!(temp.is_dir(), "cache dir not created");
    drop(cadmus);
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn cadmus_new_io_error_when_cache_path_blocked() {
    // Place a regular file at a temp path; then try to use that file path
    // as model_cache. fs::create_dir_all fails because a non-directory
    // exists at the target — surfaces as CadmusError::Io.
    let blocking = std::env::temp_dir().join(format!(
        "cadmus-blocking-cache-{}", std::process::id()
    ));
    let _ = fs::remove_file(&blocking);
    let _ = fs::remove_dir_all(&blocking);
    fs::write(&blocking, b"not a dir").expect("seed blocking file");

    let result = Cadmus::new(CadmusConfig { model_cache: blocking.clone() });
    let _ = fs::remove_file(&blocking);

    match result {
        Err(CadmusError::Io(_)) => (),
        Ok(_)   => panic!("expected Io error; new() succeeded"),
        Err(e)  => panic!("expected Io error; got {e:?}"),
    }
}

#[test]
fn list_available_models_has_seventeen_entries() {
    let cadmus = Cadmus::new(CadmusConfig { model_cache: shared_cache() })
        .expect("new failed");
    let models = cadmus.list_available_models();
    assert_eq!(models.len(), 17, "expected 17 catalog entries (D14)");

    let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    let mut dedup = names.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(dedup.len(), names.len(), "duplicate names in catalog");

    // D14 spot-checks: 12 Whisper canonical, 5 Distil-Whisper.
    let whisper   = models.iter().filter(|m| m.family == ModelFamily::Whisper).count();
    let distil    = models.iter().filter(|m| m.family == ModelFamily::DistilWhisper).count();
    assert_eq!(whisper, 12, "expected 12 Whisper canonical entries");
    assert_eq!(distil,   5, "expected 5 Distil-Whisper entries");

    // Multilingual flag rules.
    for m in &models {
        if m.name.ends_with(".en") {
            assert!(!m.multilingual, "{} should not be multilingual", m.name);
        }
    }
    for m in &models {
        assert!(!m.description.is_empty(), "missing description on {}", m.name);
        assert!(m.size_bytes > 0, "size_bytes==0 on {}", m.name);
        assert!(!m.repo.is_empty(), "missing repo on {}", m.name);
        assert!(!m.files.is_empty(), "no files listed for {}", m.name);
    }
}

#[test]
fn unknown_model_returns_unknown_model_error() {
    let cadmus = Cadmus::new(CadmusConfig { model_cache: shared_cache() })
        .expect("new failed");
    let result = cadmus.download_model("nonexistent-name", DownloadModelOptions::default());
    assert!(matches!(result, Err(CadmusError::UnknownModel(_))));
    let result = cadmus.load_model(ModelRef::from("nonexistent-name"), LoadModelOptions::default());
    assert!(matches!(result, Err(CadmusError::UnknownModel(_))));
    assert!(cadmus.find_model("nonexistent-name").is_none());
}

#[test]
fn tiny_round_trip_via_cadmus_handle() {
    let cadmus = Cadmus::new(CadmusConfig { model_cache: shared_cache() })
        .expect("new failed");

    // Stage tiny if not already cached. Plans 3/4 typically left a warm
    // cache; CI-cold runs download once.
    let info = cadmus.list_available_models()
        .into_iter()
        .find(|m| m.name == "tiny")
        .expect("tiny entry missing from catalog");
    if !info.cached {
        cadmus.download_model("tiny", DownloadModelOptions::default())
            .expect("download_model('tiny') failed");
    }

    let found = cadmus.find_model("tiny").expect("find_model after download");
    assert!(found.is_dir());

    let model = cadmus
        .load_model(ModelRef::from("tiny"), LoadModelOptions::default())
        .expect("load_model failed");

    let opts = TranscribeOptions {
        language: Some("de".to_string()),
        ..Default::default()
    };
    let result: TranscriptResult = model
        .transcribe(&fixture_bytes(), opts)
        .expect("transcribe failed");

    assert!(!result.segments.is_empty());
    assert_eq!(result.language, "de");
    assert_eq!(result.text, result.segments.iter().map(|s| s.text.as_str()).collect::<String>(),
        "TranscriptResult.text must be segments joined verbatim");
    assert_eins_zwei_drei(&result.text);

    model.free();

    // Post-free transcribe must surface AlreadyFreed via the public surface.
    let after = model.transcribe(&fixture_bytes(), TranscribeOptions::default());
    assert!(matches!(after, Err(CadmusError::AlreadyFreed)));
}

#[test]
fn one_shot_transcribe_via_path() {
    let cadmus = Cadmus::new(CadmusConfig { model_cache: shared_cache() })
        .expect("new failed");
    if !cadmus.list_available_models().iter().any(|m| m.name == "tiny" && m.cached) {
        cadmus.download_model("tiny", DownloadModelOptions::default())
            .expect("download tiny");
    }
    let dir = cadmus.find_model("tiny").expect("tiny dir");

    let result = transcribe(
        &fixture_bytes(),
        &dir,
        TranscribeOptions {
            language: Some("de".to_string()),
            ..Default::default()
        },
    ).expect("one-shot transcribe failed");

    assert!(!result.segments.is_empty());
    assert_eq!(result.language, "de");
    assert_eins_zwei_drei(&result.text);
}

#[test]
fn language_none_surfaces_detected_code() {
    // ct2rs runs language detection internally when language=None and emits
    // the detected code as a `<|xx|>` control token in the first chunk.
    // The fixture is German speech; tiny on Apple Accelerate detects this
    // reliably as "de". Assert the shape (2- or 3-char ASCII-lowercase) so
    // the test stays robust if detection ever surfaces a different code.
    let cadmus = Cadmus::new(CadmusConfig { model_cache: shared_cache() })
        .expect("new failed");
    if !cadmus.list_available_models().iter().any(|m| m.name == "tiny" && m.cached) {
        cadmus.download_model("tiny", DownloadModelOptions::default())
            .expect("download tiny");
    }
    let model = cadmus
        .load_model(ModelRef::from("tiny"), LoadModelOptions::default())
        .expect("load");
    let result = model.transcribe(&fixture_bytes(), TranscribeOptions::default())
        .expect("transcribe");
    assert!(
        (2..=3).contains(&result.language.len())
            && result.language.chars().all(|c| c.is_ascii_lowercase()),
        "detected language should be a 2-3 char ASCII lowercase code, got {:?}",
        result.language
    );
}
```

The test reuses the gitignored `target/cadmus-test-cache/` shared with Plans 3 and 4. After a fresh checkout the first `cargo test` downloads ~75 MB; warm runs are fast.

Test runtime budget on `aarch64-apple-darwin` after warm cache:

- `cadmus_new_creates_cache_dir`, `list_available_models_has_seventeen_entries`, `unknown_model_returns_unknown_model_error`: < 50 ms each (pure data + filesystem).
- `tiny_round_trip_via_cadmus_handle`: ~2–4 s (one tiny load + ~3 s fixture).
- `one_shot_transcribe_via_path`: ~2–4 s.
- `language_none_surfaces_detected_code`: ~2–4 s.

Total integration cost: ~10–15 s on warm cache, plus the Plan-3/4 cost on cold cache.

### 9. Create `docs/bug.kanban.md` with the accepted-deviation card

`docs/bug.kanban.md` does not exist yet (CLAUDE.md "created on first use"). Create it with the standard markdown-kanban scaffolding (mirror the layout of `docs/backlog.kanban.md`), then add a single `severity: accepted` card in the **Open** column documenting the dropped `TranscribeOptions::threads` field:

```markdown
# Bugs
**Issue and bug tracking for Cadmus**

id: {new}
template: bug

Tracks defects and accepted deviations from the target vision. Cards
with `severity: accepted` document conscious deviations — the
"won't fix" bucket per CLAUDE.md.

## Open
id: {new}

### `TranscribeOptions::threads` not implemented (definition.md §4.2)
id: {new}
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

## In Progress
id: {new}

## Done
id: {new}

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
```

Card IDs are `{new}` placeholders; the markdown-kanban skill replaces them on parse. The exact column-template metadata may need adjusting against the markdown-kanban skill's current schema; the Coder uses the skill (or matches `docs/backlog.kanban.md`'s shape if the skill is unavailable) to ensure the file parses.

No new cards in `docs/backlog.kanban.md`. Language detection is implemented in this plan, not deferred. Per-call threads is an accepted deviation, not pending work.

### 10. Build and test verification

Run, in order:

- `cargo build --release` — green; no `dead_code` warnings on the new modules. Plan 5 introduces the first downstream consumers of every `pub(crate)` item it depends on.
- `cargo build --release --tests` — green.
- `cargo build --release --features napi` — green; the `mod napi_bridge` block is byte-identical to Plan 1's; no surface change.
- `cargo build --release --tests --features napi` — green.
- `cargo test --release` — all tests pass. Total cargo-side count grows from 23 (Plan 4 baseline) to **33**:
  - In `src/inference.rs`: 3 new unit tests for `detect_language_from_chunks` (Step 1) on top of the existing 9 — total 12.
  - In `src/catalog.rs`: 0 — coverage lives in the integration test.
  - In `src/error.rs`: 0 — From-impl correctness is exercised transitively by the integration test.
  - In `src/api.rs`: 0 unit tests in this file — IO-error coverage lives in `tests/public_api.rs::cadmus_new_io_error_when_cache_path_blocked`.
  - In `tests/public_api.rs`: 7 integration tests.
  Net **33 tests pass** on a warm cache, all on macOS (1 version + 8 audio + 5 storage + 12 inference + 7 public_api). The Coder reports the actual `cargo test` summary line in the validation phase if it differs.
- `cargo test --release --features napi` — 26 unit tests pass; the integration crate (`tests/public_api.rs`) is gated out via `#![cfg(not(feature = "napi"))]`. The napi-feature-enabled rlib references N-API runtime symbols (`_napi_throw`, `_napi_set_named_property`, …) that exist only inside Node's process, so a standalone integration-test binary cannot link. End-to-end coverage with the napi surface enabled is Plan 6's `npm test` matrix; until then `--features napi` exercises only the unit-test layer.
- `cargo test --release --test public_api` — runs only the `tests/public_api.rs` integration crate (~10–15 s warm). Note the `--test public_api` form: it selects the integration-test target by file stem. A bare `cargo test public_api` would be a name filter, would not match any test name, and would run zero tests — wrong gate. Also note: this command must be run **without** `--features napi` for the same link-time reason as above.

If any test fails for reasons not covered by A1–A5 (e.g. a real ct2rs API mismatch, an HF rename that breaks the catalog), the Coder stops and reports per Hard Rule 7.

`npm test` is **not** part of this plan's verification matrix — the napi surface is unchanged (`mod napi_bridge` still exposes only `version()`). Plan 6 expands the napi surface and re-tests on the JS side.

### 11. Verify packaging boundaries (D27)

- `cargo package --list --allow-dirty` — additionally lists `src/api.rs`, `src/catalog.rs`, `src/error.rs`, and `tests/public_api.rs`. Still no `package.json`, no `index.ts`, no `node_modules/`, no `cadmus.*.node`, no `target/cadmus-test-cache/...`, no `docs/`.
- `npm pack --dry-run` — unchanged from Plan 4: `index.js`, `index.d.ts`, `cadmus.darwin-arm64.node`, `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md`, plus `package.json`. The `cadmus.linux-x64-gnu.node` warning carries over per the Linux-deferral override.

Implementation is done at this point. Per CLAUDE.md §5 the Coder stops here and waits — Validation, Doc Update, and Archive happen in subsequent phases driven by the next-step prompt.

## Verification

After Step 11, the working tree on macOS satisfies:

- `cargo test --release` → 33 tests pass (1 version + 8 audio + 5 storage + 12 inference + 7 public_api).
- `cargo test --release --features napi` → 26 unit tests pass; integration crate gated out via `#![cfg(not(feature = "napi"))]` (napi runtime symbols are only resolvable inside Node's process — Plan 6 reworks the napi surface and moves end-to-end coverage to `npm test`).
- `cargo build --release` → green, no `dead_code` warnings.
- `cargo build --release --tests` → green.
- `cargo build --release --features napi` → green.
- `cargo build --release --tests --features napi` → green for everything Plan 5 touches; the two pre-existing napi-bridge warnings on `VersionJs` / `version` remain (Plan 1 inheritance).
- `cargo package --list --allow-dirty` → contains the four new files; no leakage of npm-side files; no `target/cadmus-test-cache/...`.
- `npm pack --dry-run` → seven entries on this macOS-only host; `cadmus.linux-x64-gnu.node` warning carries over.
- `target/cadmus-test-cache/tiny/` reused; no new download on a warm cache.
- New public surface compiles via `tests/public_api.rs` against `cadmus::*` from outside the crate (integration-test boundary).
- `cargo doc --no-deps --features napi` → green (no rustdoc warnings on the new public types). Optional check, not required.
- Linux verification deferred per concept override.

Out of scope for this plan's verification: `npm test` and the public TypeScript surface — both arrive in Plan 6.

### Reviewer focus points

- **Concept conformance — D11/D12/D14/D15/D18/D19**:
  - `Cadmus` factory pattern (D12): `list_available_models` / `find_model` / `download_model` / `load_model` are methods on the handle; `transcribe` (one-shot) and `version` are free functions. The free `transcribe` takes `&Path`, **not** `ModelRef` — matches D12's parenthetical clarification verbatim.
  - Cache is explicit and required (D11): `CadmusConfig::model_cache` is `PathBuf` (not `Option<PathBuf>`). No env-var fallback (no `CADMUS_MODEL_DIR` lookup anywhere). No platform magic-path defaults.
  - `find_model` (D11): cache-relative lookup only. No `searchPaths` parameter, no env, no `~/.cache/...`. Definition.md §5's older description is superseded by D11 — the Closeout reconciles `definition.md`.
  - `ModelRef` (D18): owned variants (`Name(String)`, `Path(PathBuf)`), with `From` impls for ergonomic call sites. Concept-D18's borrowed snippet is illustrative. The free one-shot `transcribe` does **not** accept `ModelRef` — D12 conformance.
  - Catalog has 17 entries (D14) — Whisper canonical 12 + Distil 5; integration test asserts the count and the family split.
  - `ModelInfo` shape (D15): `name` / `description` / `size_bytes` / `family` / `multilingual` / `cached` / `repo` / `files` — no extra fields, no missing fields. `repo` is singular (the primary repo); per-file repo split stays internal in `CatalogEntry::entry`.
  - `cached` detection (D19): directory exists AND every file in `entry.files` is present with size > 0 — exactly `storage::ensure_present`. No checksum.
  - `compute_type` default Auto (D16): `ComputeType::default() == Auto`, mapped to `Ct2ComputeType::AUTO` in `to_ct2`. Direct match.
- **`Send + Sync`-correct public types**: `Cadmus`, `CadmusModel`, `ModelRef`, `TranscribeOptions`, `LoadModelOptions`, `DownloadModelOptions`, `TranscriptResult`, `Segment`, `ModelInfo`, `ModelFamily`, `CadmusError` should all be `Send + Sync` (`DownloadModelOptions` deliberately so via `Box<dyn Fn(...) + Send + Sync>` and `Arc<AtomicBool>`). The compiler derives this; the Reviewer checks no field accidentally breaks the auto-impl.
- **`InferenceHandle` extension preserves D4**: `new_with_config` and `transcribe_with_options` keep the same `Mutex<Option<Arc<Whisper>>>` invariant. The mutex critical section stays at freed-check + `Arc::clone`; the `generate` call runs on the cloned `Arc`, lock-free. Plan 4's three D4 tests still pass via the unchanged `new` / `transcribe` wrappers.
- **`CadmusError::Download` collapses `DownloadError`**: discussion-phase choice. Reviewer confirms the variant string preserves enough information to discriminate (`"download cancelled"` / `"http NNN: ..."` / `"network: ..."` / `"io: ..."`) and that no test in this plan or Plan 4 breaks because of the loss of typed discrimination.
- **Error From-impls are total**: every variant of `AudioError`, `InferenceError`, `DownloadError` maps to a `CadmusError` variant with no information loss for `AudioError` and `InferenceError` (one-to-one), and one-string-summary for `DownloadError` (collapse). No `?`-eliding `unwrap`.
- **Integration test is the public-surface compile gate**: `tests/public_api.rs` only `use cadmus::*` items that are publicly exported. If the `pub use` block in `lib.rs` misses an item used by the test, the test fails to compile.
- **`TranscriptResult.text` join policy**: segments concatenated with no separator (definition.md §5). Test asserts `result.text == segments.iter().map(|s| &s.text).collect::<String>()`.
- **`language` echo policy**: explicit `Some(lang)` echoed verbatim; `None` falls through to the language code Whisper emitted as a `<|xx|>` control token (parsed by `inference::detect_language_from_chunks`); empty string only if no token at all (extremely rare).
- **`detect_language_from_chunks` correctness**: pattern matches 2- or 3-char ASCII-lowercase token bodies only. `<|transcribe|>`, `<|translate|>`, `<|notimestamps|>` are excluded by length; `<|0.00|>` and `<|2.50|>` by character class. Three unit tests in `src/inference.rs` cover the positive case, the no-language-token case, and the empty-input case.
- **`severity: accepted` bug card**: `docs/bug.kanban.md` is created by this plan with one card documenting the dropped `TranscribeOptions::threads` field. Reviewer confirms the deviation is justified (ct2rs has no per-call thread surface in 0.9.18) and that no other deviations from `definition.md` are introduced silently.
- **No napi changes**: `mod napi_bridge` is byte-identical to Plan 1's — the Reviewer can confirm via `git diff src/lib.rs` that the napi block did not gain or lose anything.
- **Catalog data is plan-owned, not impl-time looked-up**: Step 3's tables are authoritative. The Coder copies them verbatim. Reviewer spot-checks the file tables for shape (5 files per entry, the right `(repo, file)` split) and the flag/size/description coherency. The integration test only ever pulls `tiny` at runtime.
- **No new `Open` backlog cards introduced by this plan**: the two items raised in the discussion phase resolve into (1) implemented (language detection) and (2) accepted-deviation bug card (per-call threads).
- **Linux deferral honored**: Linux-side build / test omitted entirely.
- **No new TODOs in code without cards** (Hard Rule 8): no silent TODOs, no `unwrap` followed by `// TODO`, no half-implemented branches. The accepted-deviation case is documented in `bug.kanban.md`.

<plan_ready>docs/PLAN_public_api.md</plan_ready>
