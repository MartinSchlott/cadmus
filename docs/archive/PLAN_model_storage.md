# PLAN_model_storage — Crate-internal HuggingFace downloader

Plan #3 of CONCEPT_v1_buildout. Introduces the HTTP fetch path that turns a HuggingFace model repo into a populated directory on disk. Crate-internal only — no public API change. Plan 4 (`PLAN_inference_core`) consumes this to stage a `tiny` model for its end-to-end transcription test; Plan 5 (`PLAN_public_api`) re-exposes the same machinery as `cadmus.download_model`.

Reference: [CONCEPT_v1_buildout.md](CONCEPT_v1_buildout.md), in particular **D5** (audio pipeline already done), **D6** (model directory layout — minimal `tiny` filelist established here, full 17-entry catalog comes in `PLAN_public_api`), **D11** (explicit cache directories), **D27** (packaging allowlists), and the Plan 3 row of the Plan Breakdown.

---

## Context & Goal

CONCEPT_v1_buildout's revised Plan Breakdown places this plan before `PLAN_inference_core` so the inference test has an honest path to a model on disk — without committing 75 MB of binary artefact to the repo and without writing throwaway code that the catalog plan would later replace.

After this plan:

- A crate-internal module `src/storage.rs` owns the HuggingFace download path.
- Internal data structures `pub(crate) struct FileSpec { repo: &'static str, file: &'static str }` and `pub(crate) struct ModelEntry { files: &'static [FileSpec] }`, plus exactly one `pub(crate) const TINY: ModelEntry` (see Step 4). Files within one model may originate from different HuggingFace repos because the CT2-converted Faster-Whisper repos do not ship `preprocessor_config.json` — that file is sourced from `openai/whisper-tiny`, matching the canonical `ct2-transformers-converter --copy_files preprocessor_config.json tokenizer.json` workflow documented in ct2rs's whisper example.
- Internal function `pub(crate) fn download(entry: &ModelEntry, dest: &Path, on_progress: Option<&dyn Fn(u64, u64)>, cancel: Option<&AtomicBool>) -> Result<(), DownloadError>`. Synchronous, blocking, no async runtime. All files for one entry land in the same `dest` directory regardless of source repo (ct2rs expects a flat model dir). Streams file contents in 64 KiB chunks, calls the progress callback after each chunk, polls the cancel flag between chunks. On cancel, deletes the partial file and returns `DownloadError::Cancelled`.
- Internal helper `pub(crate) fn ensure_present(entry: &ModelEntry, dir: &Path) -> bool` — returns `true` only if every `FileSpec.file` exists at `dir/file` with non-zero size. Same semantics as D19's `cached` detection (`PLAN_public_api` will surface this as `ModelInfo::cached`).
- Internal helper `pub(crate) fn test_cache_dir() -> PathBuf` — returns `<CARGO_MANIFEST_DIR>/target/cadmus-test-cache`, the deterministic gitignored cache root used by tests in this and later plans.
- Crate-internal tests covering, in line with [architecture.md §8.2](architecture.md) ("mock HTTP, verify directory structure written, progress callback invoked, cancellation flag respected") plus the Concept's Plan 3 "Done when" live-fetch criterion:
  - Live HuggingFace smoke download of `TINY` into the test cache (idempotent — second invocation skips the network entirely). Honours the Concept's "first run downloads tiny" criterion.
  - Pre-set cancel flag short-circuits before any HTTP traffic.
  - `ensure_present` correctly distinguishes complete / missing / zero-byte states.
  - **Mock-server-based progress callback test** — stdlib `std::net::TcpListener` serves a known-size payload; assertions cover monotonicity within a file, `received <= total` whenever `total > 0`, and final `received == total`.
  - **Mock-server-based mid-stream cancel test** — stdlib `TcpListener` streams the body in chunks; the test sets the cancel flag from inside the progress callback after the first chunk. The download function returns `Cancelled` and the `.part` file is deleted. This is the cancel scenario architecture.md §8.2 names but the live-only test variant cannot exercise without flake.

What this plan does **not** do:

- Public API. No new `pub` items in `src/lib.rs`. `cadmus::version()` remains the only public function.
- Catalog. `TINY` is the only entry; the full 17-entry list with `description` / `family` / `multilingual` / `cached` / `description` lives in `PLAN_public_api`.
- HTTP Range / resume. Backlog card "HTTP Range / resume on `download_model`" tracks this.
- `find_model`. Filesystem search across multiple cache directories belongs to `PLAN_public_api`.
- Integrity verification. Definition.md §5 explicitly excludes this; a truncated download surfaces later as a `Load` error.

## Breaking Changes

**None.** Additive only — new module, one new dependency (`ureq`, see Dependencies), one new gitignore entry, no existing source modified beyond a one-line `mod storage;` addition in `src/lib.rs`.

## Reference Patterns

- **`src/decode.rs`** (committed by `PLAN_audio_pipeline`) — same shape as the new `src/storage.rs`: crate-private module with `pub(crate)` items, `#[cfg(test)]` tests appended to the same file, `#![allow(dead_code)]` until the next plan consumes the module. Plan 4 removes both `#![allow(dead_code)]` lines (in `decode.rs` and `storage.rs`) when its end-to-end test consumes both.
- **`ureq` streaming download** — the canonical pattern is `ureq::get(url).call()?.body_mut().as_reader()` returning a `BodyReader: impl Read`. Read in fixed-size chunks via `std::io::Read::read`. `response.body().content_length() -> Option<u64>` provides the denominator for the progress callback.

## Dependencies

Approved by Human. **No additions during implementation without escalation (Hard Rule 11).** Concrete versions, looked up against the local cargo cache on **2026-05-08** (plan-write date) and pinned exactly. Same pinning discipline as ct2rs/symphonia/rubato in earlier plans.

**Rust (`Cargo.toml [dependencies]`):**

| Crate | Version | Role / features |
|---|---|---|
| `ureq` | `=3.3.0` | Synchronous HTTPS client. `default-features = false, features = ["rustls", "platform-verifier"]`. Pure-Rust TLS via rustls (no OpenSSL build dependency, matches the project's "no system libs" promise from definition.md §3). `gzip` / `brotli` / `cookies` / `json` deliberately not enabled — model files are already binary and ship without compression negotiation, and we make no JSON or cookie-bearing requests. |

If `ureq 3.3.0` is unavailable on crates.io at implementation time, or its API differs from the calls used in Step 5 (e.g. `body_mut()` / `content_length()` / `as_reader()` renamed or relocated), the Coder stops and reports — replacing or upgrading a pinned dependency is a plan-level decision, not an implementation one (Rule 11 / Rule 7).

No npm dependencies change. No transitive runtime cost on the npm side.

### License impact

`ureq` is dual-licensed `MIT OR Apache-2.0`. Project's MIT `LICENSE` covers the binary as a whole; no new entry in `LICENSE-THIRD-PARTY` is required (that file currently lists symphonia for its MPL-2.0 obligation; permissive licences without file-scoped copyleft do not need separate attribution beyond the project licence).

## Assumptions & Risks

- **A1.** HuggingFace serves `Systran/faster-whisper-tiny`'s and `openai/whisper-tiny`'s files at `https://huggingface.co/{repo}/resolve/main/{file}` and follows up to 5 redirects to a CDN that supports `Content-Length`. ureq follows redirects by default. If HF restructures URLs or rate-limits anonymous pulls (R2 in the concept), the smoke test fails on first run and the Coder reports — this is a plan-level surprise.
- **A2.** The test runs on a host with network access. There is no offline-only test mode in v1. Developers running `cargo test` for the first time on this branch download ~75 MB; subsequent runs skip the download by virtue of `ensure_present`. CI matrices that explicitly forbid outbound network are out of scope for v1 (D25: no CI in v1).
- **A3.** `target/` lives under the manifest directory because Cadmus is a single-crate project (D22) and no `[build.target-dir]` override exists. `env!("CARGO_MANIFEST_DIR")` joined with `target/cadmus-test-cache` is therefore a stable, gitignored path. If a developer overrides `CARGO_TARGET_DIR`, the cache still lives next to `Cargo.toml`, not next to the override target — that is acceptable as a v1 simplification; future plans can switch to `OUT_DIR`-based discovery if needed.
- **R1.** The five `tiny` `FileSpec`s (Step 4) are pinned by `(repo, file)`, not enumerated dynamically against the HF API. If either repo changes the filename layout (e.g. `model.bin` → `model.safetensors`, or `openai/whisper-tiny` retiring `preprocessor_config.json`), the smoke test fails fast with a 404, the Coder stops and reports. This is the same exposure as R2 in CONCEPT_v1_buildout and accepted at concept level. Splitting across two repos slightly broadens the surface — but the alternative (committing `preprocessor_config.json` into the cadmus tree) drifts against upstream Whisper updates and was rejected on plan amendment.
- **R2.** Network flakiness during the first-run download produces a partially written file under `target/cadmus-test-cache/...`. The download function deletes its in-progress file on any error before returning — but if the process is killed mid-write (SIGKILL, power loss), a stale partial may remain. `ensure_present` returns `false` if any file has size zero, but a non-zero partial of `model.bin` would falsely report `true`. Mitigation: `ensure_present` only considers presence and non-zero size, not byte exactness. A truncated `model.bin` would surface as a `Load` error in Plan 4's test — i.e., the inference plan, not this one. Accepted.
- **R3.** rustls + platform-verifier on macOS arm64 pulls in a small chain of pure-Rust crypto crates (`rustls`, `webpki`, `rustls-platform-verifier`, etc.). All MIT/Apache-2.0. No native dependency. First `cargo build` after this plan compiles them once (~10–20 s); subsequent builds use cache.
- **R4.** `cargo` may emit `unused` warnings on the new module since no caller exists yet; suppressed by `#![allow(dead_code)]` at the top of `src/storage.rs` exactly like `src/decode.rs`. Plan 4 removes both opt-outs together when its end-to-end test consumes both modules.

No new `severity: accepted` bug cards introduced (`docs/bug.kanban.md` does not exist yet). One new card added to `docs/backlog.kanban.md` (HTTP Range / resume) — already committed before plan approval per the same workflow that committed the audio fixtures in Plan 2's pre-approval gate.

No BREAKs. Linux deferred per concept override.

## Steps

Single phase, macOS-only execution per the concept's Linux-deferral override.

1. **Add `ureq` dependency.** Edit `Cargo.toml`. Append to the existing `[dependencies]` block, verbatim:
   ```toml
   # HTTP client for HuggingFace model downloads (PLAN_model_storage).
   # Lookup date 2026-05-08. Pure-Rust TLS via rustls; no system OpenSSL.
   ureq = { version = "=3.3.0", default-features = false, features = ["rustls", "platform-verifier"] }
   ```
   Run `cargo build --release` once to confirm compilation. `ureq 3.3.0` is already in `Cargo.lock` as a transitive dependency of `intel-onemkl-prebuild` (Linux-only `ct2rs` BLAS path), so promoting it to a direct dependency at the same pinned version produces no resolver delta. ureq + rustls + platform-verifier are pure-Rust; on macOS this is the first time they actually compile, but `cargo` will not need to refetch the source. CTranslate2, symphonia, rubato are already cached from Plans 1–2 and do not rebuild. No napi-feature build needed yet — Step 7 covers that after the new module exists.
   If `ureq 3.3.0` is unresolvable (yanked, missing from registry), stop and report (Rule 11).

2. **Update `.gitignore`.** Append a single line to the existing `.gitignore`:
   ```
   /target/cadmus-test-cache/
   ```
   The `target/` directory is already ignored as a whole, but the explicit anchor documents the test-cache location for readers and survives any future change to the toplevel `target/` rule. Verify with `git status` that `target/cadmus-test-cache/` (once tests run in Step 8) does not appear as untracked.

3. **Create `src/storage.rs`.** New file, crate-private module. Top of file:
   ```rust
   #![allow(dead_code)] // Removed in Plan 4 when the inference test consumes this.

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
       Http(u16, String),                  // status, body excerpt or empty
       Network(String),                    // ureq transport, redirect, TLS, …
       Io(String),                         // local filesystem
   }

   impl std::fmt::Display for DownloadError {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           match self {
               DownloadError::Cancelled       => write!(f, "download cancelled"),
               DownloadError::Http(s, m)      => write!(f, "http {s}: {m}"),
               DownloadError::Network(m)      => write!(f, "network: {m}"),
               DownloadError::Io(m)           => write!(f, "io: {m}"),
           }
       }
   }
   impl std::error::Error for DownloadError {}
   ```

4. **Define the `TINY` entry.** Append to `src/storage.rs`:
   ```rust
   pub(crate) const TINY: ModelEntry = ModelEntry {
       files: &[
           FileSpec { repo: "Systran/faster-whisper-tiny", file: "model.bin" },
           FileSpec { repo: "Systran/faster-whisper-tiny", file: "config.json" },
           FileSpec { repo: "Systran/faster-whisper-tiny", file: "tokenizer.json" },
           FileSpec { repo: "Systran/faster-whisper-tiny", file: "vocabulary.txt" },
           FileSpec { repo: "openai/whisper-tiny",         file: "preprocessor_config.json" },
       ],
   };
   ```
   Four of the five files come from the CTranslate2-converted `Systran/faster-whisper-tiny`. `preprocessor_config.json` is sourced from `openai/whisper-tiny` because the Faster-Whisper conversion pipeline does not include it in the converted output — see ct2rs's whisper example (`ct2-transformers-converter --copy_files preprocessor_config.json tokenizer.json`). All five files end up flat in the same destination directory, which is what `ct2rs::Whisper::new` reads (`PREPROCESSOR_CONFIG_FILE` constant in `ct2rs::whisper`). If a future HF restructure removes any of these, the smoke test in Step 8 fails fast at the per-file 404 and surfaces R1.

   Both repos are MIT-licensed; no `LICENSE-THIRD-PARTY` change required.

5. **Implement `download`, `ensure_present`, and `test_cache_dir`.** Append to `src/storage.rs`:
   ```rust
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
       let mut response = ureq::get(url)
           .call()
           .map_err(|e| match &e {
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
       let cleanup = || { let _ = fs::remove_file(&tmp); };

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
   ```

   Implementation notes:
   - The `cancelled` closure is captured by reference and re-evaluated at each top-of-loop iteration; this matches the cooperative semantics in architecture.md §4.
   - The `.part` rename pattern protects against SIGKILL mid-write — a half-written `model.bin.part` will not be picked up as a complete `model.bin` by `ensure_present` on the next run.
   - `ureq::Error::StatusCode(u16)` exists in ureq 3.x as the variant for non-2xx HTTP responses. The match arm is exhaustive enough for our purposes; any other variant collapses to `Network(...)` which carries the formatted message — we accept slightly less granular error classification here, since the public surface in Plan 5 will map all of these into `CadmusError::Load` anyway.
   - No retry. A network blip on the first run produces a `Network` error; the caller reruns `cargo test`. Adding retry would belong in the same backlog item as resume.

6. **Wire the module.** In `src/lib.rs`, add `mod storage;` near the existing `mod decode;` line (no `pub`). Verify the surface unchanged — `cadmus::version()` is still the only public symbol. The `#![allow(dead_code)]` at the top of `src/storage.rs` suppresses unused-function warnings until Plan 4.

7. **Build verification.** Run, in order:
   - `cargo build --release` → green; ureq + rustls compile once.
   - `cargo build --release --features napi` → green. This compiles the napi-feature-gated cdylib in `target/release/`; it does **not** update the repo-root `cadmus.darwin-arm64.node` (that artifact is rebuilt only by `napi build` / `npm run build:napi`, which is a release-time step per the Concept's Release Runbook). Verifies that the new `mod storage;` and the `ureq` dep do not regress the napi feature compile.

8. **Write tests.** Append `#[cfg(test)] mod tests { … }` to `src/storage.rs`. Five tests in two groups: live HF coverage (one) and stdlib-`TcpListener` mock coverage (two). Plus the no-network `cancel_before_call_returns_cancelled` and `ensure_present_distinguishes_states`.

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;
       use std::net::TcpListener;
       use std::sync::Mutex;
       use std::sync::atomic::AtomicBool;
       use std::thread;
       use std::time::Duration;

       // -----------------------------------------------------------------
       // Live HuggingFace smoke (Concept Plan 3 "Done when": real fetch
       // path exercised end-to-end). Pulls ~75 MB on first run; subsequent
       // runs on the same target/ are instant because every file is
       // present with size > 0.
       // -----------------------------------------------------------------
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

       // -----------------------------------------------------------------
       // No-network: cancel-before-call short-circuits before any HTTP.
       // -----------------------------------------------------------------
       #[test]
       fn cancel_before_call_returns_cancelled() {
           let dir = test_cache_dir().join("never-touched-cancelled-target");
           let cancel = AtomicBool::new(true);
           let result = download(&TINY, &dir, None, Some(&cancel));
           assert!(matches!(result, Err(DownloadError::Cancelled)));
           // No HTTP traffic, no files created beyond create_dir_all may have
           // run for `dir` itself — but no descendants.
           let empty_or_absent = !dir.exists()
               || dir.read_dir().map(|mut r| r.next().is_none()).unwrap_or(true);
           assert!(empty_or_absent);
       }

       // -----------------------------------------------------------------
       // No-network: ensure_present's three-state contract.
       // -----------------------------------------------------------------
       #[test]
       fn ensure_present_distinguishes_states() {
           let temp = test_cache_dir().join("ensure-present-fixture");
           let _ = fs::remove_dir_all(&temp);
           fs::create_dir_all(&temp).unwrap();

           assert!(!ensure_present(&TINY, &temp));                 // missing → false

           for spec in TINY.files {                                // zero-byte → false
               File::create(temp.join(spec.file)).unwrap();
           }
           assert!(!ensure_present(&TINY, &temp));

           for spec in TINY.files {                                // size > 0 → true
               fs::write(temp.join(spec.file), b"x").unwrap();
           }
           assert!(ensure_present(&TINY, &temp));
       }

       // -----------------------------------------------------------------
       // Local mock server: deterministic progress callback contract.
       // Bound on 127.0.0.1:0; serve a known-size payload in chunks; assert
       // received is monotonic, received <= total when total > 0, and
       // final received == total.
       // -----------------------------------------------------------------
       #[test]
       fn progress_callback_against_local_server() {
           const BODY_LEN: usize = 200_000; // enough for ~3 progress chunks
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
           let cb = |r: u64, t: u64| { calls.lock().unwrap().push((r, t)); };
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

       // -----------------------------------------------------------------
       // Local mock server: mid-stream cancel deletes the .part file and
       // returns Cancelled. Body is sent slowly enough that the cancel flag
       // is observed before the loop reads EOF.
       // -----------------------------------------------------------------
       #[test]
       fn cancel_mid_stream_against_local_server() {
           const BODY_LEN: usize = 4 * 1024 * 1024; // 4 MiB → many chunks
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
               // Errors are tolerated: the test cancels mid-stream, which
               // may close the socket before the server finishes writing.
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
           let cb = |_r: u64, _t: u64| { cancel.store(true, Ordering::SeqCst); };
           let cancelled_fn = || cancel.load(Ordering::SeqCst);

           let result = fetch_one(&url, &target, Some(&cb), &cancelled_fn);
           assert!(matches!(result, Err(DownloadError::Cancelled)));

           // .part path computed the same way fetch_one does.
           let part = target.with_extension(
               target
                   .extension()
                   .map(|e| format!("{}.part", e.to_string_lossy()))
                   .unwrap_or_else(|| "part".to_string()),
           );
           assert!(!part.exists(), ".part file should be deleted on cancel");
           assert!(!target.exists(), "final file should not exist on cancel");

           // Server thread may still be writing; let it observe socket close.
           let _ = server.join();
       }
   }
   ```

   Coverage map: `download_tiny_smoke` exercises the live HF fetch path (Concept Plan 3 "Done when"); `cancel_before_call_returns_cancelled` proves the cooperative-cancel pre-check (architecture.md §4); `ensure_present_distinguishes_states` proves the three-state contract (D19 preview); `progress_callback_against_local_server` proves the §9.1 progress contract deterministically without network traffic (architecture.md §8.2); `cancel_mid_stream_against_local_server` proves the in-loop cancel + `.part` cleanup, the variant the live test cannot exercise without flake (architecture.md §8.2).

   Why a hand-rolled `TcpListener` rather than a mocking crate: stdlib only, no new dependency (Hard Rule 11), and the request shape we need is one GET, one fixed response — a 30-line ad-hoc server is simpler than wiring `mockito`/`wiremock`. ureq will speak HTTP/1.1 plain (`http://127.0.0.1:…`) without engaging the rustls TLS path; `platform-verifier` is dormant.

9. **Run the tests.**
   - `cargo test --release storage::tests` → on first run, downloads ~75 MB into `target/cadmus-test-cache/tiny/` (~30 s on a typical home connection; four files from `Systran/faster-whisper-tiny`, one from `openai/whisper-tiny`, all flat in the same directory). The two mock-server tests serve their bodies from local memory and finish in well under a second each. Subsequent runs are fast because the smoke test reuses its cache.
   - `cargo test` (no flags, no `--release`) → same tests, slower compile, same download cost.
   - `cargo test --features napi` → all tests pass; `14 passed` total (1 version + 8 audio + 5 storage).
   - `cargo build --release` and `cargo build --release --features napi` → green (the napi build verifies feature-gated compile only; the repo-root `.node` file is not regenerated — see Step 7).

   Per Hard Rule 11 the Coder does not invoke `cargo update` or rewrite `Cargo.lock` beyond what `cargo build` does naturally.

   `npm test` is intentionally **not** part of this plan's verification matrix. Plan 3 changes nothing on the napi/Node surface, and the existing `tests/version.test.mjs` passes today only because the committed `cadmus.darwin-arm64.node` was not rebuilt at the 0.3.0 release (the binary still reports `0.2.0` against a `^0\.2\.0` regex). That drift is a separate release-discipline finding the Coder must flag to the Human (Hard Rule 4); resolving it is not in scope for `PLAN_model_storage`.

10. **Verify packaging boundaries (D27).**
    - `cargo package --list --allow-dirty` — additionally lists `src/storage.rs`. Still no `package.json`, no `index.ts`, no `node_modules/`, no `cadmus.*.node`, no `target/cadmus-test-cache/...`, no `docs/`. The cache directory must not appear because `target/` is excluded by absence-from-include-list (D27).
    - `npm pack --dry-run` — unchanged from Plan 2's actual output: lists `index.js`, `index.d.ts`, `cadmus.darwin-arm64.node`, `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md`, plus `package.json` (npm always includes the manifest regardless of `package.json#files`), for **seven entries** on this macOS-only host. `cadmus.linux-x64-gnu.node` is in `package.json.files` but absent on this branch — npm emits a warning, which is acceptable per the Concept's Linux-deferral override and matches the Plan 2 baseline. Plan 3 does not require the Linux binary to appear in the pack output. (Plan 2's prose listed six entries by overlooking the always-included `package.json`; this plan-amendment corrects the count.)

Implementation is done at this point. Per AGENTS.md §5 the Coder stops here and waits — Validation, Doc Update, and Archive happen in subsequent phases driven by the next-step prompt.

## Verification

After Step 10, the working tree on macOS satisfies:

- `cargo test --release` → all tests pass; cargo-side count grew from 9 to 14 (1 version + 8 audio + 5 storage).
- `cargo test --release --features napi` → same count, all green.
- `cargo build --release` → green.
- `cargo build --release --features napi` → green (feature-gated compile only; repo-root `.node` file is not regenerated by this command).
- `cargo package --list --allow-dirty` → contains `src/storage.rs`; no leakage of npm-side files; no `target/cadmus-test-cache/...` entry.
- `npm pack --dry-run` → seven entries on this macOS-only host (`index.js`, `index.d.ts`, `cadmus.darwin-arm64.node`, `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md`, `package.json`); the `cadmus.linux-x64-gnu.node` warning carries over from Plan 2 per the Linux-deferral override.
- `target/cadmus-test-cache/tiny/` populated with five files of non-zero size (four from `Systran/faster-whisper-tiny`, one from `openai/whisper-tiny`).
- `target/cadmus-test-cache/` is gitignored (`git status` shows nothing under that path).
- No public Rust API change: `cadmus::version()` still the only public function.
- No public napi API change. The repo-root `cadmus.darwin-arm64.node` is unchanged (not rebuilt by this plan).
- No new TODOs in code without a card in `docs/backlog.kanban.md`.
- Linux verification deferred per concept override.

Out of scope for this plan's verification: `npm test`. See Step 9 — the existing pass relies on a committed `.node` binary still reporting `0.2.0`, an unrelated release-discipline drift that the Coder flags to the Human (Hard Rule 4) but does not fix here.

### Reviewer focus points

- **Crate-internal API only**: `download`, `ensure_present`, `test_cache_dir`, `ModelEntry`, `TINY`, `DownloadError` are all `pub(crate)`. `lib.rs` re-exports nothing new. Public surface is still exactly `version()` + `Version`.
- **No async runtime**: `ureq` is blocking; no tokio, smol, or executor dependency. D2 ("core API is synchronous") is preserved.
- **Cooperative cancel correctness**: cancel flag set before invocation produces `Cancelled` immediately, no HTTP, no filesystem touch beyond `create_dir_all` which is benign. Cancel mid-download (not directly tested in this plan — would require a long-running mock or a second thread; covered indirectly by the in-loop cancel check) deletes the `.part` file and returns `Cancelled`.
- **Idempotency**: per-file early-skip in `download()` plus the smoke test's two-call structure prove a second invocation does no work. No checksum is implied (D19 / definition.md §5).
- **`.part` rename pattern**: protects `ensure_present` against SIGKILL mid-write — a stale `.part` is invisible to the `entry.files`-keyed presence check. A truncated complete-named file (rename succeeded, but bytes were short) is still possible if the network closed cleanly mid-stream without an error — this is the same exposure as definition.md §5 acknowledges and surfaces as a `Load` error in Plan 4.
- **Progress callback contract**: fires after each chunk write, never after a write failure, never after the function has returned. `received <= total` whenever `total > 0`.
- **License hygiene**: `ureq`'s MIT-or-Apache-2.0 is covered by the project's MIT licence; no `LICENSE-THIRD-PARTY` change needed (only file-scoped copyleft licences require that).
- **Dependency scope**: `ureq` enabled with `default-features = false, features = ["rustls", "platform-verifier"]`. No accidental enabling of `cookies` / `json` / `gzip` that would inflate the binary or pull in additional dependencies we don't use.
- **`#![allow(dead_code)]` opt-out**: justified for Plan 3 (the function's only caller arrives in Plan 4); flagged in R4. Plan 4 must remove it.
- **No BREAKs, no Linux work**: Linux remains deferred per concept override; this plan touches macOS only.
- **Network cost**: ~75 MB on the first run (the live `download_tiny_smoke`); zero on subsequent runs. The progress and mid-stream-cancel tests use a stdlib `TcpListener` and never touch the network. Plan 4's inference test reuses the smoke cache for free.
- **Mock-server tests follow architecture.md §8.2** — the document explicitly calls for "mock HTTP, verify directory structure written, progress callback invoked, cancellation flag respected"; the new tests cover the latter two deterministically. Directory-structure-from-mock is not duplicated because the live smoke test already proves it end-to-end against the real fetch path.
- **`fetch_one` visibility for tests**: the function is module-private; `#[cfg(test)] mod tests` lives in the same file and can call it directly, so no API change leaks to other modules.
