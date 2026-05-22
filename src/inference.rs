use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ct2rs::{Config, Whisper, WhisperOptions};

#[derive(Debug)]
pub(crate) enum InferenceError {
    Load(String),
    Generate(String),
    AlreadyFreed,
    Poisoned,
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceError::Load(m) => write!(f, "load: {m}"),
            InferenceError::Generate(m) => write!(f, "generate: {m}"),
            InferenceError::AlreadyFreed => write!(f, "model already freed"),
            InferenceError::Poisoned => write!(f, "internal lock poisoned by a panic"),
        }
    }
}
impl std::error::Error for InferenceError {}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Segment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

#[derive(Debug)]
pub(crate) struct InferenceOutput {
    pub segments: Vec<Segment>,
    pub detected_language: Option<String>,
}

/// Internal Whisper handle. Implements D4: `Arc<Whisper>` plus an atomic
/// `freed` sentinel. The `Mutex<Option<...>>` exists only so `free()` can
/// swap the owning Arc out atomically — the critical section spans
/// freed-check + `Arc::clone` (or `take()` in `free()`), never the actual
/// call to `Whisper::generate`.
pub(crate) struct InferenceHandle {
    inner: Mutex<Option<Arc<Whisper>>>,
    freed: AtomicBool,
}

impl InferenceHandle {
    pub(crate) fn new(model_dir: &Path) -> Result<Self, InferenceError> {
        Self::new_with_config(model_dir, Config::default())
    }

    pub(crate) fn new_with_config(
        model_dir: &Path,
        config: Config,
    ) -> Result<Self, InferenceError> {
        let whisper =
            Whisper::new(model_dir, config).map_err(|e| InferenceError::Load(e.to_string()))?;
        Ok(Self {
            inner: Mutex::new(Some(Arc::new(whisper))),
            freed: AtomicBool::new(false),
        })
    }

    #[cfg(test)]
    pub(crate) fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
    ) -> Result<Vec<Segment>, InferenceError> {
        self.transcribe_with_options(samples, language, &WhisperOptions::default())
            .map(|out| out.segments)
    }

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
        Ok(InferenceOutput {
            segments,
            detected_language,
        })
    }

    pub(crate) fn free(&self) {
        self.freed.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }
}

impl Drop for InferenceHandle {
    fn drop(&mut self) {
        self.free();
    }
}

const CHUNK_SECONDS: f32 = 30.0;

pub(crate) fn parse_segments(chunks: &[String]) -> Vec<Segment> {
    let mut out = Vec::new();
    for (idx, chunk) in chunks.iter().enumerate() {
        let offset = idx as f32 * CHUNK_SECONDS;
        parse_one_chunk(chunk, offset, &mut out);
    }
    out
}

fn parse_one_chunk(chunk: &str, offset: f32, out: &mut Vec<Segment>) {
    let bytes = chunk.as_bytes();
    let mut last_ts: Option<f32> = None;
    let mut pending_text = String::new();
    let mut text_start: usize = 0;
    let mut i: usize = 0;

    while i < bytes.len() {
        if bytes[i] == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
            if let Some(end) = find_token_end(bytes, i + 2) {
                // Token spans bytes i..end+2 (inclusive of `|>`).
                // Push any literal text accumulated since text_start.
                if i > text_start {
                    pending_text.push_str(&chunk[text_start..i]);
                }
                let tok = &chunk[i + 2..end];
                if let Ok(secs) = tok.parse::<f32>() {
                    if !pending_text.trim().is_empty() {
                        let start = last_ts.unwrap_or(0.0);
                        out.push(Segment {
                            start: offset + start,
                            end: offset + secs,
                            text: std::mem::take(&mut pending_text),
                        });
                    } else {
                        pending_text.clear();
                    }
                    last_ts = Some(secs);
                } else {
                    // Non-timestamp control token (e.g. <|de|>, <|transcribe|>);
                    // drop entirely.
                }
                i = end + 2;
                text_start = i;
                continue;
            }
            // Malformed: no closing `|>`. Treat the `<` as literal text and
            // advance one byte. text_start stays put so the `<` ends up in
            // the next slice.
            i += 1;
            continue;
        }
        i += 1;
    }

    if bytes.len() > text_start {
        pending_text.push_str(&chunk[text_start..]);
    }

    if !pending_text.trim().is_empty() {
        let start = last_ts.unwrap_or(0.0);
        out.push(Segment {
            start: offset + start,
            end: offset + start,
            text: pending_text,
        });
    }
}

/// Scan chunks for Whisper's `<|xx|>` language control token. ct2rs runs
/// detection internally when `language = None` and emits the detected code
/// as the first control token of the first chunk. The token body is a 2-
/// or 3-character ASCII-lowercase ISO 639 code (e.g. `<|de|>`, `<|en|>`).
/// Other control tokens (`<|transcribe|>`, `<|translate|>`,
/// `<|notimestamps|>`, `<|startoftranscript|>`, `<|endoftext|>`) exceed
/// 3 chars; timestamp tokens (`<|0.00|>`) contain `.` and digits.
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

fn find_token_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut j = from;
    while j + 1 < bytes.len() {
        if bytes[j] == b'|' && bytes[j + 1] == b'>' {
            return Some(j);
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{FileSpec, default_models};
    use crate::decode::decode_to_pcm16k;
    use crate::storage::{download, ensure_present_files, test_cache_dir, test_cache_lock};

    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc as StdArc;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn tiny_files() -> Vec<FileSpec> {
        default_models()
            .into_iter()
            .find(|m| m.name == "tiny")
            .expect("default_models missing tiny")
            .files
    }

    fn ensure_tiny() -> PathBuf {
        let _guard = test_cache_lock();
        let dir = test_cache_dir().join("tiny");
        let files = tiny_files();
        let ready = ensure_present_files(&files, &dir) && InferenceHandle::new(&dir).is_ok();
        if !ready {
            let _ = fs::remove_dir_all(&dir);
            download(&files, &dir, None, None).expect("staging tiny failed");
            InferenceHandle::new(&dir).expect("load staged tiny");
        }
        assert!(ensure_present_files(&files, &dir));
        dir
    }

    fn fixture_bytes() -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/eins-zwei-drei.mp3");
        fs::read(&path).unwrap_or_else(|_| panic!("fixture missing: {:?}", path))
    }

    fn fixture_samples() -> Vec<f32> {
        decode_to_pcm16k(&fixture_bytes()).expect("decode fixture failed")
    }

    fn long_samples(repeat: usize) -> Vec<f32> {
        let one = fixture_samples();
        let mut out = Vec::with_capacity(one.len() * repeat);
        for _ in 0..repeat {
            out.extend_from_slice(&one);
        }
        out
    }

    // tiny normalises spoken numbers to digits at will (e.g. "1, 2, 3" instead
    // of "eins, zwei, drei"). Accept either form per number.
    fn assert_eins_zwei_drei(joined: &str) {
        let one = joined.contains("eins") || joined.contains("1");
        let two = joined.contains("zwei") || joined.contains("2");
        let three = joined.contains("drei") || joined.contains("3");
        assert!(
            one && two && three,
            "transcript missing 1/2/3 markers: {joined:?}"
        );
    }

    #[test]
    fn end_to_end_eins_zwei_drei() {
        let dir = ensure_tiny();
        let handle = InferenceHandle::new(&dir).expect("load tiny");
        let samples = fixture_samples();

        let segments = handle
            .transcribe(&samples, Some("de"))
            .expect("transcribe failed");

        assert!(!segments.is_empty(), "no segments parsed");
        let joined: String = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .concat()
            .to_lowercase();
        assert_eins_zwei_drei(&joined);

        for seg in &segments {
            assert!(seg.end >= seg.start, "segment end < start: {seg:?}");
        }
    }

    #[test]
    fn eins_zwei_drei_via_webm() {
        let dir = ensure_tiny();
        let handle = InferenceHandle::new(&dir).expect("load tiny");
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/eins-zwei-drei.webm");
        let bytes = fs::read(&path).unwrap_or_else(|_| panic!("fixture missing: {:?}", path));
        let samples = decode_to_pcm16k(&bytes).expect("decode webm fixture failed");

        let segments = handle
            .transcribe(&samples, Some("de"))
            .expect("transcribe failed");

        assert!(!segments.is_empty(), "no segments parsed");
        let joined: String = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .concat()
            .to_lowercase();
        assert_eins_zwei_drei(&joined);
    }

    #[test]
    fn transcribe_after_free_returns_already_freed() {
        let dir = ensure_tiny();
        let handle = InferenceHandle::new(&dir).unwrap();

        handle.free();
        handle.free();

        let samples = fixture_samples();
        let result = handle.transcribe(&samples, Some("de"));
        assert!(matches!(result, Err(InferenceError::AlreadyFreed)));
    }

    #[test]
    fn free_during_inflight_completes_normally() {
        let dir = ensure_tiny();
        let handle = StdArc::new(InferenceHandle::new(&dir).unwrap());

        let long = long_samples(10);

        let (tx, rx) = mpsc::channel::<()>();
        let h2 = StdArc::clone(&handle);
        let worker = thread::spawn(move || {
            tx.send(()).unwrap();
            h2.transcribe(&long, Some("de"))
        });

        rx.recv().expect("worker dropped before signalling");
        thread::sleep(Duration::from_millis(50));

        handle.free();

        let result = worker.join().expect("worker panicked");
        let segments = result.expect("in-flight transcribe must complete normally");
        assert!(!segments.is_empty(), "in-flight result empty");
        let joined: String = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .concat()
            .to_lowercase();
        assert_eins_zwei_drei(&joined);

        let samples = fixture_samples();
        let after = handle.transcribe(&samples, Some("de"));
        assert!(matches!(after, Err(InferenceError::AlreadyFreed)));
    }

    #[test]
    fn concurrent_transcribe_succeeds() {
        let dir = ensure_tiny();
        let handle = StdArc::new(InferenceHandle::new(&dir).unwrap());

        let h_a = StdArc::clone(&handle);
        let h_b = StdArc::clone(&handle);
        let a = thread::spawn(move || h_a.transcribe(&fixture_samples(), Some("de")));
        let b = thread::spawn(move || h_b.transcribe(&fixture_samples(), Some("de")));

        let ra = a
            .join()
            .expect("thread a panicked")
            .expect("a transcribe failed");
        let rb = b
            .join()
            .expect("thread b panicked")
            .expect("b transcribe failed");

        assert!(!ra.is_empty());
        assert!(!rb.is_empty());
        let txt_a: String = ra
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .concat()
            .to_lowercase();
        let txt_b: String = rb
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .concat()
            .to_lowercase();
        assert_eins_zwei_drei(&txt_a);
        assert_eins_zwei_drei(&txt_b);
    }

    #[test]
    fn parse_segments_two_segments_one_chunk() {
        let chunks = vec!["<|0.00|> Hello.<|2.50|> World.<|5.00|>".to_string()];
        let segs = parse_segments(&chunks);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].start, 0.0);
        assert_eq!(segs[0].end, 2.5);
        assert!(segs[0].text.contains("Hello"));
        assert_eq!(segs[1].start, 2.5);
        assert_eq!(segs[1].end, 5.0);
        assert!(segs[1].text.contains("World"));
    }

    #[test]
    fn parse_segments_two_chunks_offsets_correctly() {
        let chunks = vec![
            "<|0.00|> First.<|10.00|>".to_string(),
            "<|0.00|> Second.<|5.00|>".to_string(),
        ];
        let segs = parse_segments(&chunks);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].start, 0.0);
        assert_eq!(segs[0].end, 10.0);
        assert_eq!(segs[1].start, 30.0);
        assert_eq!(segs[1].end, 35.0);
    }

    #[test]
    fn parse_segments_no_timestamps_emits_text_at_zero() {
        let chunks = vec!["just text, no tokens".to_string()];
        let segs = parse_segments(&chunks);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start, 0.0);
        assert_eq!(segs[0].end, 0.0);
        assert!(segs[0].text.contains("just text"));
    }

    #[test]
    fn parse_segments_drops_control_tokens_keeps_utf8_text() {
        let chunks = vec!["<|de|><|transcribe|><|0.00|> grüß dich.<|1.20|>".to_string()];
        let segs = parse_segments(&chunks);
        assert_eq!(segs.len(), 1);
        assert!(
            segs[0].text.contains("grüß"),
            "umlaut lost: {:?}",
            segs[0].text
        );
    }

    #[test]
    fn parse_segments_malformed_token_treated_as_text() {
        let chunks = vec!["<|0.00|> ok <| not a token <|1.00|>".to_string()];
        let segs = parse_segments(&chunks);
        assert_eq!(segs.len(), 1);
        assert!(segs[0].text.contains("ok"));
    }

    #[test]
    fn detect_language_from_chunks_finds_two_letter_token() {
        let chunks = vec!["<|de|><|transcribe|><|0.00|> hallo welt<|2.50|>".to_string()];
        assert_eq!(detect_language_from_chunks(&chunks), Some("de".to_string()));
    }

    #[test]
    fn detect_language_from_chunks_skips_control_and_timestamp_tokens() {
        let chunks = vec!["<|transcribe|><|0.00|> just text<|1.00|>".to_string()];
        assert_eq!(detect_language_from_chunks(&chunks), None);
    }

    #[test]
    fn detect_language_from_chunks_empty_input_returns_none() {
        let chunks: Vec<String> = vec![];
        assert_eq!(detect_language_from_chunks(&chunks), None);
    }
}
