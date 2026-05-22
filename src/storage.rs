use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::catalog::FileSpec;

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

const CHUNK_SIZE: usize = 64 * 1024;

#[cfg(test)]
pub(crate) fn test_cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/cadmus-test-cache")
}

#[cfg(test)]
static TEST_CACHE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn test_cache_lock() -> MutexGuard<'static, ()> {
    TEST_CACHE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap()
}

pub(crate) fn ensure_present_files(files: &[FileSpec], dir: &Path) -> bool {
    files.iter().all(|spec| {
        let path = dir.join(&spec.filename);
        fs::metadata(&path)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    })
}

pub(crate) fn download(
    files: &[FileSpec],
    dest: &Path,
    on_progress: Option<&dyn Fn(u64, u64)>,
    cancel: Option<&AtomicBool>,
) -> Result<(), DownloadError> {
    let cancelled = || cancel.map_or(false, |c| c.load(Ordering::SeqCst));

    if cancelled() {
        return Err(DownloadError::Cancelled);
    }

    fs::create_dir_all(dest).map_err(|e| DownloadError::Io(e.to_string()))?;

    for spec in files {
        let target = dest.join(&spec.filename);

        // Idempotency at the file level: if a previous run already
        // wrote this file with non-zero size, skip it.
        if let Ok(m) = fs::metadata(&target) {
            if m.is_file() && m.len() > 0 {
                continue;
            }
        }

        if cancelled() {
            return Err(DownloadError::Cancelled);
        }

        fetch_one(&spec.url, &target, on_progress, &cancelled)?;
    }

    Ok(())
}

fn fetch_one(
    url: &str,
    target: &Path,
    on_progress: Option<&dyn Fn(u64, u64)>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), DownloadError> {
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

    let (mut reader, total): (Box<dyn Read>, u64) =
        if let Some(path_str) = url.strip_prefix("file://") {
            let path = file_url_to_path(path_str)?;
            let meta = fs::metadata(&path).map_err(|e| DownloadError::Io(e.to_string()))?;
            if !meta.is_file() {
                return Err(DownloadError::Io(format!("{path:?} is not a regular file")));
            }
            let total = meta.len();
            let file = File::open(&path).map_err(|e| DownloadError::Io(e.to_string()))?;
            (Box::new(file), total)
        } else {
            let response = ureq::get(url).call().map_err(|e| match &e {
                ureq::Error::StatusCode(s) => DownloadError::Http(*s, String::new()),
                _ => DownloadError::Network(e.to_string()),
            })?;
            let (_parts, body) = response.into_parts();
            let total = body.content_length().unwrap_or(0);
            (Box::new(body.into_reader()), total)
        };

    let file = File::create(&tmp).map_err(|e| {
        cleanup();
        DownloadError::Io(e.to_string())
    })?;
    let mut writer = BufWriter::new(file);

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

fn percent_decode(s: &str) -> Result<String, DownloadError> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = (bytes[i + 1] as char).to_digit(16);
            let h2 = (bytes[i + 2] as char).to_digit(16);
            if let (Some(a), Some(b)) = (h1, h2) {
                out.push(((a << 4) | b) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out)
        .map_err(|e| DownloadError::Io(format!("invalid utf-8 in file:// path: {e}")))
}

#[cfg(windows)]
fn file_url_to_path(s: &str) -> Result<PathBuf, DownloadError> {
    let decoded = percent_decode(s)?;
    // file:///C:/foo -> /C:/foo -> C:/foo
    let trimmed = decoded
        .strip_prefix('/')
        .filter(|rest| {
            rest.chars().nth(1) == Some(':')
                && rest.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        })
        .map(str::to_string)
        .unwrap_or(decoded);
    Ok(PathBuf::from(trimmed))
}

#[cfg(not(windows))]
fn file_url_to_path(s: &str) -> Result<PathBuf, DownloadError> {
    Ok(PathBuf::from(percent_decode(s)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::default_models;
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::thread;
    use std::time::Duration;

    fn tiny_files() -> Vec<FileSpec> {
        default_models()
            .into_iter()
            .find(|m| m.name == "tiny")
            .expect("default_models missing tiny")
            .files
    }

    // Live HuggingFace smoke. Pulls ~75 MB on first run; subsequent runs on
    // the same target/ are instant because every file is present with size > 0.
    #[test]
    fn download_tiny_smoke() {
        let _guard = test_cache_lock();
        let dir = test_cache_dir().join("tiny");
        let files = tiny_files();
        download(&files, &dir, None, None).expect("first download failed");
        assert!(
            ensure_present_files(&files, &dir),
            "files missing after download"
        );

        // Second invocation must not re-download. Per-file fast path skips
        // each file. ensure_present_files remains true and the call returns Ok.
        download(&files, &dir, None, None).expect("second download failed");
        assert!(ensure_present_files(&files, &dir));
    }

    #[test]
    fn cancel_before_call_returns_cancelled() {
        let dir = test_cache_dir().join("never-touched-cancelled-target");
        let cancel = AtomicBool::new(true);
        let files = tiny_files();
        let result = download(&files, &dir, None, Some(&cancel));
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
        let files = tiny_files();

        assert!(!ensure_present_files(&files, &temp)); // missing → false

        for spec in &files {
            // zero-byte → false
            File::create(temp.join(&spec.filename)).unwrap();
        }
        assert!(!ensure_present_files(&files, &temp));

        for spec in &files {
            // size > 0 → true
            fs::write(temp.join(&spec.filename), b"x").unwrap();
        }
        assert!(ensure_present_files(&files, &temp));
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

    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode("/foo/bar").unwrap(), "/foo/bar");
        assert_eq!(
            percent_decode("/foo/with%20space").unwrap(),
            "/foo/with space"
        );
        // Malformed escape: pass `%` through literally.
        assert_eq!(percent_decode("/foo/100%").unwrap(), "/foo/100%");
        assert_eq!(percent_decode("/a/%C3%A9").unwrap(), "/a/é");
    }

    #[cfg(not(windows))]
    #[test]
    fn file_url_to_path_unix() {
        assert_eq!(
            file_url_to_path("/foo/bar").unwrap(),
            PathBuf::from("/foo/bar")
        );
        assert_eq!(
            file_url_to_path("/foo/with%20space").unwrap(),
            PathBuf::from("/foo/with space")
        );
    }

    #[cfg(windows)]
    #[test]
    fn file_url_to_path_windows_drive_letter() {
        assert_eq!(
            file_url_to_path("/C:/foo/bar").unwrap(),
            PathBuf::from("C:/foo/bar")
        );
        assert_eq!(
            file_url_to_path("/C:/with%20space").unwrap(),
            PathBuf::from("C:/with space")
        );
    }

    #[test]
    fn fetch_one_file_url_copies_local_file() {
        let temp = test_cache_dir().join("file-url-fixture");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();
        let src = temp.join("source.bin");
        let payload: Vec<u8> = (0u8..=255).cycle().take(8192).collect();
        fs::write(&src, &payload).unwrap();

        let target = temp.join("dest.bin");
        let url = format!("file://{}", src.to_string_lossy());
        let no_cancel = || false;
        fetch_one(&url, &target, None, &no_cancel).expect("file:// fetch failed");

        let got = fs::read(&target).unwrap();
        assert_eq!(got, payload, "file copy differs from source");
    }
}
