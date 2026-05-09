# Backlog
**Ideas and pending work for Cadmus**

id: rpqrs4lyfvb86gmhlmteln14
template: backlog

Pending work and ideas for Cadmus that are not part of the active plan.
The Someday column parks speculative items that probably will not be
implemented, so the committed columns stay honest.

## Someday
id: u9pqf6tijgyqo6eidedpm8ua

Ideas that probably will not happen, but deserve to be written down.

### Windows x86_64 build (`x86_64-pc-windows-msvc`)
id: {new}
priority: low

Third platform target deferred at v1 Concept Closeout (CONCEPT_v1_buildout.md
D8 / Out of Scope). Needs a Windows host with MSVC and the `intel-onemkl-prebuild`
MSVC artifacts. Cross-compilation from macOS or Linux is not feasible for the
ct2rs + oneMKL stack. The Cargo manifest, napi-rs target list, and TS dispatcher
are all written for two platforms only — adding Windows means extending all three
plus a third committed `.node` (`cadmus.win32-x64-msvc.node`). Reactivate when a
Windows build host becomes available.

### GPU inference (CUDA / Metal / Vulkan)
id: {new}
priority: low

v1 is CPU-only by deliberate decision (D7). The `ct2rs` features `cuda`,
`cudnn`, `cuda-dynamic-loading` are explicitly excluded from the per-platform
feature sets. Adding GPU support is a deployment-shape change, not a tweak:
binary size, distribution, and the "no system install" promise all shift.
Accept only as a separate concept.

### Streaming / real-time partial transcription
id: {new}
priority: low

Cadmus v1 transcribes complete audio buffers and returns the full
`TranscriptResult` synchronously (or as a single `Promise` on the JS side). No
incremental segment delivery, no microphone-style streaming. Adding this means
a different API shape (`AsyncIterable<Segment>` on JS, `mpsc`-channel-style on
Rust) and a different ct2rs invocation pattern. Out of v1 scope.

### Word-level timestamps
id: {new}
priority: low

Whisper's segment-level `<|t|>` tokens are parsed and surfaced. Word-level
timestamps require a different decoding mode (cross-attention alignment) that
ct2rs may or may not expose. Out of v1 scope per definition.md §6.

### Model integrity verification (checksums, signatures)
id: {new}
priority: low

`download_model` writes downloaded files without verifying their content
against a known checksum or signature. Definition.md §5 already states that
download integrity is not verified; a truncated download surfaces later as
`Load`. Adding SHA256 verification would mean shipping per-file digests in the
catalog and matching them after download. Worth doing before v1.0 is declared
production-grade for adversarial environments.

### V8 finalizer / GC-driven `free()` on the JS side
id: {new}
priority: low

JS-side `free()` is mandatory; forgetting it leaks the native instance for the
process lifetime. napi-rs supports finalizers (`#[napi(finalize)]`) which would
release the native handle when V8 collects the wrapper. Not added in v1
deliberately — finalizers run non-deterministically and would mask leaks during
development. Reconsider once the API is stable and consumer feedback warrants
it.

### Linux-arm64 and macOS-x64 builds
id: {new}
priority: low

Two additional platform variants outside v1's two-target scope (`aarch64-apple-darwin`,
`x86_64-unknown-linux-gnu`). Linux-arm64 needs the oneMKL alternative for ARM
(no MKL there — likely `ruy` + OpenBLAS) plus an arm64 build host or QEMU.
macOS-x64 is straightforward but adds a fourth committed `.node`. Both are
"if a user shows up needing it" rather than "we should ship this".

### Benchmarking suite as a public artifact
id: {new}
priority: low

Internal performance characteristics of CTranslate2 / Whisper are well-known
upstream; Cadmus's pipeline overhead (decode + downmix + resample) is small
relative to inference. A reproducible benchmark artifact (input audio, model
matrix, throughput numbers) would let consumers compare CPU configurations
honestly. Out of v1 scope; not even drafted.

### Documentation site / hosted docs
id: {new}
priority: low

`README.md` plus `docs/definition.md` and `docs/architecture.md` are the
v1 documentation surface. A hosted site (mdbook, docusaurus, or rustdoc-only)
would give Cadmus a public landing page beyond GitHub. Wait until adoption
warrants it.

## Open
id: pqhx4mr761392sc6t42lk3ji

Considered, scoped enough, ready to be picked up.

### Linux x86_64 follow-up build
id: {new}
priority: medium

Was Plan 7 in CONCEPT_v1_buildout.md — deferred at concept time and now
captured as a backlog item at Concept Closeout. Cadmus 1.0.0 ships a macOS
arm64 binary only; Linux x86_64 plumbing (`Cargo.toml` per-target ct2rs
features, `package.json.files` allowlist, `index.ts` platform dispatch) is
already wired but the Linux `.node` is not built yet.

Scope: on an `x86_64-unknown-linux-gnu` host, run Phase B Steps 12–17 of
`docs/archive/PLAN_skeleton.md` plus the Linux-half of every Plan 2–6
verification:

- Skeleton: `cargo build --release [--features napi]`, `cargo test [--features napi]`,
  `npm install && npm run build`, `cargo package --list`, `npm pack --dry-run`
  (now showing both `.node` files), commit `cadmus.linux-x64-gnu.node`.
- Audio pipeline: `cargo test` for the symphonia/rubato decode paths against
  the three fixtures (mp3/wav/flac).
- Model storage: `cargo test` for the HuggingFace downloader (idempotent
  against the test cache).
- Inference core: end-to-end inference test (tiny model, eins-zwei-drei
  fixture, digits-set assertion) plus the three D4 tests
  (free-after-free, free-during-inflight, concurrent transcribe).
- Public API: integration tests in `tests/public_api.rs` (rlib path, no napi
  feature) — see also `architecture.md §8` on the two Rust test modes.
- Napi surface: `npm test` (`node --test`) against the freshly-built Linux
  `.node` covering version, catalog, find, load+transcribe, free-after-free,
  free-during-inflight, concurrent transcribe.

Prerequisites: Linux x86_64 host with `build-essential`, `cmake`, `pkg-config`,
Node ≥ 22, Rust stable. First `cargo build --features napi` triggers the
CTranslate2 + oneMKL + DNNL CMake build (10–25 min on a fresh host).

Done when both `.node` binaries are present in HEAD, all cargo and npm
verifications green on Linux, `npm pack --dry-run` lists both binaries.

### GitHub Actions / CI matrix migration
id: {new}
priority: low

CONCEPT_v1_buildout.md D25 made v1 explicitly CI-free: verification is local
on each build host, Release Runbook is a manual six-step sequence. None of
the v1 decisions block CI migration — the same six steps become a workflow
file and the manual runs become automatic. Trigger is concept R5: "discipline,
not automation. If discipline slips often, that is the trigger to introduce
GitHub Actions". Pick this up when the local-discipline cost crosses the
CI-maintenance cost.

### Expose CTranslate2 version through ct2rs upstream — track and adopt
id: s4uvcn156fm4jtik1numnxqs
priority: medium

`ct2rs 0.9.18` does not expose the bundled CTranslate2 C++ library version
through any public Rust constant or function. The bundled version (`4.7.1`
at the time of writing) is only readable from
`ct2rs/CTranslate2/python/ctranslate2/version.py`, which is not on a stable
build-script-accessible path because ct2rs has no `[package].links` key —
so cargo does not surface the dep's source dir to our `build.rs`.

Result: `cadmus::version().ctranslate2` returns `""` until ct2rs grows a
public surface (e.g. `ct2rs::CTRANSLATE2_VERSION` or `ct2rs::ctranslate2_version()`).
Track the ct2rs upstream; once a public version surface lands, switch
`build.rs` from the empty fallback to the real value and drop this card.

PLAN_skeleton.md R1 Fallback B.

### Surface ct2rs internally-detected language token
id: yeuv5u80rh47llblsi17afd3
priority: low

`ct2rs 0.9.18`'s high-level `Whisper::generate(samples, None, ...)`
runs language detection internally
(`ct2rs/src/whisper.rs:131-170`) but drops the detected `lang_token`
after embedding it into the prompt prefix. The generated chunks
returned to the caller contain only model-output tokens, not the
prefix — so the detected language is unreachable from the public
ct2rs surface.

Track ct2rs upstream for either:
- An overload that returns the detected language alongside the
  chunks, or
- `Whisper::detect_language` exposed on the high-level wrapper
  (currently only on `sys::Whisper`, which requires self-built mel
  spectrograms).

When upstream lands either, drop the `severity: accepted` card in
`docs/bug.kanban.md` ("Detected language not surfaced when
TranscribeOptions::language == None") and rely on the existing
`detect_language_from_chunks` helper (already in
`src/inference.rs`).

### HTTP Range / resume on `download_model`
id: flna2x9g3w082f7ubsr06uod
priority: low

The downloader introduced in `PLAN_model_storage` writes downloaded
files in one shot. If a download is interrupted (network drop,
process crash, cooperative cancel), the partial file is deleted and
the next run downloads from byte zero. For `tiny` (~75 MB) that's
tolerable; for `large-v3` (~1.5 GB) on a flaky link it's painful.

Adding HTTP Range request support would let `download_model` resume
a partial download by sending `Range: bytes=N-` and appending to the
existing file. Requires the server to honour Range (HuggingFace's
CDN does), and a "is the partial file actually a prefix of the full
file" decision — the simplest is "if size matches Content-Length
already, treat as cached; if smaller, send Range; if larger, delete
and restart". Definition.md §5 already says download integrity is
not verified — Range support does not change that contract.

Open against a future plan; not part of v1's local-verification
flow which prefers the simpler "redownload on failure" path.

### Complete LICENSE-THIRD-PARTY across the full dep tree
id: {new}
priority: medium

`LICENSE-THIRD-PARTY` currently scopes itself to symphonia (MPL-2.0)
attribution per `architecture.md:65`. The bundled binaries embed
considerably more: CTranslate2 (MIT), ct2rs (MIT), Intel oneMKL via
`intel-onemkl-prebuild` (Intel Simplified Software License — requires
binary-redistribution attribution), napi-rs (MIT), ureq + rustls
(MIT/Apache-2.0/ISC), tokenizers (Apache-2.0 — NOTICE preservation),
rubato (MIT/Apache-2.0), and transitive deps. None of these block the
private-repo + public-npm + MIT model, but each carries its own
attribution duty in any binary distribution.

Scope:

- Run `cargo about generate` (or `cargo-license` + manual NOTICE
  composition) over the workspace to produce a complete
  `LICENSE-THIRD-PARTY` file covering every transitive dep with its
  licence text and upstream URL.
- Add the regeneration command as a step in the Release Runbook in
  `docs/CONCEPT_v1_buildout.md` (after `cargo package --list` /
  `npm pack --dry-run`, before `cargo publish`).
- Fix the comment on `architecture.md:65` to read
  `# third-party attributions for all bundled dependencies (regenerated via cargo-about)`
  instead of the current symphonia-only phrasing.
- Verify the generated file is included in both allowlists (D27):
  already in `Cargo.toml`'s `[package].include` and `package.json`'s
  `files`.

Done when both shipped tarballs (`cargo package`, `npm pack`)
contain a `LICENSE-THIRD-PARTY` that lists every bundled crate and
binary blob with attribution, and the Release Runbook makes
regeneration mandatory before publish.

## In Progress
id: tw80l0gyryxgw8p4rxkv055j

Being actively worked on.

## Done
id: x8vv0f33ci8qvea4z09xkqbt

Completed and shipped.

<!-- markdown-kanban
# Writers use id: {new} for new boards, columns, and cards.
# Processing systems replace {new} with generated IDs on parse.
name: backlog
description: |
  Tracks ideas and pending work through four stages: from rough
  wishlist (Someday), through deliberate intent (Open), to active
  work (In Progress), to delivery (Done).
columnsLocked: false
columns:
  - key: someday
    title: Someday
    description: |
      Ideas that probably will not happen, but deserve to be written
      down.
  - key: open
    title: Open
    description: Considered, scoped enough, ready to be picked up.
  - key: inprogress
    title: In Progress
    description: Being actively worked on.
  - key: done
    title: Done
    description: Completed and shipped.
cardFields:
  - key: priority
    type: select
    options:
      - none
      - low
      - medium
      - high
    description: |
      none — not yet decided
      low — nice to have, low impact if delayed
      medium — meaningful, should not sit indefinitely
      high — important, work on this before lower-priority items
-->
