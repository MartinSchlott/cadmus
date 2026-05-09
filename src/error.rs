use crate::decode::AudioError;
use crate::inference::InferenceError;
use crate::storage::DownloadError;

#[derive(Debug)]
pub enum CadmusError {
    Load(String),
    Decode(String),
    Resample(String),
    Inference(String),
    Poisoned,
    AlreadyFreed,
    UnknownModel(String),
    Download(String),
    Io(String),
}

impl std::fmt::Display for CadmusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(m) => write!(f, "load: {m}"),
            Self::Decode(m) => write!(f, "decode: {m}"),
            Self::Resample(m) => write!(f, "resample: {m}"),
            Self::Inference(m) => write!(f, "inference: {m}"),
            Self::Poisoned => write!(f, "internal lock poisoned"),
            Self::AlreadyFreed => write!(f, "model already freed"),
            Self::UnknownModel(n) => write!(f, "unknown model: {n}"),
            Self::Download(m) => write!(f, "download: {m}"),
            Self::Io(m) => write!(f, "io: {m}"),
        }
    }
}
impl std::error::Error for CadmusError {}

impl From<AudioError> for CadmusError {
    fn from(e: AudioError) -> Self {
        match e {
            AudioError::Decode(m) => Self::Decode(m),
            AudioError::Resample(m) => Self::Resample(m),
        }
    }
}

impl From<InferenceError> for CadmusError {
    fn from(e: InferenceError) -> Self {
        match e {
            InferenceError::Load(m) => Self::Load(m),
            InferenceError::Generate(m) => Self::Inference(m),
            InferenceError::Poisoned => Self::Poisoned,
            InferenceError::AlreadyFreed => Self::AlreadyFreed,
        }
    }
}

impl From<DownloadError> for CadmusError {
    fn from(e: DownloadError) -> Self {
        Self::Download(e.to_string())
    }
}
