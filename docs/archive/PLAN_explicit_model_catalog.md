# PLAN: Explicit Model Catalog (2.0)

## Context & Goal

The model catalog is currently hardcoded as `pub(crate) static CATALOG: &[CatalogEntry]` in `src/catalog.rs:184`. Each entry pins HuggingFace `Systran/faster-whisper-*` and `openai/whisper-*` repos at compile time. Two of these repos (`Systran/faster-whisper-large-v3-turbo`, `Systran/faster-distil-whisper-large-v3.5`) have been gated since the catalog was written and now return HTTP 401 — the affected models can no longer be downloaded by any consumer without a new Cadmus release.

This plan inverts control of the catalog: defaults become **data the consumer passes into the constructor**, exposed via a `default_models()` function. Each file in a `ModelSpec` carries its own full URL (HTTP/HTTPS/`file://`), so the consumer can route around future gating, host their own mirrors, or air-gap entirely without depending on a Cadmus release. The default catalog also shrinks from 17 entries to **6 multilingual entries**, all sourced from `ctranslate2-4you/*` (with `preprocessor_config.json` for the smallest three coming from `openai/whisper-*` — `ctranslate2-4you` ships a 15-byte placeholder for those).

This is a **2.0** release. See "Breaking Changes" below.

**In scope:**
- New public types `ModelSpec` and `FileSpec` (owned `String` fields, full URLs).
- New top-level function `default_models()` returning the 6-model default list.
- `CadmusConfig` gains a mandatory `models: Vec<ModelSpec>` field.
- `src/storage.rs` `fetch_one` learns to read `file://` URLs.
- All `static FILES_*` consts and `static CATALOG` removed.
- `ModelInfo` loses its `repo` field; `files` stays as `Vec<String>` of filenames.
- N-API surface and TS surface mirror these changes.
- Version bumps to 2.0.0 in `Cargo.toml` and `package.json`.
- **Scope extension (Human-approved 2026-05-22, after the catalog work was
  reviewer-approved):** fix the pre-existing m4a/AAC channel-layout failure
  in `src/decode.rs` so `cargo test` and `cargo test --features napi` go
  green. Strictly additive — does not touch the catalog work. See Step 10.

**Out of scope:**
- Mirrors / fallback URLs per file. Each file has exactly one URL. A consumer who needs failover catches the error and re-attempts with a different `ModelSpec`.
- URL schemes beyond `http`, `https`, `file`. No `s3://`, no `gs://`, no `ftp://`.
- Symlink mode for `file://`. Always copy into the cache.
- Adding a `vocabulary.txt` field to `FileSpec`. It is dropped entirely from the default catalog (modern CT2 uses `tokenizer.json`).
- Per-file precision selection at runtime. Default is `float16` for all 6 models; consumers wanting `float32` or `bfloat16` register their own `ModelSpec`.
- Preserving previously-cached `*.en`, `distil-*`, `large-v1`, `large-v2` models in `list_available_models`. Their on-disk directories survive but are no longer surfaced unless the consumer registers them via a custom `ModelSpec`.

## Breaking Changes

**Yes — 2.0 release.**

What breaks:

1. `CadmusConfig` gains a mandatory `models: Vec<ModelSpec>` field. Existing code constructing `CadmusConfig` with only `model_cache` no longer compiles. Migration:
   ```rust
   // before (1.x)
   let cadmus = Cadmus::new(CadmusConfig { model_cache: "...".into() })?;
   // after (2.x)
   let cadmus = Cadmus::new(CadmusConfig {
       model_cache: "...".into(),
       models: cadmus::default_models(),
   })?;
   ```
2. `FileSpec` shape changes from `{ repo: &'static str, file: &'static str }` to `{ filename: String, url: String }`. Becomes `pub`. The `repo`-template URL pattern is gone; URLs are written in full.
3. `ModelInfo` loses its `repo: String` field. `files: Vec<String>` (filenames) stays.
4. Eleven models disappear from the default catalog: `tiny.en`, `base.en`, `small.en`, `medium.en`, `large-v1`, `large-v2`, `distil-small.en`, `distil-medium.en`, `distil-large-v2`, `distil-large-v3`, `distil-large-v3.5`. Consumers who want them register custom `ModelSpec`s.
5. Default sources change for the 6 remaining models. Previously cached models keep working (the 4–5 files written by 1.x are a superset of the 4 files the new specs list), but a fresh download pulls from `ctranslate2-4you/*` instead of `Systran/faster-whisper-*`. The float16 model.bin sizes differ from Systran's int8 sizes — `ModelSpec.size_bytes` reflects the new numbers.
6. `list_available_models` semantics change: returns the configured models, not a global catalog. An empty `models: vec![]` is valid (consumer relies on `ModelRef::Path` for everything) and yields an empty list.
7. `UnknownModel` errors now mean "not in your config", not "doesn't exist". The error code stays the same; only the wording in the message changes ("model X not configured in this Cadmus instance" instead of "unknown model X").
8. N-API / TS surface: `CadmusConfig.models` becomes a mandatory `ModelSpec[]`. New top-level export `defaultModels()`. `ModelInfo.repo` removed.

What the Human must do (release-time):
- Confirm 2.0 version bump in `Cargo.toml` and `package.json` before merge.
- Update `README.md` quickstart to show the `models: defaultModels()` pattern.
- Tag and release as 2.0.0; the release notes must call out the catalog change so consumers know to add `models: defaultModels()` to their config.

Recovery for consumers who absolutely need a removed model:
- `large-v1` / `large-v2` — still available at `Systran/faster-whisper-large-v1` / `-v2` (HEAD 200 as of this plan). Consumer adds a custom `ModelSpec` with those URLs.
- `*.en` and `distil-*` — same story; consumer constructs the `ModelSpec` from upstream HF URLs.

## Reference Patterns

- `src/catalog.rs` — current static catalog; will be largely replaced. The `ModelFamily`, `ModelInfo`, and `CatalogEntry` types are the starting point; `CatalogEntry` collapses into `ModelSpec`.
- `src/storage.rs:71` — current `download` / `fetch_one` HTTP path. The `file://` branch goes in `fetch_one` alongside the existing `ureq` call.
- `src/api.rs:86` — current `CadmusConfig` and `Cadmus`. The `models` field plumbs through to a `HashMap<String, ModelSpec>` stored on `Cadmus` for O(1) lookup.
- `src/napi.rs` — N-API bindings. Mirror the new types via `#[napi(object)]`. Existing per-file progress aggregation (line ~26 of `napi.rs`'s doc comment) keeps working: total comes from `ModelSpec.size_bytes`.
- `types.ts` — public TS surface. Add `FileSpec`, `ModelSpec`, `defaultModels()`. Update `CadmusConfig`, remove `ModelInfo.repo`.
- `tests/catalog.test.mjs` — existing catalog smoke test; rewrite to assert against `defaultModels()`.
- `tests/download.test.mjs` — existing download test; should still pass with the new `tiny` default after URL change.

## Dependencies

**None new.** `std::fs::File` covers `file://` reading. URL handling stays string-prefix-based: `url.strip_prefix("file://")` plus a Windows drive-letter quirk. No `url` crate, no `reqwest`. The existing `ureq` keeps handling `http`/`https`.

## Assumptions & Risks

- **A1:** `ctranslate2-4you/*` repos stay public for the lifetime of the 2.0 default catalog. Verified accessible at plan-writing time (HEAD 200 across all 6 models for the float16 variant). Not guaranteed long-term — the override API is the answer when (not if) this changes.
- **A2:** CTranslate2 / ct2rs accepts float16 `model.bin` files and re-quantizes on load when `ComputeType::Auto` selects int8 on CPU. This is documented CT2 behavior; verified empirically by ct2rs' existing tests.
- **A3:** `preprocessor_config.json` content for `large-v3` and `large-v3-turbo` is identical between `ctranslate2-4you/*` and `openai/whisper-*` (both 340 bytes; both contain only the minimal post-large-v3 mel-bin parameter). We pull from `ctranslate2-4you` for consistency with the model files. The 185 KB legacy form (with embedded mel tensor) is only needed for `tiny`/`base`/`small`/`medium`, where the defaults already point at the right source.
- **R1:** Old cache directories from 1.x contain `vocabulary.txt` that the new specs do not list. `ensure_present` checks that *the listed files* are present and non-empty — extra files are ignored. The stale `vocabulary.txt` wastes a few hundred KB but causes no error. We do not delete it; the cache is consumer-owned.
- **R2:** Old `*.en` / `distil-*` / `large-v1` / `large-v2` cache directories become unreachable via `list_available_models` and `find_model(name)`. `ModelRef::Path` still loads them. Mention in the migration note in `README.md`.
- **R3:** `file://` URL on Windows: `file:///C:/foo/bar.bin` after `strip_prefix("file://")` yields `/C:/foo/bar.bin`. On Windows we must detect the `/<letter>:/` pattern and drop the leading slash before passing to `Path::new`. On Unix, leave the leading `/` intact. Implementation: `cfg(windows)` branch.
- **R4:** A consumer passes a `ModelSpec` where two `FileSpec.filename` values collide. We do not de-dup or validate — the second download overwrites the first on disk. Document in the `ModelSpec` doc comment that filenames must be unique within a spec; do not add runtime validation.

## Steps

### Step 1 — New public types in `src/catalog.rs`

Replace the existing module contents with the following structure (keep `ModelFamily` and the public `ModelInfo`):

```rust
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFamily {
    Whisper,
    DistilWhisper,
}

#[derive(Debug, Clone)]
pub struct FileSpec {
    pub filename: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub family: ModelFamily,
    pub multilingual: bool,
    pub files: Vec<FileSpec>,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub family: ModelFamily,
    pub multilingual: bool,
    pub cached: bool,
    pub files: Vec<String>,  // filenames only
}

impl ModelSpec {
    pub(crate) fn to_info(&self, cache: &Path) -> ModelInfo {
        let dir = cache.join(&self.name);
        let cached = crate::storage::ensure_present_files(&self.files, &dir);
        ModelInfo {
            name: self.name.clone(),
            description: self.description.clone(),
            size_bytes: self.size_bytes,
            family: self.family.clone(),
            multilingual: self.multilingual,
            cached,
            files: self.files.iter().map(|f| f.filename.clone()).collect(),
        }
    }
}
```

Delete: `CatalogEntry`, `static CATALOG`, all `static FILES_*` constants, the `lookup` and `model_entry` free functions.

### Step 2 — `default_models()` function (in `src/catalog.rs`)

Add at the bottom of `src/catalog.rs`:

```rust
pub fn default_models() -> Vec<ModelSpec> {
    fn ct4you(model: &str, file: &str) -> FileSpec {
        FileSpec {
            filename: file.to_string(),
            url: format!(
                "https://huggingface.co/ctranslate2-4you/whisper-{model}-ct2-float16/resolve/main/{file}"
            ),
        }
    }
    fn openai_preproc(model: &str) -> FileSpec {
        FileSpec {
            filename: "preprocessor_config.json".to_string(),
            url: format!(
                "https://huggingface.co/openai/whisper-{model}/resolve/main/preprocessor_config.json"
            ),
        }
    }

    vec![
        ModelSpec {
            name: "tiny".into(),
            description: "39M-parameter Whisper, float16 CT2. Fastest; lowest accuracy. Multilingual.".into(),
            size_bytes: 78_021_061,
            family: ModelFamily::Whisper,
            multilingual: true,
            files: vec![
                ct4you("tiny", "model.bin"),
                ct4you("tiny", "config.json"),
                ct4you("tiny", "tokenizer.json"),
                openai_preproc("tiny"),
            ],
        },
        ModelSpec {
            name: "base".into(),
            description: "74M-parameter Whisper, float16 CT2. Better accuracy than tiny; still fast. Multilingual.".into(),
            size_bytes: 147_700_383,
            family: ModelFamily::Whisper,
            multilingual: true,
            files: vec![
                ct4you("base", "model.bin"),
                ct4you("base", "config.json"),
                ct4you("base", "tokenizer.json"),
                openai_preproc("base"),
            ],
        },
        ModelSpec {
            name: "small".into(),
            description: "244M-parameter Whisper, float16 CT2. Common balance of speed and accuracy. Multilingual.".into(),
            size_bytes: 486_029_792,
            family: ModelFamily::Whisper,
            multilingual: true,
            files: vec![
                ct4you("small", "model.bin"),
                ct4you("small", "config.json"),
                ct4you("small", "tokenizer.json"),
                openai_preproc("small"),
            ],
        },
        ModelSpec {
            name: "medium".into(),
            description: "769M-parameter Whisper, float16 CT2. Substantially higher accuracy; slower. Multilingual.".into(),
            size_bytes: 1_530_705_482,
            family: ModelFamily::Whisper,
            multilingual: true,
            files: vec![
                ct4you("medium", "model.bin"),
                ct4you("medium", "config.json"),
                ct4you("medium", "tokenizer.json"),
                ct4you("medium", "preprocessor_config.json"),
            ],
        },
        ModelSpec {
            name: "large-v3".into(),
            description: "1.55B-parameter Whisper, float16 CT2, current generation. Multilingual.".into(),
            size_bytes: 3_089_882_727,
            family: ModelFamily::Whisper,
            multilingual: true,
            files: vec![
                ct4you("large-v3", "model.bin"),
                ct4you("large-v3", "config.json"),
                ct4you("large-v3", "tokenizer.json"),
                ct4you("large-v3", "preprocessor_config.json"),
            ],
        },
        ModelSpec {
            name: "large-v3-turbo".into(),
            description: "809M-parameter distilled-decoder Whisper, float16 CT2. ~6x faster than large-v3 with comparable accuracy. Multilingual.".into(),
            size_bytes: 1_620_598_093,
            family: ModelFamily::Whisper,
            multilingual: true,
            files: vec![
                ct4you("large-v3-turbo", "model.bin"),
                ct4you("large-v3-turbo", "config.json"),
                ct4you("large-v3-turbo", "tokenizer.json"),
                ct4you("large-v3-turbo", "preprocessor_config.json"),
            ],
        },
    ]
}
```

Re-export at the crate root in `src/lib.rs`: `pub use catalog::{default_models, FileSpec, ModelFamily, ModelInfo, ModelSpec};`.

### Step 3 — Refactor `src/storage.rs`

Three changes:

**3a.** Drop the `FileSpec` / `ModelEntry` types from `storage.rs`. They now live in `catalog.rs`.

**3b.** Replace the `ensure_present(entry: &ModelEntry, dir: &Path) -> bool` function with `ensure_present_files(files: &[FileSpec], dir: &Path) -> bool` that iterates `FileSpec` slices instead of `ModelEntry`. Signature called from `ModelSpec::to_info` (Step 1).

**3c.** Rewrite `download` to take `&[FileSpec]` directly:

```rust
pub(crate) fn download(
    files: &[FileSpec],
    dest: &Path,
    on_progress: Option<&dyn Fn(u64, u64)>,
    cancel: Option<&AtomicBool>,
) -> Result<(), DownloadError> {
    // identical body to current `download`, but iterate `files`
    // and pass `&spec.url` to `fetch_one` instead of building
    // the Systran/openai URL inline.
}
```

**3d.** Teach `fetch_one` to handle `file://` URLs. Replace the `let mut response = ureq::get(url).call()...` block with:

```rust
let mut reader: Box<dyn Read + Send> = if let Some(path_str) = url.strip_prefix("file://") {
    let path = file_url_to_path(path_str);
    let meta = fs::metadata(&path).map_err(|e| DownloadError::Io(e.to_string()))?;
    if !meta.is_file() {
        return Err(DownloadError::Io(format!("{path:?} is not a regular file")));
    }
    let total = meta.len();
    // store `total` somewhere accessible to the progress loop below
    Box::new(File::open(&path).map_err(|e| DownloadError::Io(e.to_string()))?)
} else {
    let response = ureq::get(url).call().map_err(|e| match &e {
        ureq::Error::StatusCode(s) => DownloadError::Http(*s, String::new()),
        _ => DownloadError::Network(e.to_string()),
    })?;
    // store content_length for the progress loop
    Box::new(response.into_body().into_reader())
};
```

(The exact ownership shape depends on how `total` is plumbed — the Coder picks the cleanest factoring. Both branches must yield a reader and a `total: u64` for the existing chunked-read loop to consume unchanged.)

`file_url_to_path` is a small helper. `file://` URLs are real URLs per RFC 8089 and use percent-encoding — Node's `pathToFileURL("/tmp/my file.bin")` yields `file:///tmp/my%20file.bin`, and a literal-string `PathBuf::from(s)` would look up `my%20file.bin` on disk and fail. Decode percent-escapes before path construction:

```rust
fn percent_decode(s: &str) -> Result<String, DownloadError> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = (bytes[i + 1] as char).to_digit(16);
            let h2 = (bytes[i + 2] as char).to_digit(16);
            match (h1, h2) {
                (Some(a), Some(b)) => {
                    out.push(((a << 4) | b) as u8);
                    i += 3;
                    continue;
                }
                _ => {}  // fall through: malformed escape, pass byte through
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).map_err(|e| DownloadError::Io(format!("invalid utf-8 in file:// path: {e}")))
}

#[cfg(windows)]
fn file_url_to_path(s: &str) -> Result<PathBuf, DownloadError> {
    let decoded = percent_decode(s)?;
    // file:///C:/foo -> /C:/foo -> C:/foo
    let trimmed = decoded.strip_prefix('/').filter(|rest| {
        rest.chars().nth(1) == Some(':')
            && rest.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
    }).map(str::to_string).unwrap_or(decoded);
    Ok(PathBuf::from(trimmed))
}

#[cfg(not(windows))]
fn file_url_to_path(s: &str) -> Result<PathBuf, DownloadError> {
    Ok(PathBuf::from(percent_decode(s)?))
}
```

The plan rejects two simpler alternatives explicitly: (a) adding the `percent-encoding` crate as a dependency — out of scope per the Dependencies section, and a 15-line inline decoder is trivial; (b) narrowing the contract to raw paths — would silently break the `URL`-producing path on the JS side (Node's standard `pathToFileURL`).

The existing `.part`-file pattern, cancel check, and progress callbacks must continue to apply identically to the `file://` branch.

### Step 4 — Refactor `src/api.rs`

`CadmusConfig` gains the `models` field; `Cadmus` stores a name-indexed map for lookups:

```rust
use std::collections::HashMap;
use crate::catalog::{default_models, ModelSpec};

pub struct CadmusConfig {
    pub model_cache: PathBuf,
    pub models: Vec<ModelSpec>,
}

pub struct Cadmus {
    cache: PathBuf,
    models: HashMap<String, ModelSpec>,
}

impl Cadmus {
    pub fn new(config: CadmusConfig) -> Result<Self, CadmusError> {
        fs::create_dir_all(&config.model_cache).map_err(/* ... */)?;
        let models = config.models.into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();
        Ok(Self { cache: config.model_cache, models })
    }

    pub fn list_available_models(&self) -> Vec<ModelInfo> {
        self.models.values().map(|m| m.to_info(&self.cache)).collect()
    }

    pub fn find_model(&self, name: &str) -> Option<PathBuf> {
        let spec = self.models.get(name)?;
        let dir = self.cache.join(name);
        crate::storage::ensure_present_files(&spec.files, &dir).then_some(dir)
    }

    pub fn download_model(&self, name: &str, options: DownloadModelOptions) -> Result<PathBuf, CadmusError> {
        let spec = self.models.get(name)
            .ok_or_else(|| CadmusError::UnknownModel(name.to_string()))?;
        let dir = self.cache.join(name);
        // ... existing progress-adapter wrapping, but pass &spec.files instead of `entry`
    }

    pub fn load_model(&self, model_ref: ModelRef, options: LoadModelOptions) -> Result<CadmusModel, CadmusError> {
        let dir = match model_ref {
            ModelRef::Name(name) => {
                let spec = self.models.get(&name)
                    .ok_or_else(|| CadmusError::UnknownModel(name.clone()))?;
                let dir = self.cache.join(&name);
                if !crate::storage::ensure_present_files(&spec.files, &dir) {
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
```

`UnknownModel` error message updates to mention "not configured in this Cadmus instance" (the variant itself stays — same error code surfaces in N-API).

`list_available_models` ordering: iteration order of a `HashMap` is non-deterministic. Sort by insertion order — easiest by storing `Vec<ModelSpec>` directly and doing a linear scan for lookups (6 entries, lookup is trivially fast), or by maintaining a parallel `Vec<String>` of names. Pick the simplest: store `Vec<ModelSpec>` and scan. Drop the `HashMap` idea.

Final shape:
```rust
pub struct Cadmus {
    cache: PathBuf,
    models: Vec<ModelSpec>,
}
// lookups: self.models.iter().find(|m| m.name == name)
```

### Step 5 — N-API bindings (`src/napi.rs`)

**5a.** Add `#[napi(object)]` types mirroring `FileSpec` and `ModelSpec`:

```rust
#[napi(object)]
pub struct FileSpecJs {
    pub filename: String,
    pub url: String,
}

#[napi(object)]
pub struct ModelSpecJs {
    pub name: String,
    pub description: String,
    pub size_bytes: f64,  // JS Number; bytes fit in f64 mantissa for any realistic model
    pub family: String,    // "whisper" | "distil_whisper"
    pub multilingual: bool,
    pub files: Vec<FileSpecJs>,
}
```

(Use `f64` for `size_bytes` to match how the existing N-API binding handles other byte counts — check existing convention.)

**5b.** Update the existing `CadmusConfig` napi object to include `models: Vec<ModelSpecJs>`.

**5c.** Convert `ModelSpecJs` → `catalog::ModelSpec` in the constructor; pass through to `api::Cadmus::new`.

**5d.** Add a free-function export `defaultModels()`:

```rust
#[napi(js_name = "defaultModels")]
pub fn default_models_js() -> Vec<ModelSpecJs> {
    catalog::default_models().into_iter().map(/* convert */).collect()
}
```

**5e.** Remove the `repo` field from the napi `ModelInfo`.

**5f.** Widen the progress bridge from `u32` to `f64`. The current `ProgressTsfn` type alias (`src/napi.rs:52-53`) and the call-site clamps to `u32::MAX` (`src/napi.rs:347`, `:366`, `:525-526`) silently truncate any total above ~4 GiB. The previous static catalog never exceeded that bound; under 2.0 a consumer-registered float32 model spec (e.g. float32 `large-v3` at ~6.2 GB, or any composite spec aggregating past 4 GiB) is a legitimate config that the bridge must report correctly.

Change `ProgressTsfn` to `ThreadsafeFunction<FnArgs<(f64, f64)>, (), FnArgs<(f64, f64)>, Status, false, false, 0>`. At every emit site, convert `u64` bytes to `f64` (no clamp — `f64` is exact for integer values up to 2^53 ≈ 9 PB, far beyond any plausible model size). Update the JS-facing `onProgress` signature: it already declares `(received: number, total: number)` in `types.ts`, which is f64 — this change makes the Rust side honest. No TS surface change is needed.

**5g.** Progress aggregation semantics in `downloadModel` (per-file delta accumulated against `spec.size_bytes` total) stays as before — only the carrier width changes. The total now comes from the JS-provided spec stored on `Cadmus`, not from a static catalog.

### Step 6 — TS surface (`types.ts` and `index.ts`)

`types.ts` changes:

```ts
export interface FileSpec {
  filename: string;
  url: string;
}

export interface ModelSpec {
  name: string;
  description: string;
  sizeBytes: number;
  family: ModelFamily;
  multilingual: boolean;
  files: FileSpec[];
}

export interface CadmusConfig {
  modelCache: string;
  models: ModelSpec[];
}

export interface ModelInfo {
  name: string;
  description: string;
  sizeBytes: number;
  family: ModelFamily;
  multilingual: boolean;
  cached: boolean;
  files: string[];
  // `repo` removed
}

export declare function defaultModels(): ModelSpec[];
```

`index.ts` re-exports `defaultModels` from the napi binding.

### Step 7 — Tests

**7a.** Rewrite `tests/catalog.test.mjs` to:
- Assert `defaultModels().length === 6`.
- Assert names are exactly `['tiny', 'base', 'small', 'medium', 'large-v3', 'large-v3-turbo']` (order-stable).
- Assert all `multilingual: true`.
- Assert each spec has 4 files and includes `model.bin`, `config.json`, `tokenizer.json`, `preprocessor_config.json` in `files[].filename`.
- Assert all URLs start with `https://huggingface.co/`.

**7b.** Update `tests/download.test.mjs` to construct Cadmus with `models: defaultModels()`. The existing `tiny`-download smoke test continues to work — the URL changes but the test does not pin the URL.

**7c.** Update `tests/lifecycle.test.mjs`, `tests/transcribe.test.mjs`, `tests/_helpers/cache.mjs` and any other test helper to pass `models: defaultModels()` (or `models: []` plus a `ModelRef::Path` if the test does not need named lookup).

**7d.** Add a new test `tests/file_url.test.mjs` (or as a section in `tests/download.test.mjs`):
- Construct a temporary directory with a tiny file.
- Register a custom `ModelSpec` whose single file uses `file://<tmp>/x.bin` as URL.
- Call `downloadModel`. Assert the file appears in the cache directory and matches the source byte-for-byte.
- **Add a second case for percent-encoded paths:** stage a file at `<tmp>/with space.bin`, build the URL via Node's `pathToFileURL(...).href` (yields `file:///<tmp>/with%20space.bin`), confirm download succeeds and the cache file matches byte-for-byte. This guards `percent_decode` against silent regression.

**7e.** Update the Rust unit tests in `src/storage.rs` (the `download_tiny_smoke` test pins `crate::catalog::FILES_TINY`, which no longer exists). Replace with constructing a `Vec<FileSpec>` for `tiny` inline from `default_models()`, or feed the test a synthetic spec — whichever keeps the test focused on the download mechanics rather than the catalog contents.

**7f.** Add a Rust unit test for `file_url_to_path` and `percent_decode`: at least one Unix case (`/foo/bar` → `/foo/bar`), one percent-encoded case (`/foo/with%20space` → `/foo/with space`), one malformed-escape pass-through case (`/foo/100%`), and, gated behind `cfg(windows)`, one Windows case (`/C:/foo/bar` → `C:/foo/bar`).

**7g.** Update `tests/public_api.rs` (the integration suite that runs under `cargo test` without features — see `README.md:142`, `:188`). The file currently:
- Constructs `CadmusConfig { model_cache: ... }` in six places (`tests/public_api.rs:42`, `:58`, `:70`, `:101`, `:112`, `:156`, `:185`) — add `models: cadmus::default_models()` (or `models: vec![]` where the test never references named models).
- Asserts `models.len() == 17` (`tests/public_api.rs:73`) — update to `== 6`.
- Reads `m.repo` (`tests/public_api.rs:94`) — remove that assertion entirely since `ModelInfo.repo` no longer exists. If the test's intent was "metadata is populated", replace with an assertion that `!m.files.is_empty()` and `!m.description.is_empty()`.

**7h.** Update `src/inference.rs` test module. The current code at `:238` reads:
```rust
use crate::storage::{TINY, download, ensure_present, test_cache_dir, test_cache_lock};
```
`TINY` no longer exists. Replace by building the tiny `Vec<FileSpec>` inline from `default_models()`:
```rust
use crate::catalog::default_models;
use crate::storage::{download, ensure_present_files, test_cache_dir, test_cache_lock};
// inside the test:
let tiny = default_models().into_iter().find(|m| m.name == "tiny").unwrap();
let ready = ensure_present_files(&tiny.files, &dir) && InferenceHandle::new(&dir).is_ok();
// ...
download(&tiny.files, &dir, None, None).expect("download failed");
assert!(ensure_present_files(&tiny.files, &dir));
```
Two call sites (`:250`, `:256`) need this pattern.

### Step 8 — Docs

**8a.** `docs/architecture.md` — update the "Model storage" / "Catalog" section to describe the explicit-models pattern. Drop any mention of a static catalog.

**8b.** `docs/definition.md` — update the model list to the 6 multilingual defaults. Add one paragraph explaining that consumers can register additional models at runtime.

**8c.** `README.md` — update the quickstart to show `models: defaultModels()` in the config. Add a "Custom models" section with a short example using a custom `ModelSpec` (one file via HTTPS, one via `file://`). Update the model table to the 6 defaults with the new float16 sizes.

**8d.** No changes to `bug.kanban.md` — this plan resolves the 401-on-turbo issue without a separate bug card (the underlying source choice changes; no new accepted-deviation).

### Step 9 — Version bumps

**9a.** `Cargo.toml`: bump `version = "1.1.1"` to `version = "2.0.0"`.

**9b.** `package.json`: bump `"version"` to `"2.0.0"`.

**9c.** Verify `Cargo.lock` updates as a side effect of `cargo build`.

### Step 10 — Audio pipeline: infer AAC/m4a channels from packet spec (scope extension)

Background: `cargo test` and `cargo test --features napi` fail on `main`
in `src/decode.rs::decode_m4a_aac_to_pcm16k` and
`src/decode.rs::fixtures_have_consistent_length` with
`Decode("track lacks channel layout")`. Symphonia's `isomp4` demuxer
returns `CodecParameters.channels = None` for the AAC-LC fixture, so the
existing upfront `ok_or_else` in `decode_interleaved` rejects the stream
before any packet is decoded. The same fixture decodes correctly once a
packet is consumed: `Decoded::spec().channels` is fully populated.

This bug predates the catalog plan (introduced by commit `688f425
feat(audio): WebM/Opus + MP4/AAC-LC ...`, where the m4a test was added
without catching the failure). It was tracked as a separate bug card
under `docs/bug.kanban.md` and is now folded into this plan per Human
approval so the plan's own Verification §1–§3 can pass.

Change in `src/decode.rs::decode_interleaved`:

1. Remove the upfront `codec_params.channels.ok_or_else(...)` reject.
2. Introduce `let mut channels: Option<u16> = codec_params.channels.map(|c| c.count() as u16);`.
3. Inside the decode loop, when `channels.is_none()` and a packet successfully
   decodes, set `channels = Some(spec.channels.count() as u16)`.
4. After the loop, unwrap `channels` with a clear error
   (`AudioError::Decode("no audio packets decoded; cannot infer channels")`).

Strict additivity: nothing else in `src/decode.rs` changes. The MP3 / WAV /
FLAC / WebM-Opus paths continue to read `channels` from `codec_params` on
the first iteration so the resulting `Option` is `Some` immediately and
behavior is identical. The Opus branch is unchanged (it reads channels from
the parsed `OpusHead`, not codec-params).

No new tests required — the bug already comes with two failing tests
(`decode_m4a_aac_to_pcm16k`, `fixtures_have_consistent_length`) that turn
green with the fix. After the fix, move the bug card to `Done` in
`docs/bug.kanban.md`.

## Verification

After implementation, all of the following must pass on `darwin-arm64` (the development host):

1. `cargo fmt --check` and `cargo clippy -- -D warnings` clean.
2. `cargo test` (no features) passes. This mode runs the unit tests in `src/` plus the integration tests in `tests/public_api.rs` (gated `#![cfg(not(feature = "napi"))]`). Includes:
   - `storage::tests::download_*` (refactored to use `default_models()`'s tiny spec or a synthetic spec — must still exercise the live HF HTTPS path, the local-server progress path, and the cancel path).
   - The new `file_url_to_path` / `percent_decode` unit test.
   - The updated `tests/public_api.rs` (Step 7g).
3. **`cargo test --features napi` passes** — separate release-verification mode per `README.md:142`, `:188`. Compiles `src/napi.rs` and the napi-flavoured rlib; `tests/public_api.rs` resolves to zero tests in this mode but the unit tests in `src/` (including the napi-gated ones) all run.
4. `npm run build` succeeds (napi binding compiles, `tsc` emits `index.{js,d.ts}` + `types.{js,d.ts}`).
5. `npm test` passes. Specifically:
   - `tests/catalog.test.mjs` asserts the 6-model default list as specified in Step 7a.
   - `tests/download.test.mjs` downloads `tiny` from the new ctranslate2-4you URL successfully (network test).
   - The new `tests/file_url.test.mjs` confirms `file://` downloads land in the cache.
6. Manual smoke test, scripted in a one-off `.mjs` file (not committed):
   - Construct Cadmus with `models: defaultModels()` against a fresh cache directory.
   - `await cadmus.downloadModel('large-v3-turbo', { onProgress: (r, t) => ... })` completes without 401. Verify all 4 files exist with non-zero size in the cache dir. (This is the regression-test for the bug that motivated the plan.)
   - Verify the `onProgress` callback receives `total` that matches the spec's `sizeBytes` exactly — confirms the f64 widening (Step 5f).
7. Manual smoke test for `large-v3-turbo` transcription end-to-end with a short test audio file: `await cadmus.loadModel({ name: 'large-v3-turbo' }, ...)` then `await model.transcribe(...)` returns a non-empty `text`. Confirms the float16 model.bin from ctranslate2-4you actually works with ct2rs.
8. Manual smoke test for an empty-models config:
   - `new Cadmus({ modelCache: ..., models: [] })` succeeds.
   - `cadmus.listAvailableModels()` returns `[]`.
   - `await cadmus.loadModel({ name: 'tiny' }, ...)` rejects with `err.code === 'UnknownModel'`.
   - `await cadmus.loadModel({ path: '<existing cache dir>' }, ...)` succeeds.
9. Manual smoke test for a custom model via `file://`:
   - Stage a known-good tiny model into a local directory (e.g., copy from a previously-downloaded cache).
   - Construct a custom `ModelSpec` whose 4 files use `file:///<staged-dir>/<filename>` URLs.
   - `new Cadmus({ models: [customSpec, ...defaultModels()] })`.
   - `await cadmus.downloadModel('<customName>')` completes; cache directory matches the source byte-for-byte.
10. `cargo build --release` and `npm run build --release` (or whatever the prebuild step is) succeed — confirms the release path works for 2.0.0.

If any of items 6–9 fails, the implementation is incomplete; do not proceed to doc updates (Hard Rule 12).

<plan_ready>docs/PLAN_explicit_model_catalog.md</plan_ready>
