use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct FileSpec {
    pub repo: &'static str,
    pub file: &'static str,
}

pub(crate) struct ModelEntry {
    pub files: &'static [FileSpec],
}

#[derive(Debug)]
pub(crate) enum DownloadError {
    Cancelled,
    Http(u16, String),
    Network(String),
    Io(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Cancelled => write!(f, "download cancelled"),
            DownloadError::Http(s, m) => write!(f, "http {s}: {m}"),
            DownloadError::Network(m) => write!(f, "network: {m}"),
            DownloadError::Io(m) => write!(f, "io: {m}"),
        }
    }
}
impl std::error::Error for DownloadError {}

pub(crate) const TINY: ModelEntry = ModelEntry {
    files: &[
        FileSpec { repo: "Systran/faster-whisper-tiny", file: "model.bin" },
        FileSpec { repo: "Systran/faster-whisper-tiny", file: "config.json" },
        FileSpec { repo: "Systran/faster-whisper-tiny", file: "tokenizer.json" },
        FileSpec { repo: "Systran/faster-whisper-tiny", file: "vocabulary.txt" },
        FileSpec { repo: "openai/whisper-tiny",         file: "preprocessor_config.json" },
    ],
};

const CHUNK_SIZE: usize = 64 * 1024;

pub(crate) fn test_cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/cadmus-test-cache")
}

pub(crate) fn ensure_present(entry: &ModelEntry, dir: &Path) -> bool {
    entry.files.iter().all(|spec| {
        let path = dir.join(spec.file);
        fs::metadata(&path)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    })
}

pub(crate) fn download(
    entry: &ModelEntry,
    dest: &Path,
    on_progress: Option<&dyn Fn(u64, u64)>,
    cancel: Option<&AtomicBool>,
) -> Result<(), DownloadError> {
    let cancelled = || cancel.map_or(false, |c| c.load(Ordering::SeqCst));

    if cancelled() {
        return Err(DownloadError::Cancelled);
    }

    fs::create_dir_all(dest).map_err(|e| DownloadError::Io(e.to_string()))?;

    for spec in entry.files {
        let target = dest.join(spec.file);

        // Idempotency at the file level: if a previous run already
        // wrote this file with non-zero size, skip it. ensure_present()
        // (called by callers before download) covers the all-files case;
        // this guards against a partially completed earlier run.
        if let Ok(m) = fs::metadata(&target) {
            if m.is_file() && m.len() > 0 {
                continue;
            }
        }

        if cancelled() {
            return Err(DownloadError::Cancelled);
        }

        let url = format!("https://huggingface.co/{}/resolve/main/{}", spec.repo, spec.file);
        fetch_one(&url, &target, on_progress, &cancelled)?;
    }

    Ok(())
}

fn fetch_one(
    url: &str,
    target: &Path,
    on_progress: Option<&dyn Fn(u64, u64)>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), DownloadError> {
    let mut response = ureq::get(url).call().map_err(|e| match &e {
        ureq::Error::StatusCode(s) => DownloadError::Http(*s, String::new()),
        _ => DownloadError::Network(e.to_string()),
    })?;

    let total = response.body().content_length().unwrap_or(0);

    // Write to a temporary `<target>.part` so an interrupted process
    // never leaves a partially written final filename. Rename on success.
    let tmp = target.with_extension(
        target
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    );
    let cleanup = || {
        let _ = fs::remove_file(&tmp);
    };

    let file = File::create(&tmp).map_err(|e| {
        cleanup();
        DownloadError::Io(e.to_string())
    })?;
    let mut writer = BufWriter::new(file);

    let mut reader = response.body_mut().as_reader();
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut received: u64 = 0;

    loop {
        if cancelled() {
            drop(writer);
            cleanup();
            return Err(DownloadError::Cancelled);
        }

        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                drop(writer);
                cleanup();
                return Err(DownloadError::Network(e.to_string()));
            }
        };

        if let Err(e) = writer.write_all(&buf[..n]) {
            drop(writer);
            cleanup();
            return Err(DownloadError::Io(e.to_string()));
        }

        received += n as u64;
        if let Some(cb) = on_progress {
            cb(received, total);
        }
    }

    writer.flush().map_err(|e| {
        cleanup();
        DownloadError::Io(e.to_string())
    })?;
    drop(writer);

    fs::rename(&tmp, target).map_err(|e| {
        cleanup();
        DownloadError::Io(e.to_string())
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::thread;
    use std::time::Duration;

    // Live HuggingFace smoke (Concept Plan 3 "Done when": real fetch
    // path exercised end-to-end). Pulls ~75 MB on first run; subsequent
    // runs on the same target/ are instant because every file is
    // present with size > 0.
    #[test]
    fn download_tiny_smoke() {
        let dir = test_cache_dir().join("tiny");
        download(&TINY, &dir, None, None).expect("first download failed");
        assert!(ensure_present(&TINY, &dir), "files missing after download");

        // Second invocation must not re-download. Per-file fast path skips
        // each file. We can't observe the skip directly, but ensure_present
        // remains true and the call returns Ok.
        download(&TINY, &dir, None, None).expect("second download failed");
        assert!(ensure_present(&TINY, &dir));
    }

    #[test]
    fn cancel_before_call_returns_cancelled() {
        let dir = test_cache_dir().join("never-touched-cancelled-target");
        let cancel = AtomicBool::new(true);
        let result = download(&TINY, &dir, None, Some(&cancel));
        assert!(matches!(result, Err(DownloadError::Cancelled)));
        let empty_or_absent = !dir.exists()
            || dir
                .read_dir()
                .map(|mut r| r.next().is_none())
                .unwrap_or(true);
        assert!(empty_or_absent);
    }

    #[test]
    fn ensure_present_distinguishes_states() {
        let temp = test_cache_dir().join("ensure-present-fixture");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        assert!(!ensure_present(&TINY, &temp)); // missing → false

        for spec in TINY.files {
            // zero-byte → false
            File::create(temp.join(spec.file)).unwrap();
        }
        assert!(!ensure_present(&TINY, &temp));

        for spec in TINY.files {
            // size > 0 → true
            fs::write(temp.join(spec.file), b"x").unwrap();
        }
        assert!(ensure_present(&TINY, &temp));
    }

    #[test]
    fn progress_callback_against_local_server() {
        const BODY_LEN: usize = 200_000;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut req_buf = [0u8; 1024];
            let _ = stream.read(&mut req_buf);
            let body = vec![0xABu8; BODY_LEN];
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        });

        let target = test_cache_dir().join("mock-progress-target");
        let _ = fs::remove_file(&target);
        let _ = fs::create_dir_all(target.parent().unwrap());
        let url = format!("http://127.0.0.1:{port}/file.bin");

        let calls: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
        let cb = |r: u64, t: u64| {
            calls.lock().unwrap().push((r, t));
        };
        let no_cancel = || false;
        fetch_one(&url, &target, Some(&cb), &no_cancel).expect("fetch_one failed");
        server.join().unwrap();

        let log = calls.lock().unwrap().clone();
        assert!(!log.is_empty(), "no progress callbacks fired");
        let mut prev = 0u64;
        for (r, t) in &log {
            assert!(*r >= prev, "received went backwards: {prev} → {r}");
            assert_eq!(*t, BODY_LEN as u64, "total mismatch: {t}");
            assert!(*r <= *t, "received {r} exceeds total {t}");
            prev = *r;
        }
        assert_eq!(prev, BODY_LEN as u64, "final received != total");
    }

    #[test]
    fn cancel_mid_stream_against_local_server() {
        const BODY_LEN: usize = 4 * 1024 * 1024;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut req_buf = [0u8; 1024];
            let _ = stream.read(&mut req_buf);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                BODY_LEN
            );
            let _ = stream.write_all(header.as_bytes());
            for _ in 0..(BODY_LEN / (32 * 1024)) {
                if stream.write_all(&[0u8; 32 * 1024]).is_err() {
                    return;
                }
                thread::sleep(Duration::from_millis(2));
            }
        });

        let target = test_cache_dir().join("mock-cancel-target");
        let _ = fs::remove_file(&target);
        let _ = fs::create_dir_all(target.parent().unwrap());
        let url = format!("http://127.0.0.1:{port}/file.bin");

        let cancel = AtomicBool::new(false);
        let cb = |_r: u64, _t: u64| {
            cancel.store(true, Ordering::SeqCst);
        };
        let cancelled_fn = || cancel.load(Ordering::SeqCst);

        let result = fetch_one(&url, &target, Some(&cb), &cancelled_fn);
        assert!(matches!(result, Err(DownloadError::Cancelled)));

        let part = target.with_extension(
            target
                .extension()
                .map(|e| format!("{}.part", e.to_string_lossy()))
                .unwrap_or_else(|| "part".to_string()),
        );
        assert!(!part.exists(), ".part file should be deleted on cancel");
        assert!(!target.exists(), "final file should not exist on cancel");

        let _ = server.join();
    }
}
