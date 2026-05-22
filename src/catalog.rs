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

/// Consumer-supplied model description. `filename`s must be unique within a
/// spec — duplicates silently overwrite each other on disk.
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
    pub files: Vec<String>,
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
