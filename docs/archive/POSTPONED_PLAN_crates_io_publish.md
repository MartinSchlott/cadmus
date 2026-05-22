# PLAN — Publish `cadmus` to crates.io (first manual release, v1.1.1)

## Status — POSTPONED (2026-05-22)

Postponed by Human decision before execution. **Nothing was published** —
`cadmus` is not on crates.io. Step 1's `Cargo.toml` metadata edit was reverted,
so this plan starts clean from Step 1.

While postponed, the three core docs were corrected to stop claiming the crate
is on crates.io — `README.md`, `definition.md`, and `architecture.md` now say
"not yet on crates.io" and describe the crate as a git dependency. This inverts
the Context & Goal premise "no doc text needs to be softened": the text *has*
been softened. Resuming this plan re-hardens it — the Doc Update section's list
of crates.io-mentioning lines is still the correct set of edits, applied in the
opposite direction (drop "not yet", restore the crates.io claim).

Tracked in `docs/backlog.kanban.md` (Open column).

## Context & Goal

Cadmus ships two artifacts from one repository root (D27): the npm package
`@ai-inquisitor/cadmus` (published, currently v1.1.1) and the Rust crate
`cadmus` on crates.io. The npm side is live; the crates.io side is **not** —
nothing has ever been published there.

This is a live documentation drift. Three core documents already assert the
crate is on crates.io:

- `README.md:5` — "as `cadmus` on crates.io for Rust callers"
- `definition.md:23` — "`cadmus` (crates.io) — Rust library wrapping CTranslate2 …"
- `architecture.md:48` — "the rlib (for Rust consumers via crates.io)"

Publishing resolves the drift by making those statements true — no doc text
needs to be softened.

**Goal:** publish the crate `cadmus` at version **1.1.1** to crates.io as a
one-time **manual** `cargo publish` (Human decision: not automated into the
release workflow). The crate name `cadmus` was verified free on crates.io on
2026-05-21 (`GET /api/v1/crates/cadmus` → HTTP 404).

The publish is `cargo publish` — no source/API changes. The only file edit is
adding crates.io metadata fields to `Cargo.toml`.

## Breaking Changes

**No.** No code, no public API, no behaviour change. Purely a new distribution
artifact plus four metadata lines in `Cargo.toml`.

Note — not a *breaking* change, but an **irreversible** one: once published,
version 1.1.1 is permanently immutable on crates.io (it can only be *yanked*,
not overwritten or re-uploaded) and the crate name `cadmus` is permanently
claimed by the publishing account. This is why Step 5 is a `BREAK`.

## Reference Patterns

- `docs/archive/CONCEPT_v1_buildout.md` — D27 (explicit packaging boundaries)
  and the original Release Runbook, whose step 6 was `cargo publish`. This plan
  is that step, executed for the first time.
- `Cargo.toml:1-23` — the existing `[package]` section and the D27 `include`
  allowlist. The allowlist is already correct and is **not** changed here.

## Dependencies

**None.** No new packages, no system tools, no build changes. `cargo` (already
present — `cargo 1.95.0` on the host) is the only tool. `cargo publish`
additionally needs a crates.io API token configured on the Human's machine
(see Step 5).

## Assumptions & Risks

- **A1.** The crate builds cleanly at HEAD on the build host (macOS arm64):
  `cargo build --release` is green. The metadata edit in Step 1 is the only
  change this plan makes to tracked source.
- **A2.** The Human has, or will create, a crates.io account (login via GitHub
  at crates.io) with a verified email — prerequisite for token creation in
  Step 5.
- **A3.** `categories = ["multimedia::audio", "external-ffi-bindings"]` are
  valid crates.io category slugs (both long-standing — `multimedia::audio` is
  used by `cpal`/`rodio`, `external-ffi-bindings` by `*-sys` crates). crates.io
  silently drops unknown categories with a warning rather than rejecting the
  upload, so a slug error is non-fatal even if it slipped through.
- **R1. docs.rs documentation build will almost certainly fail.** docs.rs
  builds every published crate in a sandbox with **no network access** and a
  build-time limit (~15 min). Cadmus depends on `ct2rs`, whose build script
  compiles CTranslate2 from source via CMake (5–25 min per `README.md:140`),
  and the Linux feature set pulls oneMKL over the network. Both conditions
  break the docs.rs sandbox. **Consequence:** the docs.rs *page* for the crate
  will show a failed build; the crate itself publishes normally and
  `cargo add cadmus` works for consumers regardless. This is a known, accepted
  limitation of native-wrapper crates (`tch`, `opencv` have the same). It
  cannot be fixed here — `ct2rs` is a hard, non-optional dependency.
  **Mitigation:** Step 1 sets `documentation` to the GitHub repository so the
  crates.io "Documentation" link works even with docs.rs red. Tracked as a
  backlog card (Doc Update).
- **R2.** First `cargo build` for a Rust consumer is a 5–25 min cold ct2rs
  CMake build. Already documented in `README.md:140`. No action.
- **R3.** The name `cadmus` is free but **not reservable**. If it is claimed by
  someone else between now and Step 5, `cargo publish` fails with a name
  conflict. The Coder stops and reports (Rule 7) — a different name would
  contradict the three docs cited in Context & Goal and is a Human decision.
- **R4.** crates.io enforces a packaged-size limit (~10 MB). `fixtures/` totals
  436 KB uncompressed; the full Rust-only source tarball compresses to well
  under 1 MB. Step 3 verifies the actual `.crate` size.

## Steps

Single phase, executed on the build host (macOS arm64). Implementation runs
through Step 4, then **stops at the `BREAK` in Step 5**. Step 6 resumes only
after the Human confirms the publish succeeded. Doc Update and Archive happen
in their own workflow phases after Validation (see sections below).

1. **Add crates.io metadata to `Cargo.toml`.** Insert four fields into the
   `[package]` section, immediately after `readme = "README.md"` (line 7) and
   before the `# D27 allowlist.` comment block:

   ```toml
   readme = "README.md"
   repository = "https://github.com/MartinSchlott/cadmus"
   documentation = "https://github.com/MartinSchlott/cadmus"
   keywords = ["whisper", "transcription", "speech-to-text", "ctranslate2", "audio"]
   categories = ["multimedia::audio", "external-ffi-bindings"]
   ```

   `repository` is expected crates.io hygiene. `documentation` points at the
   repo deliberately (R1 — docs.rs is expected to fail; a working link beats a
   broken one). `keywords`/`categories` are for discoverability. Do **not** add
   a `rust-version` field — `README.md:190` deliberately documents MSRV as
   "current stable Rust at release time", and pinning one would contradict
   that. Do not touch the `include` allowlist — D27 already has it right.

2. **Confirm the build is unaffected.** `cargo build --release` → green (warm
   build; the metadata edit cannot affect compilation). Confirm `Cargo.lock` is
   **unchanged** — adding `repository`/`keywords`/`categories`/`documentation`
   does not alter the dependency graph, so `Cargo.lock` must not be modified
   (`git status` shows it clean). If `Cargo.lock` did change, something else
   moved — stop and report (Rule 7).

3. **Verify the published tarball contents and size.**
   - `cargo package --list --allow-dirty` → the file list must **contain** the
     full D27 allowlist: `Cargo.toml`, `Cargo.lock`, `build.rs`, `src/**/*.rs`,
     `tests/**/*.rs`, the five `fixtures/eins-zwei-drei.{mp3,wav,flac,webm,m4a}`,
     `LICENSE`, `LICENSE-THIRD-PARTY`, `README.md`. It will **also** contain
     files Cargo generates into every published tarball — at minimum
     `Cargo.toml.orig` (the verbatim pre-normalization manifest), and typically
     `.cargo_vcs_info.json` (the VCS commit record). Those are expected and
     correct; do **not** assert an exact file count or "exactly the allowlist".
     The gate is the **negative** assertion: there must be **no**
     `package.json`, no `index.ts`/`.js`/`.d.ts`, no `cadmus.*.node`, no
     `node_modules/`, no `docs/`.
   - `cargo package --allow-dirty` (builds the verify tarball) → produces
     `target/package/cadmus-1.1.1.crate`. Check its size with
     `ls -la target/package/cadmus-1.1.1.crate` — must be well under 10 MB
     (expected: < 1 MB).

   **Why `--allow-dirty`:** this workflow commits only at Archive — after
   Validation and Doc Update (CLAUDE.md §6–§7) — so while Steps 3–5 run the
   worktree is intentionally dirty: `Cargo.toml` is modified (Step 1) and
   `docs/PLAN_crates_io_publish.md` is untracked. `cargo package` and
   `cargo publish` refuse to operate on a dirty VCS worktree unless
   `--allow-dirty` is passed. This matches the project convention —
   `docs/archive/PLAN_browser_audio_formats.md` verification step 6 uses
   `cargo package --list --allow-dirty`.

4. **Pre-flight: `cargo publish --dry-run --allow-dirty`.** This packages the
   crate and runs the full verify build **without uploading and without
   requiring a token** (`--allow-dirty` for the same reason as Step 3).
   The verify build is a cold ct2rs CMake build — expect 5–25 min; allow the
   command a long timeout. It must exit 0 with no errors (it ends with a
   `warning: aborting upload due to dry run`, which is expected and is *not* a
   failure). Resolve any *metadata* warnings it surfaces (missing field, bad
   category) by editing `Cargo.toml`; warnings unrelated to this plan's scope
   are noted, not fixed (Rule 8). A green dry-run is the gate that protects the
   irreversible Step 5.

   **Implementation stops here.** The Coder reports: Step 1 edit applied,
   Steps 2–4 green, `.crate` size, dry-run output summary — then waits.

5. **`BREAK` — Human performs the irreversible publish.**

   **Justification (required by CLAUDE.md §2):** `cargo publish` is an
   irreversible external side effect — it permanently claims the crate name
   `cadmus` and makes version 1.1.1 immutable on crates.io (yank-only, never
   overwritable). It also requires the Human's personal crates.io account
   credentials, which the Coder neither has nor should have. The Coder cannot
   and must not perform this step.

   The Human:
   1. Ensures a crates.io account exists (log in via GitHub at
      <https://crates.io>) with a **verified email**.
   2. Creates an API token at <https://crates.io/settings/tokens> with scope
      **publish-new** (first publish of a new crate) — `publish-update` may be
      added for future releases. Copies the token.
   3. `cargo login` and pastes the token (writes `~/.cargo/credentials.toml` —
      no such file exists yet on this host).
   4. Runs `cargo publish --allow-dirty` (the worktree is still uncommitted —
      Archive happens after Validation; same reason as Step 3). The default
      re-runs the verify build (another 5–25 min);
      `cargo publish --allow-dirty --no-verify` is acceptable here because
      Step 4's dry-run already verified the identical bytes and nothing changed
      since — the Human chooses.
   5. Confirms in chat that the publish succeeded.

   On confirmation, the Coder resumes at Step 6. If `cargo publish` fails on a
   name conflict (R3) or any other error, the Coder stops and reports (Rule 7).

6. **Post-publish verification (Coder, after Human confirmation).**
   - `GET https://crates.io/api/v1/crates/cadmus` → HTTP 200, JSON reports
     `max_version`/`newest_version` `1.1.1`.
   - The page <https://crates.io/crates/cadmus> renders with the description,
     keywords, and categories from Step 1.
   - Note: docs.rs will queue a build for the new release; per R1 it is
     expected to fail. That is not a blocker and not a regression.

Implementation ends after Step 6. Doc Update and Archive happen in their own
workflow phases after Validation, per CLAUDE.md §6–§7 and Hard Rule 12.

## Verification

Run on the build host (macOS arm64):

1. `cargo build --release` → green; `git status` shows `Cargo.lock` clean
   (Step 2).
2. `cargo package --list --allow-dirty` → contains the full D27 allowlist plus
   the Cargo-generated `Cargo.toml.orig` (and typically `.cargo_vcs_info.json`);
   no `package.json`, no `index.*`, no `*.node`, no `node_modules/`, no `docs/`
   (Step 3).
3. `cargo package --allow-dirty`, then
   `ls -la target/package/cadmus-1.1.1.crate` → file exists, size well under
   10 MB (Step 3).
4. `cargo publish --dry-run --allow-dirty` → exits 0, only the expected dry-run
   abort warning (Step 4).
5. **Post-publish** (after the `BREAK`): `GET /api/v1/crates/cadmus` returns
   HTTP 200 with version `1.1.1`; the crates.io page renders (Step 6).

If any of items 1–4 fails before the `BREAK`, the implementation is incomplete —
fix and re-run. Stop and report only if the plan itself is wrong (Rule 7), or
if `cargo publish` fails at Step 5.

## Doc Update (post-validation)

Per CLAUDE.md §6 and Hard Rule 12, these edits happen **after** Validation:

- `README.md` — in the "Build from source" / releases paragraph (around
  line 144), add one sentence: the GitHub Actions `Release` workflow publishes
  the npm package only; the crates.io crate is published manually with
  `cargo publish`. No other README change — the crates.io claim at line 5 is
  now accurate.
- `docs/architecture.md` — two edits in the publishing/build area:
  1. In the packaging description (§2, around lines 106–109), add a one-line
     note that the crates.io artifact is published manually with `cargo publish`
     while the npm artifact is published by the `Release` workflow.
  2. Replace the now-stale §6 paragraph **"### Verification is local, not CI"**
     (lines 224–226). It asserts "There are no GitHub Actions workflows" —
     false since `.github/workflows/release.yml` exists (the "GitHub Actions /
     CI matrix migration" card sits in the backlog Done column). Rewrite it to
     the current shape: the `Release` workflow
     (`.github/workflows/release.yml`) builds the prebuilt `.node` for all
     three platforms on native runners, bumps the npm version, commits the
     binaries, tags, and publishes to npm with provenance; the crates.io crate
     is **not** part of that workflow and is published manually with
     `cargo publish`. This is a pre-existing drift — the CI migration updated
     the backlog but not this paragraph — corrected here because this plan
     edits the same publishing area and leaving the contradiction would make
     `architecture.md` self-inconsistent (Truth Triangle / Hard Rule 4).
- `docs/definition.md` — no change (`definition.md:23` is now accurate).
- `docs/bug.kanban.md` — no new cards.
- `docs/backlog.kanban.md` — add two cards:
  1. **Open column** — "Automate crates.io publish in the release workflow":
     `cargo publish` is currently a manual post-release step (Human decision).
     `release.yml` bumps and publishes npm only; each release that should reach
     crates.io needs a manual `cargo publish`. Relates to existing card
     `m598ahwzunu8ag01j29lowe8` ("Bump Cargo.toml and package.json versions
     together") — automating both is one combined improvement to the release
     pipeline. Needs a `CARGO_REGISTRY_TOKEN` secret and a verify build on the
     runner.
  2. **Someday column** — "docs.rs build for `cadmus` fails": records R1 — the
     docs.rs sandbox cannot run the ct2rs CMake build (no network, build
     timeout), so the docs.rs page shows a failed build. The crate and
     `cargo add cadmus` are unaffected; `documentation` in `Cargo.toml` points
     at the GitHub repo as the working alternative. Pick up only if a hosted
     Rust API reference becomes worth the effort (relates to existing card
     `amzkuc859di3vayju8ugp6mb`, "Documentation site / hosted docs").

## Archive (post-validation, post-Doc-Update)

Per CLAUDE.md §7, after Doc Update:

- Move `docs/PLAN_crates_io_publish.md` to `docs/archive/`.
- Create a single Git commit. Subject:
  `chore(release): publish cadmus 1.1.1 to crates.io`. Stage files individually
  (no `git add .`):
  - `Cargo.toml`
  - `README.md`
  - `docs/architecture.md`
  - `docs/backlog.kanban.md`
  - `docs/archive/PLAN_crates_io_publish.md` (moved)
