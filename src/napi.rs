//! napi-rs bridge — exposes the public Rust API to JavaScript via AsyncTask.
//!
//! Each long-running operation (`downloadModel`, `loadModel`, `transcribe`)
//! is dispatched on the libuv thread pool through `napi::Task`. Synchronous
//! validations (catalog lookup, ModelRef shape, AlreadyFreed) run on the JS
//! thread before the AsyncTask is constructed, so the resulting Error
//! carries the precise `code` (e.g. `UnknownModel`, `InvalidArgument`,
//! `AlreadyFreed`) instead of a generic Promise rejection.
//!
//! Error code propagation:
//!
//! - **Synchronous throws** (constructor I/O, ModelRef shape, AlreadyFreed,
//!   UnknownModel via `loadModel({ name: ... })`) call `JsError<String>::throw_into(env)`
//!   directly and return a `PendingException` sentinel — `throw_into`
//!   short-circuits when an exception is already pending, so the typed
//!   error is what JS observes.
//! - **AsyncTask rejections** (failures inside `compute()`) are routed
//!   through `Task::reject`, which builds a JS Error via raw napi calls
//!   (`napi_create_error` with the variant name as the code) and packs
//!   it into a `napi::Error` whose `maybe_raw` carries that JS Error
//!   verbatim. `JsError::into_value` returns the existing JS Error when
//!   `maybe_raw` is set, so the framework's deferred-reject path preserves
//!   our `err.code`.
//!
//! `downloadModel` progress: the napi bridge accumulates per-file progress
//! against the catalog's total size for the model, so JS sees a single
//! `(received, total)` stream with monotonic `received` and a constant
//! `total` across the call (plan §25 contract).

// Items annotated with `#[napi]` are registered with the JS module loader
// at startup but are never reached by Rust-side `cargo test`. Silence the
// resulting test-build warnings without hiding production dead code.
#![cfg_attr(test, allow(dead_code))]

use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use napi::bindgen_prelude::{
    AbortSignal, AsyncTask, Buffer, FnArgs, Function, Object, Result, Task, Unknown,
};
use napi::sys;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::{Env, JsError, Status};
use napi_derive::napi;

use crate::api;
use crate::catalog;
use crate::error::CadmusError;

type ProgressTsfn =
    ThreadsafeFunction<FnArgs<(u32, u32)>, (), FnArgs<(u32, u32)>, Status, false, false, 0>;

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn err_code(e: &CadmusError) -> &'static str {
    match e {
        CadmusError::Load(_) => "Load",
        CadmusError::Decode(_) => "Decode",
        CadmusError::Resample(_) => "Resample",
        CadmusError::Inference(_) => "Inference",
        CadmusError::Poisoned => "Poisoned",
        CadmusError::AlreadyFreed => "AlreadyFreed",
        CadmusError::UnknownModel(_) => "UnknownModel",
        CadmusError::Download(_) => "Download",
        CadmusError::Io(_) => "Io",
    }
}

/// Throw a JS Error with a custom `code` synchronously and return a
/// `PendingException` sentinel. The codegen for `#[napi]` functions calls
/// `JsError::from(e).throw_into(env)`, which is a no-op when the error's
/// status is `PendingException` — so the typed error we just threw is
/// what JS observes.
fn throw_with_code(env: Env, code: &str, msg: impl Into<String>) -> napi::Error {
    let typed: napi::Error<String> = napi::Error::new(code.to_string(), msg.into());
    unsafe { JsError::<String>::from(typed).throw_into(env.raw()) };
    napi::Error::from_status(Status::PendingException)
}

fn throw_cadmus(env: Env, e: CadmusError) -> napi::Error {
    throw_with_code(env, err_code(&e), e.to_string())
}

/// Build a JS Error with a custom `code` and pack it into a `napi::Error`
/// via the `maybe_raw` field path (`From<Unknown> for Error`). The
/// async-work codegen rejects the deferred Promise via
/// `JsError::from(err).into_value(env)`, which returns the existing JS
/// Error when `maybe_raw` is set — so JS sees `err.code === code`.
fn async_err_with_code(env_raw: sys::napi_env, code: &str, msg: &str) -> napi::Error {
    unsafe {
        let mut code_str = ptr::null_mut();
        let _ = sys::napi_create_string_utf8(
            env_raw,
            code.as_ptr().cast(),
            code.len() as isize,
            &mut code_str,
        );
        let mut msg_str = ptr::null_mut();
        let _ = sys::napi_create_string_utf8(
            env_raw,
            msg.as_ptr().cast(),
            msg.len() as isize,
            &mut msg_str,
        );
        let mut js_err = ptr::null_mut();
        let _ = sys::napi_create_error(env_raw, code_str, msg_str, &mut js_err);
        let unknown = Unknown::from_raw_unchecked(env_raw, js_err);
        napi::Error::from(unknown)
    }
}

/// Carrier for the original `CadmusError` between `compute()` and
/// `reject()` (Task's compute returns `napi::Error<Status>`, which would
/// otherwise lose the variant). Each Task stashes the typed error into
/// `Self::stash` on failure; reject() reads it.
fn stashed_async_err(env: Env, stash: &mut Option<CadmusError>, fallback: napi::Error) -> napi::Error {
    match stash.take() {
        Some(e) => async_err_with_code(env.raw(), err_code(&e), &e.to_string()),
        None => async_err_with_code(env.raw(), "GenericFailure", &fallback.reason),
    }
}

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct VersionJs {
    pub cadmus: String,
    // napi-derive auto-camelcases snake_case Rust fields. `ct2rs` would
    // otherwise emit as `ct2Rs` (`2` is a word boundary).
    #[napi(js_name = "ct2rs")]
    pub ct2rs: String,
    pub ctranslate2: String,
}

#[napi]
pub fn version() -> VersionJs {
    let v = crate::version();
    VersionJs {
        cadmus: v.cadmus,
        ct2rs: v.ct2rs,
        ctranslate2: v.ctranslate2,
    }
}

// ---------------------------------------------------------------------------
// Catalog / option types
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct CadmusConfigJs {
    pub model_cache: String,
}

#[napi(object)]
pub struct ModelInfoJs {
    pub name: String,
    pub description: String,
    /// Total bytes across all model files. JS `number` is f64; safe up to
    /// 2^53. Largest catalog entry is ~3 GB.
    pub size_bytes: f64,
    /// `"whisper"` or `"distil_whisper"`.
    pub family: String,
    pub multilingual: bool,
    pub cached: bool,
    pub repo: String,
    pub files: Vec<String>,
}

impl From<catalog::ModelInfo> for ModelInfoJs {
    fn from(m: catalog::ModelInfo) -> Self {
        let family = match m.family {
            catalog::ModelFamily::Whisper => "whisper",
            catalog::ModelFamily::DistilWhisper => "distil_whisper",
        };
        ModelInfoJs {
            name: m.name,
            description: m.description,
            size_bytes: m.size_bytes as f64,
            family: family.to_string(),
            multilingual: m.multilingual,
            cached: m.cached,
            repo: m.repo,
            files: m.files,
        }
    }
}

#[napi(object)]
pub struct ModelRefJs {
    pub name: Option<String>,
    pub path: Option<String>,
}

impl ModelRefJs {
    fn into_core(self, env: Env) -> Result<api::ModelRef> {
        match (self.name, self.path) {
            (Some(n), None) => Ok(api::ModelRef::Name(n)),
            (None, Some(p)) => Ok(api::ModelRef::Path(PathBuf::from(p))),
            (Some(_), Some(_)) => Err(throw_with_code(
                env,
                "InvalidArgument",
                "ModelRef: exactly one of `name` or `path` must be set, not both",
            )),
            (None, None) => Err(throw_with_code(
                env,
                "InvalidArgument",
                "ModelRef: exactly one of `name` or `path` must be set",
            )),
        }
    }
}

#[napi(object)]
pub struct LoadModelOptionsJs {
    pub threads: Option<u32>,
    /// One of `"auto" | "int8" | "int8_float16" | "float16" | "float32"`.
    pub compute_type: Option<String>,
}

impl LoadModelOptionsJs {
    fn into_core(self, env: Env) -> Result<api::LoadModelOptions> {
        let compute_type = match self.compute_type.as_deref() {
            None | Some("auto") => api::ComputeType::Auto,
            Some("int8") => api::ComputeType::Int8,
            Some("int8_float16") => api::ComputeType::Int8Float16,
            Some("float16") => api::ComputeType::Float16,
            Some("float32") => api::ComputeType::Float32,
            Some(other) => {
                return Err(throw_with_code(
                    env,
                    "InvalidArgument",
                    format!("unknown computeType: {other}"),
                ));
            }
        };
        Ok(api::LoadModelOptions { threads: self.threads, compute_type })
    }
}

#[napi(object)]
pub struct TranscribeOptionsJs {
    pub language: Option<String>,
    pub beam_size: Option<u32>,
}

impl From<TranscribeOptionsJs> for api::TranscribeOptions {
    fn from(o: TranscribeOptionsJs) -> Self {
        api::TranscribeOptions { language: o.language, beam_size: o.beam_size }
    }
}

#[napi(object)]
pub struct SegmentJs {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[napi(object)]
pub struct TranscriptResultJs {
    pub text: String,
    pub language: String,
    pub segments: Vec<SegmentJs>,
}

impl From<api::TranscriptResult> for TranscriptResultJs {
    fn from(r: api::TranscriptResult) -> Self {
        TranscriptResultJs {
            text: r.text,
            language: r.language,
            segments: r
                .segments
                .into_iter()
                .map(|s| SegmentJs { start: s.start as f64, end: s.end as f64, text: s.text })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Cadmus class
// ---------------------------------------------------------------------------

#[napi]
pub struct Cadmus {
    inner: Arc<api::Cadmus>,
}

#[napi]
impl Cadmus {
    #[napi(constructor)]
    pub fn new(env: Env, config: CadmusConfigJs) -> Result<Self> {
        let inner = api::Cadmus::new(api::CadmusConfig {
            model_cache: PathBuf::from(config.model_cache),
        })
        .map_err(|e| throw_cadmus(env, e))?;
        Ok(Self { inner: Arc::new(inner) })
    }

    #[napi]
    pub fn list_available_models(&self) -> Vec<ModelInfoJs> {
        self.inner
            .list_available_models()
            .into_iter()
            .map(ModelInfoJs::from)
            .collect()
    }

    #[napi]
    pub fn find_model(&self, name: String) -> Option<String> {
        self.inner
            .find_model(&name)
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// `options` is the JS-side `DownloadModelOptions` object (`{ onProgress?,
    /// signal? }`). napi-derive's auto-emitted TS type is `object | null |
    /// undefined`; the proper public shape is overridden via `ts_args_type`.
    #[napi(
        ts_args_type = "name: string, options?: { onProgress?: (received: number, total: number) => void, signal?: AbortSignal }"
    )]
    pub fn download_model(
        &self,
        env: Env,
        name: String,
        options: Option<Object<'_>>,
    ) -> Result<AsyncTask<DownloadTask>> {
        let entry = match catalog::lookup(&name) {
            Some(e) => e,
            None => {
                return Err(throw_with_code(
                    env,
                    "UnknownModel",
                    format!("unknown model: {name}"),
                ));
            }
        };

        let (on_progress, signal) = match options.as_ref() {
            Some(o) => (
                o.get::<Function<FnArgs<(u32, u32)>, ()>>("onProgress")
                    .map_err(|e| {
                        throw_with_code(env, "InvalidArgument", format!("invalid onProgress: {e}"))
                    })?,
                o.get::<AbortSignal>("signal").map_err(|e| {
                    throw_with_code(env, "InvalidArgument", format!("invalid signal: {e}"))
                })?,
            ),
            None => (None, None),
        };

        let cancel = Arc::new(AtomicBool::new(false));
        if let Some(sig) = &signal {
            let c = Arc::clone(&cancel);
            sig.on_abort(move || c.store(true, Ordering::SeqCst));
        }

        let tsfn = match on_progress {
            Some(f) => Some(
                f.build_threadsafe_function::<FnArgs<(u32, u32)>>()
                    .build()
                    .map_err(|e| {
                        throw_with_code(
                            env,
                            "InvalidArgument",
                            format!("failed to wrap onProgress: {e}"),
                        )
                    })?,
            ),
            None => None,
        };

        Ok(AsyncTask::new(DownloadTask {
            cadmus: Arc::clone(&self.inner),
            name,
            cancel,
            tsfn,
            total_size: entry.size_bytes,
            stash: None,
        }))
    }

    #[napi]
    pub fn load_model(
        &self,
        env: Env,
        model_ref: ModelRefJs,
        options: Option<LoadModelOptionsJs>,
    ) -> Result<AsyncTask<LoadTask>> {
        let core_ref = model_ref.into_core(env)?;
        if let api::ModelRef::Name(n) = &core_ref {
            if catalog::lookup(n).is_none() {
                return Err(throw_with_code(
                    env,
                    "UnknownModel",
                    format!("unknown model: {n}"),
                ));
            }
        }
        let core_opts = match options {
            Some(o) => o.into_core(env)?,
            None => api::LoadModelOptions::default(),
        };
        Ok(AsyncTask::new(LoadTask {
            cadmus: Arc::clone(&self.inner),
            model_ref: Some(core_ref),
            options: Some(core_opts),
            stash: None,
        }))
    }
}

// ---------------------------------------------------------------------------
// CadmusModel class
// ---------------------------------------------------------------------------

#[napi]
pub struct CadmusModel {
    inner: Arc<api::CadmusModel>,
    // Mirrors the underlying handle's freed flag so we can throw
    // AlreadyFreed synchronously per definition.md §5: a fresh transcribe()
    // after free() must throw before the Promise is constructed.
    freed: Arc<AtomicBool>,
}

#[napi]
impl CadmusModel {
    #[napi]
    pub fn transcribe(
        &self,
        env: Env,
        audio: Buffer,
        options: Option<TranscribeOptionsJs>,
    ) -> Result<AsyncTask<TranscribeTask>> {
        if self.freed.load(Ordering::SeqCst) {
            return Err(throw_with_code(
                env,
                "AlreadyFreed",
                "model already freed",
            ));
        }
        Ok(AsyncTask::new(TranscribeTask {
            model: Arc::clone(&self.inner),
            audio,
            options: options.map(api::TranscribeOptions::from).unwrap_or_default(),
            stash: None,
        }))
    }

    #[napi]
    pub fn free(&self) {
        self.freed.store(true, Ordering::SeqCst);
        self.inner.free();
    }
}

// One-shot transcribe (D12). No `Cadmus` handle, takes a path string.
#[napi(js_name = "transcribe")]
pub fn transcribe_oneshot(
    audio: Buffer,
    model_path: String,
    options: Option<TranscribeOptionsJs>,
) -> AsyncTask<OneShotTranscribeTask> {
    AsyncTask::new(OneShotTranscribeTask {
        audio,
        model_path: PathBuf::from(model_path),
        options: options.map(api::TranscribeOptions::from).unwrap_or_default(),
        stash: None,
    })
}

// ---------------------------------------------------------------------------
// AsyncTask implementations
//
// Each Task stashes the original CadmusError on failure so reject() can
// build a JS Error with the proper `code`. The compute() return value
// uses Status::GenericFailure as a placeholder — Task's signature locks
// errors to napi::Error<Status>, so the variant tag travels through
// `stash` instead.
// ---------------------------------------------------------------------------

pub struct DownloadTask {
    cadmus: Arc<api::Cadmus>,
    name: String,
    cancel: Arc<AtomicBool>,
    tsfn: Option<ProgressTsfn>,
    /// Catalog-level total for the model; used as the `total` argument
    /// emitted to JS so the per-file resets in `storage::download` don't
    /// leak through.
    total_size: u64,
    stash: Option<CadmusError>,
}

fn placeholder_err() -> napi::Error {
    napi::Error::new(Status::GenericFailure, String::new())
}

impl Task for DownloadTask {
    type Output = PathBuf;
    type JsValue = String;

    fn compute(&mut self) -> Result<Self::Output> {
        let tsfn = self.tsfn.take();
        let total = self.total_size;
        let cb_box: Option<Box<dyn Fn(u64, u64) + Send + Sync>> = tsfn.map(|tsfn| {
            // Per-file resets: storage::fetch_one fires (received, file_total)
            // with `received` restarting at 0 each new file. We accumulate
            // committed bytes across files and emit (cum + cur, total_size)
            // so JS sees a single monotonic stream.
            let committed = Arc::new(AtomicU64::new(0));
            let last = Arc::new(AtomicU64::new(0));
            let f: Box<dyn Fn(u64, u64) + Send + Sync> = Box::new(move |received, _file_total| {
                let prev = last.load(Ordering::SeqCst);
                if received < prev {
                    committed.fetch_add(prev, Ordering::SeqCst);
                }
                last.store(received, Ordering::SeqCst);
                let cum = committed.load(Ordering::SeqCst).saturating_add(received);
                let r = cum.min(total).min(u64::from(u32::MAX)) as u32;
                let t = total.min(u64::from(u32::MAX)) as u32;
                tsfn.call((r, t).into(), ThreadsafeFunctionCallMode::NonBlocking);
            });
            f
        });
        let opts = api::DownloadModelOptions {
            on_progress: cb_box,
            cancel: Some(Arc::clone(&self.cancel)),
        };
        match self.cadmus.download_model(&self.name, opts) {
            Ok(p) => Ok(p),
            Err(e) => {
                self.stash = Some(e);
                Err(placeholder_err())
            }
        }
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.to_string_lossy().into_owned())
    }

    fn reject(&mut self, env: Env, err: napi::Error) -> Result<Self::JsValue> {
        Err(stashed_async_err(env, &mut self.stash, err))
    }
}

pub struct LoadTask {
    cadmus: Arc<api::Cadmus>,
    // `Option` lets `compute` move the values out — neither `api::ModelRef`
    // nor `LoadModelOptions` are `Clone`.
    model_ref: Option<api::ModelRef>,
    options: Option<api::LoadModelOptions>,
    stash: Option<CadmusError>,
}

impl Task for LoadTask {
    type Output = api::CadmusModel;
    type JsValue = CadmusModel;

    fn compute(&mut self) -> Result<Self::Output> {
        let model_ref = self
            .model_ref
            .take()
            .expect("LoadTask::compute called twice");
        let opts = self.options.take().unwrap_or_default();
        match self.cadmus.load_model(model_ref, opts) {
            Ok(m) => Ok(m),
            Err(e) => {
                self.stash = Some(e);
                Err(placeholder_err())
            }
        }
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(CadmusModel {
            inner: Arc::new(output),
            freed: Arc::new(AtomicBool::new(false)),
        })
    }

    fn reject(&mut self, env: Env, err: napi::Error) -> Result<Self::JsValue> {
        Err(stashed_async_err(env, &mut self.stash, err))
    }
}

pub struct TranscribeTask {
    model: Arc<api::CadmusModel>,
    audio: Buffer,
    options: api::TranscribeOptions,
    stash: Option<CadmusError>,
}

impl Task for TranscribeTask {
    type Output = api::TranscriptResult;
    type JsValue = TranscriptResultJs;

    fn compute(&mut self) -> Result<Self::Output> {
        let opts = self.options.clone();
        match self.model.transcribe(self.audio.as_ref(), opts) {
            Ok(r) => Ok(r),
            Err(e) => {
                self.stash = Some(e);
                Err(placeholder_err())
            }
        }
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into())
    }

    fn reject(&mut self, env: Env, err: napi::Error) -> Result<Self::JsValue> {
        Err(stashed_async_err(env, &mut self.stash, err))
    }
}

pub struct OneShotTranscribeTask {
    audio: Buffer,
    model_path: PathBuf,
    options: api::TranscribeOptions,
    stash: Option<CadmusError>,
}

impl Task for OneShotTranscribeTask {
    type Output = api::TranscriptResult;
    type JsValue = TranscriptResultJs;

    fn compute(&mut self) -> Result<Self::Output> {
        match api::transcribe(self.audio.as_ref(), &self.model_path, self.options.clone()) {
            Ok(r) => Ok(r),
            Err(e) => {
                self.stash = Some(e);
                Err(placeholder_err())
            }
        }
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into())
    }

    fn reject(&mut self, env: Env, err: napi::Error) -> Result<Self::JsValue> {
        Err(stashed_async_err(env, &mut self.stash, err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn err_code_covers_every_variant() {
        for (e, expected) in [
            (CadmusError::Load("x".into()), "Load"),
            (CadmusError::Decode("x".into()), "Decode"),
            (CadmusError::Resample("x".into()), "Resample"),
            (CadmusError::Inference("x".into()), "Inference"),
            (CadmusError::Poisoned, "Poisoned"),
            (CadmusError::AlreadyFreed, "AlreadyFreed"),
            (CadmusError::UnknownModel("x".into()), "UnknownModel"),
            (CadmusError::Download("x".into()), "Download"),
            (CadmusError::Io("x".into()), "Io"),
        ] {
            assert_eq!(err_code(&e), expected);
        }
    }

    #[test]
    fn model_info_js_family_round_trip() {
        let m = catalog::ModelInfo {
            name: "tiny".into(),
            description: "x".into(),
            size_bytes: 1,
            family: catalog::ModelFamily::Whisper,
            multilingual: true,
            cached: false,
            repo: "r".into(),
            files: vec!["f".into()],
        };
        let js: ModelInfoJs = m.into();
        assert_eq!(js.family, "whisper");

        let m2 = catalog::ModelInfo {
            name: "distil-small.en".into(),
            description: "x".into(),
            size_bytes: 2,
            family: catalog::ModelFamily::DistilWhisper,
            multilingual: false,
            cached: true,
            repo: "r".into(),
            files: vec!["f".into()],
        };
        let js2: ModelInfoJs = m2.into();
        assert_eq!(js2.family, "distil_whisper");
    }
}
