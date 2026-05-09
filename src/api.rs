use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use ct2rs::{ComputeType as Ct2ComputeType, Config as Ct2Config, WhisperOptions};

use crate::catalog::{model_entry, ModelInfo, CATALOG};
use crate::decode::decode_to_pcm16k;
use crate::error::CadmusError;
use crate::inference::{InferenceHandle, InferenceOutput};
use crate::storage::{self, ensure_present};

#[derive(Default)]
pub struct LoadModelOptions {
    pub threads: Option<u32>,
    pub compute_type: ComputeType,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputeType {
    #[default]
    Auto,
    Int8,
    Int8Float16,
    Float16,
    Float32,
}

impl ComputeType {
    fn to_ct2(self) -> Ct2ComputeType {
        match self {
            Self::Auto => Ct2ComputeType::AUTO,
            Self::Int8 => Ct2ComputeType::INT8,
            Self::Int8Float16 => Ct2ComputeType::INT8_FLOAT16,
            Self::Float16 => Ct2ComputeType::FLOAT16,
            Self::Float32 => Ct2ComputeType::FLOAT32,
        }
    }
}

#[derive(Default, Clone)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub beam_size: Option<u32>,
}

#[derive(Default)]
pub struct DownloadModelOptions {
    pub on_progress: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
    pub cancel: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text: String,
    pub language: String,
    pub segments: Vec<Segment>,
}

pub enum ModelRef {
    Name(String),
    Path(PathBuf),
}

impl From<&str> for ModelRef {
    fn from(s: &str) -> Self { Self::Name(s.to_string()) }
}
impl From<String> for ModelRef {
    fn from(s: String) -> Self { Self::Name(s) }
}
impl From<&Path> for ModelRef {
    fn from(p: &Path) -> Self { Self::Path(p.to_path_buf()) }
}
impl From<PathBuf> for ModelRef {
    fn from(p: PathBuf) -> Self { Self::Path(p) }
}

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
        let cancel: Option<&AtomicBool> = options.cancel.as_deref();
        let result = match options.on_progress.as_deref() {
            Some(cb) => {
                let adapter = move |r: u64, t: u64| cb(r, t);
                storage::download(entry, &dir, Some(&adapter), cancel)
            }
            None => storage::download(entry, &dir, None, cancel),
        };
        result.map_err(CadmusError::from)?;
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
    let segments: Vec<Segment> = out
        .segments
        .into_iter()
        .map(|s| Segment { start: s.start, end: s.end, text: s.text })
        .collect();
    let text: String = segments.iter().map(|s| s.text.as_str()).collect();
    let language = options
        .language
        .clone()
        .or(out.detected_language)
        .unwrap_or_default();
    Ok(TranscriptResult { text, language, segments })
}

pub fn transcribe(
    audio: &[u8],
    model_path: &Path,
    options: TranscribeOptions,
) -> Result<TranscriptResult, CadmusError> {
    let handle = InferenceHandle::new(model_path).map_err(CadmusError::from)?;
    transcribe_with_handle(&handle, audio, options)
}
