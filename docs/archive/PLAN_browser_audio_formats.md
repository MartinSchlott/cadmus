# PLAN — Browser audio formats (WebM/Opus + MP4/AAC-LC)

## Context & Goal

Cadmus today decodes MP3, WAV, FLAC, and raw PCM (`Cargo.toml:56`, `src/decode.rs`). Browser `MediaRecorder` produces neither — Chromium/Firefox emit WebM/Opus, Safari emits MP4/AAC. A server that wants to feed browser-recorded audio into Cadmus must currently transcode first, which defeats the "format detected from bytes" promise (`definition.md:122`).

This plan extends the decode pipeline to cover both browser-default formats:

- **MP4 + AAC-LC** — purely a symphonia feature-flag extension (`isomp4` + `aac`). Symphonia handles the entire path natively.
- **WebM + Opus** — symphonia's `mkv` demuxer handles the WebM container (WebM is a Matroska subset) and tags the track with `CODEC_TYPE_OPUS`. Symphonia's Opus *decoder* in 0.5.4 is a literal empty stub (`symphonia-codec-opus/src/lib.rs` is 1 byte), so we route Opus packets to a thin safe wrapper around `unsafe-libopus` (c2rust transpile of libopus 1.3.1, pure-Rust build, BSD-3-Clause).

Cadmus's public surface does not change. `transcribe(audio)` continues to detect format from bytes; callers see two more supported containers.

## Breaking Changes

**No.** Purely additive. Existing MP3/WAV/FLAC paths are untouched. No public API change, no behavioural change for any existing input.

## Reference Patterns

- `docs/archive/PLAN_audio_pipeline.md` — the original symphonia + rubato wiring. Same shape of work: add fixtures, add dependencies/features, extend `src/decode.rs`, update doc + `LICENSE-THIRD-PARTY`.
- `src/decode.rs:43-112` (`decode_interleaved`) — the current symphonia decode loop. The Opus branch slots in *before* the generic decoder call at line 78 ff.

## Dependencies

Already added to `Cargo.toml` during Discussion (per Human approval — Coder cannot install during Implementation):

- `unsafe-libopus = "=0.2.0"` — pure-Rust libopus, no C toolchain at build time. License: BSD-3-Clause.

Additionally, **symphonia feature flags** extend (no new direct dep — features pull in symphonia's existing sub-crates transitively):

```toml
symphonia = { version = "=0.5.4", default-features = false,
              features = ["mp3", "wav", "flac", "pcm",
                          "mkv", "isomp4", "aac"] }
```

Three feature flags added: `mkv`, `isomp4`, `aac`. No `ogg` — browsers do not emit raw Ogg/Opus from `MediaRecorder`; the WebM path covers their actual output. Adding `ogg` later is a one-line change if needed.

## Assumptions & Risks

- **A1.** The two browser-format fixtures already exist and are committed-ready: `fixtures/eins-zwei-drei.webm` (Opus, 48 kHz mono) and `fixtures/eins-zwei-drei.m4a` (AAC-LC, 44.1 kHz mono), both derived from `eins-zwei-drei.mp3` via ffmpeg during Discussion. The Coder confirms presence as Step 1.
- **A2.** Browser `MediaRecorder` produces **mono** when fed a mono `MediaStreamTrack`, and Cadmus already downmixes anything else. The Opus glue therefore supports 1- and 2-channel inputs; OpusHead channel-mapping family 0 only (the only mapping in MediaRecorder output and our fixtures). Multi-channel Ambisonics (mapping family ≥ 1) is out of scope.
- **A3.** AAC-LC profile only. HE-AAC / HE-AACv2 (SBR/PS) is not supported by symphonia 0.5.4 and is not produced by browser `MediaRecorder` for `audio/mp4`. If a caller supplies an HE-AAC file, decode fails with `Decode`; that is acceptable and documented behaviour.
- **R1.** **Opus pre-skip.** OpusHead carries a `pre_skip` field — the number of 48 kHz samples to discard from the front of decoded output (decoder priming). Failure to honour it produces ~3-7 ms of garbage at the start of every WebM/Opus transcript. Mitigation: parse `pre_skip` from OpusHead (`extra_data[10..12]` as little-endian u16) and drop that many samples from the head of the decoded buffer before passing to downmix/resample.
- **R2.** **Track selection on multi-track MP4.** `format.default_track()` in `decode_interleaved` may return a video track if a caller supplies a video container — symphonia's reader has no notion of "audio-first" and will hand back whatever the container marked as default, which for MP4 with video is commonly the video track. The existing fallback (`.find(|t| t.codec != CODEC_TYPE_NULL)`, `decode.rs:60-63`) does not discriminate audio vs. video either. Mitigation: filter **both** paths through an `is_audio(t)` predicate that requires non-null codec and `sample_rate.is_some()`. See Step 5 for the concrete code.
- **R3.** **OpusHead in Matroska CodecPrivate.** Symphonia's mkv demuxer surfaces CodecPrivate as `codec_params.extra_data` (`Box<[u8]>`). For Opus this is the OpusHead block (≥ 19 bytes, RFC 7845 §5.1). The implementation validates magic `"OpusHead"` at bytes 0..8, version byte 8 == 1, channel count at byte 9 ∈ {1, 2}, and **channel mapping family at byte 18 == 0** (single-stream mono/stereo). Anything else → `Decode("…")` with a specific message. Bytes 12..16 (input sample rate u32 LE) and 16..18 (output gain i16 LE) are parsed for completeness but not used — the decoder always runs at 48 kHz and we do not honour output gain (browser-recorded streams set it to 0 in practice).
- **R4.** **`unsafe-libopus` is unsafe Rust.** The crate is a `c2rust` transpile, so every internal call site is wrapped in `unsafe`. Our wrapper module isolates this in `src/opus.rs` and exposes a safe API to `decode.rs`. The wrapper is the only `unsafe` block this plan introduces.
- **R5.** **mkv feature pulls in additional symphonia code.** `cargo package --list` and `npm pack --dry-run` should both remain within their existing allowlists (`Cargo.toml [package].include`, `package.json files`) — neither is affected by adding transitive features. Verification re-checks both.

## Steps

Single phase, macOS-only execution per the project's Linux-deferral convention. Implementation ends after Step 8 — Doc Update and the single Archive commit happen in their own workflow phases after Validation (see *Doc Update* and *Archive* sections below).

1. **Verify fixtures.** `ls fixtures/eins-zwei-drei.{mp3,wav,flac,webm,m4a}` — all five files present. Quickly re-check codecs:
   ```
   ffprobe -hide_banner -v error -show_entries stream=codec_name,sample_rate,channels \
     -of default=noprint_wrappers=1 fixtures/eins-zwei-drei.webm
   # expect: codec_name=opus, sample_rate=48000, channels=1

   ffprobe -hide_banner -v error -show_entries stream=codec_name,sample_rate,channels \
     -of default=noprint_wrappers=1 fixtures/eins-zwei-drei.m4a
   # expect: codec_name=aac, sample_rate=44100, channels=1
   ```
   If either file is missing or wrong, stop and report.

2. **Extend symphonia feature set.** Edit `Cargo.toml:56`:
   ```toml
   symphonia = { version = "=0.5.4", default-features = false,
                 features = ["mp3", "wav", "flac", "pcm", "mkv", "isomp4", "aac"] }
   ```
   Update the preceding comment lookup date to today (2026-05-20). Leave the `unsafe-libopus` line (already added) and the comment block untouched.

3. **Create `src/opus.rs`.** New private module exposing a safe `OpusDecoder` wrapper around `unsafe-libopus`. Wire it into `src/lib.rs` as `mod opus;` next to `mod decode;`.

   Required surface (only what `decode.rs` needs — keep it minimal):
   ```rust
   pub(crate) struct OpusDecoder { /* opaque, holds *mut unsafe_libopus::OpusDecoder + channels */ }

   impl OpusDecoder {
       /// channels must be 1 or 2. Decoder fixed at 48 kHz (Opus internal rate).
       pub(crate) fn new(channels: u8) -> Result<Self, AudioError>;

       /// Decode one Opus packet to interleaved f32 samples at 48 kHz.
       /// Internal buffer sized for the maximum Opus frame (120 ms = 5 760 samples per channel).
       pub(crate) fn decode_packet(&mut self, packet: &[u8]) -> Result<Vec<f32>, AudioError>;
   }

   impl Drop for OpusDecoder { /* opus_decoder_destroy */ }
   ```

   Implementation notes (for the Coder):
   - `unsafe_libopus::opus_decoder_create(48_000, channels as i32, &mut err)` — sample rate must be one of {8000, 12000, 16000, 24000, 48000}; we always use 48 000.
   - `opus_decode_float` with `frame_size = 5760` (maximum: 120 ms at 48 kHz). The return value is the actual number of samples decoded per channel; truncate the buffer accordingly. `decode_fec = 0` (no forward-error-correction; not applicable to local files).
   - Map non-`OPUS_OK` returns to `AudioError::Decode(format!("opus: {code}"))`. Use `unsafe_libopus::opus_strerror` if you want human-readable text; ASCII-only.
   - The struct's `OpusDecoder` pointer is owned; mark the struct `Send` only if needed by callers. Initial scope: `decode.rs` holds it on the stack inside a single function call, so no `Send`/`Sync` impls are required.
   - All `unsafe` code lives in this module. Any pointer math, raw FFI call, or `*mut` handling is here, not in `decode.rs`.

4. **Branch the decode path in `src/decode.rs::decode_interleaved`.** After the track is selected (around line 65) and before `make` is called on the codec registry (line 78), add an Opus branch:

   ```rust
   if codec_params.codec == symphonia::core::codecs::CODEC_TYPE_OPUS {
       return decode_opus_track(&mut format, track_id, &codec_params);
   }
   ```

   Implement `fn decode_opus_track(format: &mut Box<dyn FormatReader>, track_id: u32, params: &CodecParameters) -> Result<(Vec<f32>, u32, u16), AudioError>` in the same module:

   1. **Parse OpusHead** from `params.extra_data` (`&[u8]`, RFC 7845 §5.1, `≥ 19` bytes — else `Decode("OpusHead too short: N bytes")`):
      - bytes 0..8: magic `b"OpusHead"` — else `Decode("missing OpusHead")`
      - byte 8: version — must be `1`, else `Decode("unsupported OpusHead version: N")`
      - byte 9: channel count — must be `1` or `2`, else `Decode("unsupported channel count: N")`
      - bytes 10..12: pre-skip (u16 LE) — used (see step 4 below)
      - bytes 12..16: input sample rate (u32 LE) — read for completeness, **ignored** (decoder always runs at 48 kHz)
      - bytes 16..18: output gain (i16 LE, Q7.8 dB) — read for completeness, **ignored** (always 0 in browser output and our fixture)
      - byte 18: channel mapping family — must be `0`, else `Decode("unsupported channel mapping family: N")`. Mapping family 0 has no channel mapping table; bytes ≥ 19 are not read.
   2. Construct `OpusDecoder::new(channels)`.
   3. Loop `format.next_packet()` exactly like the existing symphonia loop (same EOF / track-id / error handling). For each matching packet call `decoder.decode_packet(packet.data())` and append samples to an interleaved buffer.
   4. After the loop, drop the first `pre_skip * channels as usize` samples from the front (Opus pre-skip). If the buffer is shorter than that, return `Decode("packet stream shorter than pre-skip")`.
   5. Return `(interleaved, 48_000, channels as u16)` — downstream `downmix_to_mono` and `resample_to_target` consume it identically to the symphonia path.

5. **Filter track selection for audio (R2).** Both the default-track path and the fallback in `decode_interleaved` (lines 56-65) must require an audio track — `default_track()` can legitimately return a video track when the container places video first and marks it as the default, which is common in real-world MP4 files. Define a local predicate:

   ```rust
   fn is_audio(t: &symphonia::core::formats::Track) -> bool {
       t.codec_params.codec != CODEC_TYPE_NULL
           && t.codec_params.sample_rate.is_some()
   }
   ```

   Replace the current `default_track().or_else(...)` block with:
   ```rust
   let track = format.default_track()
       .filter(|t| is_audio(t))
       .or_else(|| format.tracks().iter().find(|t| is_audio(t)))
       .ok_or_else(|| AudioError::Decode("no audio track in stream".into()))?;
   ```
   No effect on existing fixtures (all single-track audio). The error message changes from "no decodable track" to "no audio track" when the input has only non-audio streams.

6. **Add decode tests in `src/decode.rs`.** Add at the top of `mod tests`:
   ```rust
   const WEBM: &[u8] = include_bytes!("../fixtures/eins-zwei-drei.webm");
   const M4A:  &[u8] = include_bytes!("../fixtures/eins-zwei-drei.m4a");
   ```
   Two new format-integration tests:
   ```rust
   #[test] fn decode_webm_opus_to_pcm16k() { assert_valid_pcm16k(&decode_to_pcm16k(WEBM).unwrap()); }
   #[test] fn decode_m4a_aac_to_pcm16k()   { assert_valid_pcm16k(&decode_to_pcm16k(M4A).unwrap()); }
   ```
   Extend `fixtures_have_consistent_length` to include both new files and assert the same `< 2048`-sample divergence bound across all five. (A tighter direct length comparison between MP3 and WebM is not used — Opus encoders pad to a 20 ms frame boundary at the tail, producing a legitimate length difference on top of pre-skip; a brittle threshold would mask real bugs in *either* direction.)

   **Deterministic R1 coverage** — add two synthetic unit tests in `src/opus.rs` (the module owning OpusHead parsing and the pre-skip helper). These cover the failure mode without depending on real-fixture lengths:

   ```rust
   #[test]
   fn parse_opus_head_extracts_pre_skip_and_channels() {
       let mut head = vec![0u8; 19];
       head[0..8].copy_from_slice(b"OpusHead");
       head[8] = 1;                                          // version
       head[9] = 2;                                          // channels
       head[10..12].copy_from_slice(&312u16.to_le_bytes());  // pre_skip
       head[12..16].copy_from_slice(&48_000u32.to_le_bytes()); // input sample rate (ignored)
       head[16..18].copy_from_slice(&0i16.to_le_bytes());    // output gain (ignored)
       head[18] = 0;                                         // mapping family
       let parsed = parse_opus_head(&head).expect("valid OpusHead");
       assert_eq!(parsed.pre_skip, 312);
       assert_eq!(parsed.channels, 2);
   }

   #[test]
   fn parse_opus_head_rejects_nonzero_mapping_family() {
       let mut head = vec![0u8; 19];
       head[0..8].copy_from_slice(b"OpusHead");
       head[8] = 1;
       head[9] = 1;
       head[18] = 1;  // mapping family 1 — rejected
       assert!(matches!(parse_opus_head(&head), Err(AudioError::Decode(_))));
   }

   #[test]
   fn pre_skip_drops_correct_prefix_for_stereo() {
       // 1000 interleaved frames at 2 channels = 2000 samples. pre_skip is in
       // 48 kHz frames, so the helper drops pre_skip * channels samples.
       let mut buf: Vec<f32> = (0..2000).map(|i| i as f32).collect();
       apply_pre_skip(&mut buf, /*pre_skip=*/100, /*channels=*/2);
       assert_eq!(buf.len(), 1800);
       assert_eq!(buf[0], 200.0);  // first surviving sample is original index 200
   }
   ```

   Required surface to make the above compile (extends the surface listed in Step 3):
   ```rust
   pub(crate) struct OpusHead { pub pre_skip: u16, pub channels: u8 }
   pub(crate) fn parse_opus_head(bytes: &[u8]) -> Result<OpusHead, AudioError>;
   pub(crate) fn apply_pre_skip(buf: &mut Vec<f32>, pre_skip: u16, channels: u8);
   ```
   `apply_pre_skip` is `.drain(..pre_skip as usize * channels as usize)` with a saturating check; if the buffer is shorter than `pre_skip * channels`, the function returns the buffer truncated to empty (the caller — `decode_opus_track` in Step 4 — already errors with `Decode("packet stream shorter than pre-skip")` before invoking the helper, so this branch is defensive only).

7. **Add one end-to-end inference test in `src/inference.rs`.** Mirror `end_to_end_eins_zwei_drei` (line 292) with a new `eins_zwei_drei_via_webm` test that loads `fixtures/eins-zwei-drei.webm`, decodes via `decode_to_pcm16k`, builds an `InferenceHandle` via `ensure_tiny()` (the existing helper at `src/inference.rs:247`), transcribes with `language=Some("de")`, joins the chunks, and runs `assert_eins_zwei_drei` on the joined text. One new test is sufficient — the goal is to prove the Opus path reaches ct2rs intact, not to re-test the model. The AAC path is covered by the decode test in Step 6 (AAC reuses the proven symphonia path; no new inference variable).

8. **Update `LICENSE-THIRD-PARTY`.** Append after the symphonia block:
   ```
   ─────────────────────────────────────────────
   libopus (via unsafe-libopus)  —  https://github.com/xiph/opus
   License:   BSD-3-Clause
   Version:   1.3.1 (transpiled by unsafe-libopus 0.2.0)
   Copyright (c) 2001-2011, Xiph.Org Foundation and contributors

   The unsafe-libopus crate is a c2rust transpile of libopus 1.3.1. The
   BSD-3-Clause licence applies to both the original C sources and the
   transpiled Rust code.
   ─────────────────────────────────────────────
   ```

Implementation ends here. Doc Update and Archive (commit) happen in the workflow phases after Validation per `CLAUDE.md §5–§7` and Hard Rule 12 — see the *Doc Update (post-validation)* and *Archive (post-validation)* sections below.

## Verification

Run end-to-end on macOS arm64 (the v1 build host):

1. `cargo build --release` → green.
2. `cargo build --release --features napi` → green; existing `cadmus.darwin-arm64.node` still produced cleanly.
3. `cargo test` (no features) → all existing tests green; the two new decode tests pass (`decode_webm_opus_to_pcm16k`, `decode_m4a_aac_to_pcm16k`); `fixtures_have_consistent_length` reports ≤ 2 048-sample divergence across five formats.
4. `cargo test --features napi` → all `src/*` unit tests pass including the new `eins_zwei_drei_via_webm` end-to-end test (requires `tests/.cadmus-cache/` with tiny model — `ensure_tiny()` downloads it on first run).
5. `npm test` → unchanged green (no Node-side surface change in this plan).
6. `cargo package --list --allow-dirty` → additionally lists `src/opus.rs`, `fixtures/eins-zwei-drei.webm`, `fixtures/eins-zwei-drei.m4a`. Still no `package.json`, no `index.ts`/`.js`/`.d.ts`, no `cadmus.*.node`, no `node_modules/`, no `docs/`.
7. `npm pack --dry-run` → identical entry list to before this plan (fixtures and `src/opus.rs` are crate-side; npm allowlist is unaffected).
8. `ffprobe fixtures/eins-zwei-drei.webm` confirms codec=opus / 48 kHz / mono; `ffprobe fixtures/eins-zwei-drei.m4a` confirms codec=aac / 44 100 Hz / mono.
9. **R1 (pre-skip) deterministic coverage.** The three synthetic unit tests added in Step 6 (`parse_opus_head_extracts_pre_skip_and_channels`, `parse_opus_head_rejects_nonzero_mapping_family`, `pre_skip_drops_correct_prefix_for_stereo`) run as part of `cargo test` in item 3 and must all pass. These prove the OpusHead parser extracts the right field at the right offset and that the pre-skip helper drops `pre_skip * channels` samples — independent of any fixture-length comparison.
10. `LICENSE-THIRD-PARTY` contains the libopus BSD-3 block (added in Step 8).

If any of the above fails, the implementation is incomplete. Stop and report only if the plan itself turns out to be wrong (Rule 7), not for routine fix-ups.

## Doc Update (post-validation)

Per `CLAUDE.md §6` and Hard Rule 12, these doc edits happen **after** Validation succeeds, not as part of Implementation:

- `docs/architecture.md §1` (Technology Stack) — add a row for `unsafe-libopus` (rationale: symphonia 0.5.4 Opus is a stub; pure-Rust transpile of libopus 1.3.1; BSD-3-Clause). Extend the symphonia row to mention the new feature set (`mkv`/`isomp4`/`aac`) and the formats it covers.
- `docs/architecture.md §8.1` (The Fixtures) — update from three formats / three rates to **five formats** (mp3, wav, flac, webm, m4a) at three distinct rates (22 050, 44 100, 48 000 — m4a shares 44 100 with wav; webm shares 48 000 with flac). Note that webm and m4a are derived from the mp3 master via ffmpeg, preserving the "same master" guarantee.
- `docs/definition.md` — no changes (format support is not enumerated in definition.md; "Format detected from bytes" continues to hold).
- `docs/bug.kanban.md` / `docs/backlog.kanban.md` — no new cards expected from this plan. If during Implementation a new accepted-deviation surfaces (e.g. an HE-AAC input failing — see A3), file it then.

## Archive (post-validation, post-Doc-Update)

Per `CLAUDE.md §7`, after Doc Update:

- Move `docs/PLAN_browser_audio_formats.md` to `docs/archive/`.
- Create a single Git commit, message: `feat(audio): WebM/Opus + MP4/AAC-LC via symphonia features and unsafe-libopus`. Stage files individually (no `git add .`):
  - `Cargo.toml`, `Cargo.lock`
  - `src/lib.rs`, `src/opus.rs`, `src/decode.rs`, `src/inference.rs`
  - `LICENSE-THIRD-PARTY`
  - `docs/architecture.md`
  - `docs/archive/PLAN_browser_audio_formats.md` (moved)
  - `fixtures/eins-zwei-drei.webm`, `fixtures/eins-zwei-drei.m4a`
