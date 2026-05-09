mod api;
mod catalog;
mod decode;
mod error;
mod inference;
mod storage;

#[cfg(feature = "napi")]
mod napi;

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
    pub cadmus: String,
    pub ct2rs: String,
    pub ctranslate2: String,
}

pub fn version() -> Version {
    Version {
        cadmus: env!("CARGO_PKG_VERSION").to_string(),
        ct2rs: env!("CADMUS_DEP_CT2RS_VERSION").to_string(),
        ctranslate2: env!("CADMUS_DEP_CTRANSLATE2_VERSION").to_string(),
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
