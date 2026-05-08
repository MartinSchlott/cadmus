# PLAN_audio_pipeline — Audio decoding and resampling pipeline

Plan #2 of CONCEPT_v1_buildout. Adds the crate-internal audio pipeline: bytes → symphonia decode → channel downmix → rubato resample → `Vec<f32>` at 16 kHz mono in `[-1, 1]`. Crate-internal API only — no public surface change. Plan 4 wires this into the public `transcribe()` path.

Reference: [CONCEPT_v1_buildout.md](CONCEPT_v1_buildout.md), in particular **D5** (audio pipeline composition), **D20** (license discipline), **D27** (packaging allowlists), and the Plan 2 row of the Plan Breakdown.

---

## Context & Goal

Cadmus's promise (definition.md §3) — "no FFmpeg or system audio libs, format detected from bytes" — depends on a self-contained pure-Rust pipeline. This plan implements it.

After this plan:

- A crate-internal function `decode_to_pcm16k(bytes: &[u8]) -> Result<Vec<f32>, AudioError>` accepts raw audio bytes (MP3, WAV, FLAC), produces 16 kHz mono `Vec<f32>` in `[-1, 1]`.
- Pipeline is symphonia (decode) → in-house downmix → rubato (resample). symphonia detects format from the byte stream's magic bytes — caller never declares format.
- Audio that already arrives as 16 kHz mono is passed through without resampling and without channel manipulation (no-op fast paths, matching architecture.md §3 "If the source is already mono at 16 kHz, the downmix and resample stages are no-ops").
- Crate-internal `pub(crate) enum AudioError { Decode(String), Resample(String) }`. Plan 4 promotes this into `CadmusError` variants; until then it lives next to the function.
- Module: `src/decode.rs` (matching the canonical name in CONCEPT_v1_buildout § Repository layout). Wired into `src/lib.rs` as `mod decode;` (private).
- `LICENSE-THIRD-PARTY` created at repo root (D27 already lists it in `Cargo.toml [package].include`; symphonia is MPL-2.0 per D20). `package.json`'s `files` array is extended to include it (D27 npm-side allowlist).
- Fixture-based tests: existing `fixtures/eins-zwei-drei.mp3` plus two additional checked-in fixtures `fixtures/eins-zwei-drei.wav` and `fixtures/eins-zwei-drei.flac` (same recording, different containers/codecs, sample-rate ≠ 16 kHz so the resampler is exercised). **Both files are committed by the Human as part of the plan-approval gate (see Pre-approval gate below); they are present in `fixtures/` before the Coder begins Step 1.** Synthetic stereo and resample tests validate the in-house downmix and the rubato pass independently of any codec quirk.

### Pre-approval gate

Before the Human approves this plan, the Human commits `fixtures/eins-zwei-drei.wav` and `fixtures/eins-zwei-drei.flac` to the repository. Both files contain the same spoken phrase as the existing MP3 and use a sample rate ≠ 16 kHz (44.1 kHz or 48 kHz; WAV PCM 16-bit; FLAC any standard configuration). Once the fixtures are on disk, the plan is fully self-contained — a fresh AI instance with checkout access can execute every step. If the Human chooses not to provide them, the plan must be revised before approval (e.g. by relaxing format coverage to MP3 + synthetic-WAV).

No public API changes; `version()` remains the only public surface. Plan 4 will surface `transcribe()` to callers and route it through `decode_to_pcm16k`.

## Breaking Changes

**None.** Additive only — new module, two new dependencies, two new fixtures, new file `LICENSE-THIRD-PARTY`. No existing source modified beyond a one-line `mod decode;` addition in `src/lib.rs` and the `[dependencies]` block in `Cargo.toml`.

## Reference Patterns

- **symphonia** decode skeleton: `examples/basic-interleaved.rs` in the symphonia repository (https://github.com/pdeljanov/Symphonia). The pattern is `MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default())` → `default::get_probe().format(...)` with empty hints (magic-byte detection, no filename) → `default::get_codecs().make(track.codec_params, ...)` → packet decode loop. We collapse the loop's output into a single interleaved `Vec<f32>` before downmix/resample.
- **rubato** SincFixedIn: `examples/process_f64.rs` in the rubato repository (https://github.com/HEnquist/rubato), adapted to `f32` (Whisper's input type). Construct once with `(target_rate as f64) / (src_rate as f64)` ratio; call `process` over fixed-size input chunks and `process_partial` for the trailing partial chunk; concatenate outputs.

If the upstream repos are unreachable at implementation time, the patterns spelled out in Steps 4 are the binding spec — the Coder implements them directly.

## Dependencies

Approved by Human. **No additions during implementation without escalation (Hard Rule 11).** Concrete versions, looked up against crates.io on **2026-05-08** (plan-write date) and pinned exactly. The Coder uses these literally; if any has been yanked or is otherwise unobtainable, the Coder stops and reports rather than silently picking a different release (Rule 11). This matches PLAN_skeleton's pinning discipline for ct2rs (`=0.9.18`).

**Rust (`Cargo.toml [dependencies]`):**

| Crate | Version | Role / features |
|---|---|---|
| `symphonia` | `=0.5.4` | Audio decode + magic-byte format detection. `default-features = false`, `features = ["mp3", "wav", "flac"]`. Other codecs (AAC, Vorbis, Opus, …) are explicitly **not** enabled — v1 fixture coverage is exactly these three formats; future formats are a separate plan. |
| `rubato` | `=0.16.2` | Sample-rate conversion via `SincFixedIn<f32>`. No feature flags needed. |

If either crate's pinned version is unavailable on crates.io at implementation time, or its API differs from the calls used in Steps 4 (e.g. `SincFixedIn` renamed, `Probe::format` signature changed, sample-conversion path moved), the Coder stops and reports — replacing or upgrading a pinned dependency is a plan-level decision, not an implementation one (Rule 11 / Rule 7).

No npm dependencies change. No transitive runtime cost on the npm side; both crates are pure Rust and compile into the existing `.node`/rlib without any additional system requirement.

### License impact (D20)

symphonia is **MPL-2.0** — file-scoped copyleft. Cadmus's own code stays MIT. The MPL obligation is per-file attribution at distribution; this plan creates `LICENSE-THIRD-PARTY` with one entry (symphonia name + version + license + upstream URL). Rubato is MIT — its attribution is covered by the project's existing MIT `LICENSE` covering the binary as a whole; no separate entry needed.

`Cargo.toml`'s `[package].include` already lists `/LICENSE-THIRD-PARTY` (PLAN_skeleton committed it preemptively), so no allowlist change is needed — only the file itself.

## Assumptions & Risks

- **A1.** The two additional fixtures (`eins-zwei-drei.wav`, `eins-zwei-drei.flac`) are committed to `fixtures/` before plan approval (see Pre-approval gate in Context & Goal). The Coder verifies their presence as Step 1 and stops if absent — but under normal flow this is a no-op check, since the gate prevents approval without the files. WAV: PCM 16-bit, sample rate ≠ 16 kHz. FLAC: any standard configuration, sample rate ≠ 16 kHz.
- **A2.** symphonia returns decoded samples in one of its supported formats (`f32`, `f64`, `i16`, `i32`, `u8`, `u16`, `u24`, `u32`, …) depending on codec. The Coder normalises whatever symphonia produces into a single interleaved `Vec<f32>` in `[-1, 1]` using the codec-agnostic conversion path symphonia documents (typically via `AudioBufferRef`'s sample conversion or per-format scaling). If a fixture's source codec returns a sample format not directly normalisable by the obvious arithmetic, that is a plan-level surprise — Coder stops and reports (Rule 7). For MP3/WAV/FLAC at typical bit depths this should not trigger.
- **R1.** symphonia's MPL-2.0 obligation is satisfied by `LICENSE-THIRD-PARTY` attribution; this is the same pattern used by other Rust audio projects shipping symphonia as a dependency. No further legal action needed if the file exists.
- **R2.** rubato's `SincFixedIn` requires fixed input chunk size. For a single-shot full-file resample, the Coder feeds full chunks via `process` and the trailing partial chunk via `process_partial`. Output sample count differs from the ideal `len * target / src` by at most a few samples at the tail; the tests use approximate (`±64 samples`) length checks and never strict equality.
- **R3.** Memory: a 1-minute 48 kHz stereo WAV decoded into `Vec<f32>` is ~23 MB. Fixture is ~5 seconds — negligible. v1 explicitly accepts the in-memory-buffer cost; streaming/chunked decode is out of scope (definition.md §6).
- **R4.** `cargo` may emit `unused` warnings on the new module since no public path consumes it before Plan 4. This is expected. The module is opted out of dead-code lint with `#![allow(dead_code)]` at the top of `src/decode.rs`. Plan 4 removes that line when `transcribe()` becomes the consumer.

No new `severity: accepted` bug cards introduced. No BREAKs (no host switch — Linux remains deferred per concept override).

## Steps

Single phase, macOS-only execution per the concept's Linux-deferral override.

1. **Verify fixtures.** `fixtures/eins-zwei-drei.mp3` exists (already committed). Check that `fixtures/eins-zwei-drei.wav` and `fixtures/eins-zwei-drei.flac` also exist on disk. If either is missing, stop and ask the Human to supply them (A1) before continuing — fixture creation is out of scope for this plan.

2. **Add dependencies.** Edit `Cargo.toml`:
   - Append to the existing `[dependencies]` block, verbatim:
     ```toml
     # Audio pipeline (PLAN_audio_pipeline). Lookup date 2026-05-08.
     symphonia = { version = "=0.5.4", default-features = false, features = ["mp3", "wav", "flac"] }
     rubato    = "=0.16.2"
     ```
   - Run `cargo build --release` once (no features) to confirm the new deps compile cleanly into the rlib path. Run `cargo build --release --features napi` to confirm the napi path still builds. CTranslate2 is already cached from Plan 1 — only symphonia and rubato compile this round (~30 s).
   - If either pin is not resolvable by cargo (yanked, missing from the registry), stop and report (Rule 11). Do not silently float to a neighbouring version.

3. **Update `package.json` — extend the `files` allowlist.** Add `"LICENSE-THIRD-PARTY"` to the `files` array so it ships in the npm tarball alongside `LICENSE` (D27 npm-side allowlist). The current array is `["index.js", "index.d.ts", "cadmus.darwin-arm64.node", "cadmus.linux-x64-gnu.node", "LICENSE", "README.md"]`; the new array is `["index.js", "index.d.ts", "cadmus.darwin-arm64.node", "cadmus.linux-x64-gnu.node", "LICENSE", "LICENSE-THIRD-PARTY", "README.md"]`. Insert the entry between `LICENSE` and `README.md` to match the literal D27 ordering. No other field in `package.json` changes. (PLAN_skeleton omitted this entry because the file did not exist yet; this plan introduces both the file and the allowlist entry together.)

4. **Create `LICENSE-THIRD-PARTY`** at the repository root. Plain text, content:
   ```
   This software incorporates third-party code under the following licenses.

   ─────────────────────────────────────────────
   symphonia  —  https://github.com/pdeljanov/Symphonia
   License:   Mozilla Public License Version 2.0
   Version:   <pinned version>

   See https://www.mozilla.org/en-US/MPL/2.0/ for the full license text.
   The MPL-2.0 applies to symphonia source files only; modifications to those
   files (none in this project) would be subject to MPL terms.
   ─────────────────────────────────────────────
   ```
   Substitute the actual pinned `symphonia` version from Step 2 into the `Version:` line.

5. **Implement `src/decode.rs`.** New file, crate-private module. Skeleton:

   ```rust
   #![allow(dead_code)]  // Removed in Plan 4 when transcribe() consumes this.

   use std::io::Cursor;

   pub(crate) const TARGET_SAMPLE_RATE: u32 = 16_000;

   #[derive(Debug)]
   pub(crate) enum AudioError {
       Decode(String),
       Resample(String),
   }

   impl std::fmt::Display for AudioError {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           match self {
               AudioError::Decode(m)   => write!(f, "decode failed: {m}"),
               AudioError::Resample(m) => write!(f, "resample failed: {m}"),
           }
       }
   }
   impl std::error::Error for AudioError {}

   /// Decode arbitrary audio bytes to 16 kHz mono `f32` in `[-1, 1]`.
   /// Format is detected from the byte stream — caller does not declare it.
   pub(crate) fn decode_to_pcm16k(bytes: &[u8]) -> Result<Vec<f32>, AudioError> {
       let (interleaved, sample_rate, channels) = decode_interleaved(bytes)?;
       let mono = downmix_to_mono(&interleaved, channels);
       if sample_rate == TARGET_SAMPLE_RATE {
           return Ok(mono);
       }
       resample_to_target(&mono, sample_rate)
   }

   fn decode_interleaved(bytes: &[u8]) -> Result<(Vec<f32>, u32, u16), AudioError> { /* … */ }
   fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> { /* … */ }
   fn resample_to_target(mono: &[f32], src_rate: u32) -> Result<Vec<f32>, AudioError> { /* … */ }
   ```

   Implementation notes per helper:

   - **`decode_interleaved`**:
     - `let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());`
     - Use `symphonia::default::get_probe().format(&Hint::new(), mss, &Default::default(), &Default::default())` (no filename hint — magic-byte detection only). On error: `AudioError::Decode(format!("probe: {e}"))`.
     - Take the first track in `format.default_track()` (or first track in `tracks()` if `default_track()` is `None`).
     - Build a decoder: `symphonia::default::get_codecs().make(&track.codec_params, &Default::default())`. Capture `sample_rate` and `channels.count() as u16` from `codec_params`.
     - Loop: `format.next_packet()`. Decode each packet. For each `AudioBufferRef`, copy samples into a growing `Vec<f32>` interleaved, converting non-`f32` formats with the obvious scaling (`i16` → `f / 32768.0`, `i32` → `f / 2_147_483_648.0`, etc.). symphonia's per-`AudioBufferRef` variants expose `chan(i)` (planar accessor) — interleave by iterating frames and channels. Stop on `Err(SymphoniaError::IoError)` with `ErrorKind::UnexpectedEof` (clean EOF) or on `ResetRequired`/`DecodeError` (treat as decode failure).
     - Wrap any unexpected error as `AudioError::Decode(format!("{e}"))`.

   - **`downmix_to_mono`**:
     - If `channels == 1`: return `interleaved.to_vec()`.
     - Otherwise: `interleaved.chunks_exact(channels as usize).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect()`.

   - **`resample_to_target`**:
     - Construct rubato's `SincFixedIn::<f32>::new(...)` with:
       - `resample_ratio: target_rate as f64 / src_rate as f64`
       - `max_resample_ratio_relative: 1.0` (no runtime ratio change)
       - `parameters: SincInterpolationParameters { sinc_len: 256, f_cutoff: 0.95, interpolation: SincInterpolationType::Linear, oversampling_factor: 256, window: WindowFunction::BlackmanHarris2 }` — rubato's documented "good quality default" set; if rubato's struct names have moved, the Coder uses the equivalent in the pinned version.
       - `chunk_size: 1024`
       - `nbr_channels: 1`
     - Process the input: feed `1024`-frame chunks via `process(&[chunk], None)`; feed the trailing `< 1024` chunk via `process_partial(Some(&[tail]), None)`. Each call returns one `Vec<Vec<f32>>` — take channel 0 and extend the output buffer.
     - Wrap rubato errors as `AudioError::Resample(format!("{e}"))`.
     - Return the output `Vec<f32>` at 16 kHz.

   No `pub use` in `lib.rs`. The function is callable only from within the crate.

6. **Wire the module.** In `src/lib.rs`, add `mod decode;` near the top (no `pub`). Verify `cargo build` and `cargo build --features napi` are green. The `#![allow(dead_code)]` at the top of `decode.rs` suppresses unused-function warnings until Plan 4.

7. **Write fixture-based decode tests.** Append to `src/decode.rs`:

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       const MP3:  &[u8] = include_bytes!("../fixtures/eins-zwei-drei.mp3");
       const WAV:  &[u8] = include_bytes!("../fixtures/eins-zwei-drei.wav");
       const FLAC: &[u8] = include_bytes!("../fixtures/eins-zwei-drei.flac");

       fn assert_valid_pcm16k(samples: &[f32]) {
           assert!(!samples.is_empty(), "decoded buffer is empty");
           assert!(samples.len() >= 16_000 * 2,
               "≥ 2 seconds expected, got {} samples", samples.len());
           assert!(samples.len() <= 16_000 * 30,
               "≤ 30 seconds expected, got {} samples", samples.len());
           assert!(samples.iter().all(|s| (-1.0..=1.0).contains(s)),
               "sample outside [-1, 1]");
       }

       #[test] fn decode_mp3_to_pcm16k()  { assert_valid_pcm16k(&decode_to_pcm16k(MP3 ).unwrap()); }
       #[test] fn decode_wav_to_pcm16k()  { assert_valid_pcm16k(&decode_to_pcm16k(WAV ).unwrap()); }
       #[test] fn decode_flac_to_pcm16k() { assert_valid_pcm16k(&decode_to_pcm16k(FLAC).unwrap()); }

       // The three fixtures hold the same recording. Decoded lengths must agree
       // within a small margin (codec framing + resampler-tail rounding).
       #[test]
       fn fixtures_have_consistent_length() {
           let m = decode_to_pcm16k(MP3 ).unwrap().len();
           let w = decode_to_pcm16k(WAV ).unwrap().len();
           let f = decode_to_pcm16k(FLAC).unwrap().len();
           let (lo, hi) = ([m, w, f].iter().min().unwrap().clone(),
                           [m, w, f].iter().max().unwrap().clone());
           assert!(hi - lo < 2048, "fixtures diverge by {} samples (mp3={m} wav={w} flac={f})", hi - lo);
       }
   }
   ```

8. **Write synthetic tests for downmix and resample.** Append to the same `tests` module:

   - **Stereo downmix test.** Build a synthetic interleaved `Vec<f32>` of 1000 stereo frames with `L = +0.5`, `R = -0.5` (so `frame = [0.5, -0.5]` repeated). Call `downmix_to_mono(&buf, 2)`. Assert output length is 1000 and every sample is within `1e-6` of `0.0`.

   - **Resample-rate test.** Build a 1 kHz sine at 48 kHz mono for 1.0 second (48 000 samples, `s[i] = (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin()`). Call `resample_to_target(&sine, 48_000)`. Assert output length is in `15_900..=16_100`. Assert peak amplitude (max of `|s|`) stays in `[0.85, 1.0]` — sinc filtering attenuates negligibly at 1 kHz when Nyquist is 8 kHz, so the tone must survive nearly intact.

   - **Mono-16k passthrough test.** Generate 16 000 deterministic `f32` samples in `[-0.5, 0.5]` (e.g. `s[i] = ((i as f32 * 0.0001).sin()) * 0.5`). Construct a minimal valid WAV byte stream (16 kHz, 1 channel, 16-bit PCM) by hand:
     - Header: 44 bytes total, written via `Vec<u8>` extension calls (`extend_from_slice`). Format:
       - `b"RIFF"` (4) + `(36 + data_len) as u32` LE (4) + `b"WAVE"` (4)
       - `b"fmt "` (4) + `16u32` LE (4) + `1u16` LE (PCM, 2) + `1u16` LE (channels, 2) + `16_000u32` LE (sample rate, 4) + `32_000u32` LE (byte rate = sr·channels·bps/8, 4) + `2u16` LE (block align = channels·bps/8, 2) + `16u16` LE (bits per sample, 2)
       - `b"data"` (4) + `data_len as u32` LE (4) where `data_len = 16_000 * 2`
     - Data: for each `f32` sample `s`, encode as `let q: i16 = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16; bytes.extend_from_slice(&q.to_le_bytes());`. The clamp guards against `s = +1.0` exactly (would overflow `i16::MAX as f32` rounding) and against any future input outside `[-1, 1]`.
     - Total ~30 lines of byte-pushing; no extra dependency.
     - Call `decode_to_pcm16k(&wav_bytes)`. Assert output length is exactly 16 000 (resampler bypassed because rate already matches). Sample-by-sample comparison against the original `f32` input within tolerance `2.0 / 32768.0` (one i16 quantisation step round-trip).

   These three synthetic tests pin the in-house downmix and the resample path independently of any codec quirk, so a future symphonia bug cannot mask a regression in our own glue.

9. **Run tests on macOS.**
   - `cargo test` (no features) → `8 passed; 0 failed` (1 existing version test + 3 fixture decode tests + 1 cross-fixture length test + 3 synthetic tests).
   - `cargo test --features napi` → same count, all pass.
   - `cargo build --release` → green.
   - `cargo build --release --features napi` → green.
   - `npm test` → unchanged green (existing `version.test.mjs` passes; no new Node test in this plan).

10. **Verify packaging boundaries (D27).**
    - `cargo package --list --allow-dirty` — additionally lists `src/decode.rs`, `fixtures/eins-zwei-drei.wav`, `fixtures/eins-zwei-drei.flac`, `LICENSE-THIRD-PARTY`. Still no `package.json`, no `index.ts`, no `index.js`/`index.d.ts`, no `node_modules/`, no `cadmus.*.node`, no `docs/`.
    - `npm pack --dry-run` — now lists **seven** entries: `index.js`, `index.d.ts`, `cadmus.darwin-arm64.node`, `cadmus.linux-x64-gnu.node` (still absent on this host; npm warning OK), `LICENSE`, `LICENSE-THIRD-PARTY` (new in this plan), `README.md`. The added entry is the only delta vs. Plan 1's npm pack output. Anything else outside D27's allowlist → fix and re-verify.

11. **Commit.** Single commit titled `feat(audio): symphonia + rubato pipeline (crate-internal)` (or similar wording — Coder's discretion). Stage files individually (no `git add .`):
    - `Cargo.toml`, `Cargo.lock`
    - `package.json`
    - `src/lib.rs`, `src/decode.rs`
    - `LICENSE-THIRD-PARTY`
    - `fixtures/eins-zwei-drei.wav`, `fixtures/eins-zwei-drei.flac`

## Verification

After Step 11, branch HEAD on macOS satisfies:

- `cargo test` (no features) → all tests pass; the cargo-side count grew from 1 to 8.
- `cargo test --features napi` → same count.
- `cargo build --release` → green.
- `cargo build --release --features napi` → green; existing `cadmus.darwin-arm64.node` still produced cleanly.
- `npm test` → green (unchanged Node-side surface).
- `cargo package --list --allow-dirty` → contains `src/decode.rs`, `fixtures/eins-zwei-drei.{mp3,wav,flac}`, `LICENSE-THIRD-PARTY`; no leakage of npm-side files.
- `npm pack --dry-run` → seven entries including the new `LICENSE-THIRD-PARTY`; one-entry delta vs. Plan 1.
- `LICENSE-THIRD-PARTY` exists at root with a symphonia entry (MPL-2.0) and the actual pinned version.
- `package.json`'s `files` array contains `LICENSE-THIRD-PARTY` between `LICENSE` and `README.md`.
- No public Rust API change: `cadmus::version()` still the only public function; `pub use` set in `lib.rs` unchanged.
- No public napi API change: the `#[napi] pub fn version` is untouched.
- No new TODOs in code without a card in `docs/backlog.kanban.md`.
- `docs/bug.kanban.md` does not exist in the current repo and this plan does not create it. If a future plan creates it, the policy "no new `severity: accepted` cards introduced silently" applies — but for Plan 2 this verification line is moot and the file's absence is the expected state.
- Linux verification deferred per concept override.

### Reviewer focus points

- **Crate-internal API only**: `decode_to_pcm16k` is `pub(crate)`, not `pub`. `lib.rs` re-exports nothing new. Public surface is still exactly `version()` + `Version`.
- **Magic-byte format detection**: three fixtures (MP3/WAV/FLAC) decode without manual format hints — symphonia's `Hint::new()` (empty) succeeds for each. The byte-stream contract from definition.md §3 holds.
- **Pipeline correctness**: synthetic stereo downmix yields ≈ 0; synthetic 1 kHz sine survives 48 kHz → 16 kHz resampling at near-full amplitude; mono-16k input passes through bit-accurate (within i16 quantisation). The three synthetic tests catch regressions in our own glue independent of symphonia/rubato changes.
- **License hygiene**: `LICENSE-THIRD-PARTY` exists, lists symphonia (MPL-2.0) with the exact pinned version. Rubato (MIT) is covered by the project's MIT `LICENSE`.
- **Range invariant**: every test confirms output samples stay in `[-1, 1]`. This is the contract Plan 3 (inference) consumes.
- **Symphonia feature scope**: `default-features = false` with exactly `mp3`, `wav`, `flac`. No accidental codec sprawl that would inflate the binary or pull a wider MPL surface than necessary.
- **`#![allow(dead_code)]` opt-out**: justified for Plan 2 (the function's only caller arrives in Plan 4); flagged in R4. Plan 4 must remove it.
- **No BREAKs, no Linux work**: Linux remains deferred per concept override; this plan touches macOS only.
