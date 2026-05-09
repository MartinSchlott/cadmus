use std::path::Path;

use crate::storage::{ensure_present, FileSpec, ModelEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFamily {
    Whisper,
    DistilWhisper,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub family: ModelFamily,
    pub multilingual: bool,
    pub cached: bool,
    pub repo: String,
    pub files: Vec<String>,
}

pub(crate) struct CatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub size_bytes: u64,
    pub family: ModelFamily,
    pub multilingual: bool,
    pub repo: &'static str,
    pub entry: ModelEntry,
}

impl CatalogEntry {
    pub(crate) fn to_info(&self, cache: &Path) -> ModelInfo {
        let dir = cache.join(self.name);
        let cached = ensure_present(&self.entry, &dir);
        let files: Vec<String> = self
            .entry
            .files
            .iter()
            .map(|f| f.file.to_string())
            .collect();
        ModelInfo {
            name: self.name.to_string(),
            description: self.description.to_string(),
            size_bytes: self.size_bytes,
            family: self.family.clone(),
            multilingual: self.multilingual,
            cached,
            repo: self.repo.to_string(),
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
