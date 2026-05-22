// Integration tests link against the rlib. With `--features napi` enabled
// the rlib references N-API runtime symbols (`_napi_throw`, …) that only
// exist inside Node's process — the standalone test binary cannot resolve
// them. Plan 6 reworks the napi surface and moves end-to-end coverage to
// `npm test`. Until then, gate this file out under `--features napi`.
#![cfg(not(feature = "napi"))]

use std::fs;
use std::path::PathBuf;

use cadmus::{
    Cadmus, CadmusConfig, CadmusError, DownloadModelOptions, LoadModelOptions, ModelFamily,
    ModelRef, TranscribeOptions, TranscriptResult, default_models, transcribe,
};

fn shared_cache() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/cadmus-test-cache")
}

fn fixture_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/eins-zwei-drei.mp3");
    fs::read(&path).unwrap_or_else(|_| panic!("fixture missing: {path:?}"))
}

fn assert_eins_zwei_drei(joined: &str) {
    let lower = joined.to_lowercase();
    let one = lower.contains("eins") || lower.contains("1");
    let two = lower.contains("zwei") || lower.contains("2");
    let three = lower.contains("drei") || lower.contains("3");
    assert!(
        one && two && three,
        "transcript missing 1/2/3 markers: {joined:?}"
    );
}

#[test]
fn cadmus_new_creates_cache_dir() {
    let temp = std::env::temp_dir().join(format!("cadmus-public-api-cache-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    let cadmus = Cadmus::new(CadmusConfig {
        model_cache: temp.clone(),
        models: vec![],
    })
    .expect("new should create the cache dir");
    assert!(temp.is_dir(), "cache dir not created");
    drop(cadmus);
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn cadmus_new_io_error_when_cache_path_blocked() {
    let blocking =
        std::env::temp_dir().join(format!("cadmus-blocking-cache-{}", std::process::id()));
    let _ = fs::remove_file(&blocking);
    let _ = fs::remove_dir_all(&blocking);
    fs::write(&blocking, b"not a dir").expect("seed blocking file");

    let result = Cadmus::new(CadmusConfig {
        model_cache: blocking.clone(),
        models: vec![],
    });
    let _ = fs::remove_file(&blocking);

    match result {
        Err(CadmusError::Io(_)) => (),
        Ok(_) => panic!("expected Io error; new() succeeded"),
        Err(e) => panic!("expected Io error; got {e:?}"),
    }
}

#[test]
fn list_available_models_returns_default_six() {
    let cadmus = Cadmus::new(CadmusConfig {
        model_cache: shared_cache(),
        models: default_models(),
    })
    .expect("new failed");
    let models = cadmus.list_available_models();
    assert_eq!(
        models.len(),
        6,
        "expected the 6 default multilingual entries"
    );

    let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "tiny",
            "base",
            "small",
            "medium",
            "large-v3",
            "large-v3-turbo"
        ],
        "default models should be returned in registration order"
    );

    for m in &models {
        assert_eq!(
            m.family,
            ModelFamily::Whisper,
            "{} should be Whisper family",
            m.name
        );
        assert!(m.multilingual, "{} should be multilingual", m.name);
        assert!(
            !m.description.is_empty(),
            "missing description on {}",
            m.name
        );
        assert!(m.size_bytes > 0, "size_bytes==0 on {}", m.name);
        assert!(!m.files.is_empty(), "no files listed for {}", m.name);
    }
}

#[test]
fn unknown_model_returns_unknown_model_error() {
    let cadmus = Cadmus::new(CadmusConfig {
        model_cache: shared_cache(),
        models: default_models(),
    })
    .expect("new failed");
    let result = cadmus.download_model("nonexistent-name", DownloadModelOptions::default());
    assert!(matches!(result, Err(CadmusError::UnknownModel(_))));
    let result = cadmus.load_model(
        ModelRef::from("nonexistent-name"),
        LoadModelOptions::default(),
    );
    assert!(matches!(result, Err(CadmusError::UnknownModel(_))));
    assert!(cadmus.find_model("nonexistent-name").is_none());
}

#[test]
fn empty_models_config_is_valid() {
    let cadmus = Cadmus::new(CadmusConfig {
        model_cache: shared_cache(),
        models: vec![],
    })
    .expect("new with empty models failed");
    assert!(cadmus.list_available_models().is_empty());
    let result = cadmus.load_model(ModelRef::from("tiny"), LoadModelOptions::default());
    assert!(matches!(result, Err(CadmusError::UnknownModel(_))));
}

#[test]
fn tiny_round_trip_via_cadmus_handle() {
    let cadmus = Cadmus::new(CadmusConfig {
        model_cache: shared_cache(),
        models: default_models(),
    })
    .expect("new failed");

    let info = cadmus
        .list_available_models()
        .into_iter()
        .find(|m| m.name == "tiny")
        .expect("tiny entry missing from configured models");
    if !info.cached {
        cadmus
            .download_model("tiny", DownloadModelOptions::default())
            .expect("download_model('tiny') failed");
    }

    let found = cadmus
        .find_model("tiny")
        .expect("find_model after download");
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
    assert_eq!(
        result.text,
        result
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>(),
        "TranscriptResult.text must be segments joined verbatim"
    );
    assert_eins_zwei_drei(&result.text);

    model.free();

    let after = model.transcribe(&fixture_bytes(), TranscribeOptions::default());
    assert!(matches!(after, Err(CadmusError::AlreadyFreed)));
}

#[test]
fn one_shot_transcribe_via_path() {
    let cadmus = Cadmus::new(CadmusConfig {
        model_cache: shared_cache(),
        models: default_models(),
    })
    .expect("new failed");
    if !cadmus
        .list_available_models()
        .iter()
        .any(|m| m.name == "tiny" && m.cached)
    {
        cadmus
            .download_model("tiny", DownloadModelOptions::default())
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
    )
    .expect("one-shot transcribe failed");

    assert!(!result.segments.is_empty());
    assert_eq!(result.language, "de");
    assert_eins_zwei_drei(&result.text);
}

#[test]
fn language_none_yields_empty_language_until_ct2rs_exposes_detection() {
    // ct2rs 0.9.18 runs language detection internally but discards the
    // detected token before returning chunks. Accepted deviation, documented
    // in docs/bug.kanban.md; tracked in docs/backlog.kanban.md. Until upstream
    // exposes it, language == None yields language = "".
    let cadmus = Cadmus::new(CadmusConfig {
        model_cache: shared_cache(),
        models: default_models(),
    })
    .expect("new failed");
    if !cadmus
        .list_available_models()
        .iter()
        .any(|m| m.name == "tiny" && m.cached)
    {
        cadmus
            .download_model("tiny", DownloadModelOptions::default())
            .expect("download tiny");
    }
    let model = cadmus
        .load_model(ModelRef::from("tiny"), LoadModelOptions::default())
        .expect("load");
    let result = model
        .transcribe(&fixture_bytes(), TranscribeOptions::default())
        .expect("transcribe");
    assert_eq!(
        result.language, "",
        "until ct2rs exposes the internally-detected language code"
    );
    assert!(
        !result.segments.is_empty(),
        "transcription itself must still work"
    );
}
