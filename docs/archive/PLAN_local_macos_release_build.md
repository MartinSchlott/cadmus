# PLAN: Build macOS Release Binary Locally, CI Only Builds Linux + Windows

## Context & Goal

The release workflow (`.github/workflows/release.yml`) currently builds the
prebuilt napi binary for all three target platforms (`darwin-arm64`,
`linux-x64-gnu`, `win32-x64-msvc`) on GitHub-hosted runners. `macos-latest` is
billed at a 10× multiplier per minute, which is the dominant cost of each
release, while the Product Owner has a capable Apple-Silicon laptop sitting
right there.

**Goal:** move the `darwin-arm64` build off GitHub-hosted runners. `npm run
release` builds `cadmus.darwin-arm64.node` locally, commits + pushes it, then
triggers the workflow. CI only builds Linux + Windows, then commits the bumped
version together with all three binaries (the macOS one being already in the
tree from the prior push).

This deliberately replaces the previous one-button release with a two-step
flow: one local commit produced by `npm run release` (the macOS binary), one CI
commit produced by the workflow (Linux+Windows binaries + version bump). Both
land on `main` automatically.

## Breaking Changes

**No** breaking changes to the published package or its consumers. The
release-time developer experience changes:

- `npm run release` now performs a local build before triggering the workflow.
  It takes longer and requires a clean working tree on `main`.
- A clean `cadmus.darwin-arm64.node` build from cold (no `target/` cache) on
  the Product Owner's machine still takes minutes — same toolchain
  (`napi build --release`) the workflow ran.

No DB, env-var, or migration impact.

## Reference Patterns

- `.github/workflows/release.yml` — the existing workflow. The matrix entry for
  `darwin-arm64` is removed; everything else (publish job, verify step,
  artifact upload/download) stays.
- `package.json:39-46` — the existing `scripts.release`, `release:minor`,
  `release:major` entries that we will rewire.
- The existing publish job already does `actions/checkout@v6` with the latest
  `main`, then runs a "Verify all platform binaries present" step that fails
  the release if `cadmus.darwin-arm64.node` is missing. That same guard
  protects us if someone trips the local build.

## Dependencies

**None new.** The release script is plain Node ESM (the project is already
`"type": "module"` and targets Node ≥22) using only built-in modules
(`node:child_process`, `node:fs`, `node:process`). No new npm packages.

The `gh` CLI is already a prerequisite of the current `release` script — that
stays.

## Assumptions & Risks

**Assumptions:**

- The Product Owner runs `npm run release` from a clean working tree on `main`
  with a configured Apple-Silicon Mac. Pre-flight checks enforce this and
  abort with a clear error otherwise.
- `git push` on `main` succeeds without further auth prompts in normal cases
  (matches today's setup — the current workflow also pushes to `main`).
- The npm binary file `cadmus.darwin-arm64.node` is and stays tracked in git
  (it's listed in `package.json` `files` and is not in `.gitignore` — already
  the case).

**Risks:**

- *Risk:* Developer pushes a stale `cadmus.darwin-arm64.node` (e.g. forgot to
  rebuild after changing Rust code) and the npm package ships with a binary
  that doesn't match the source.
  *Mitigation:* `npm run release` **always** rebuilds before committing. No
  "skip build" flag. The post-build `git add` makes any drift visible in the
  commit (or no-op if the binary truly didn't change).
- *Risk:* Local push succeeds, but the subsequent workflow trigger fails (e.g.
  `gh` auth missing). Result: a "prebuilt darwin-arm64 for upcoming release"
  commit sits on `main` with no release behind it.
  *Mitigation:* Cosmetic only — the next release picks up where it left off,
  and the orphan commit causes no harm (binary content is correct, just not
  yet shipped). The script reports the trigger failure loudly so the Product
  Owner can re-run `gh workflow run release.yml --field bump=<x>` manually.
- *Risk:* Concurrent macOS-binary commit and release-workflow commit race on
  push.
  *Mitigation:* The macOS commit lands **before** the workflow is triggered
  (sequential steps in the script), and the workflow's `concurrency` group
  (`release`) already prevents two release workflows running in parallel.

## Steps

### 1. Add `scripts/release.mjs`

Create a new file `scripts/release.mjs` (the `scripts/` directory does not yet
exist; the script will be the first inhabitant). The file is a Node ESM
program with a shebang (`#!/usr/bin/env node`) and the following behavior, in
order:

1. **Parse bump argument** — first positional CLI arg, must be one of `patch`,
   `minor`, `major`. Default `patch`. Reject anything else with a clear error.
2. **Pre-flight: branch must be `main`** — run `git rev-parse
   --abbrev-ref HEAD`; abort if not `main`.
3. **Pre-flight: working tree clean** — run `git status --porcelain`; abort if
   output is non-empty (no uncommitted changes, no untracked files).
4. **Pre-flight: in sync with `origin/main`** — run `git fetch origin main`,
   then verify `git rev-parse HEAD` equals `git rev-parse origin/main`. Abort
   otherwise (Product Owner needs to pull/push first).
5. **Build the macOS binary** — run `npm run build:napi`. Inherit stdio so
   build output is visible. Abort on non-zero exit.
6. **Verify build output** — `fs.existsSync('cadmus.darwin-arm64.node')`,
   abort if missing.
7. **Stage and conditionally commit** — `git add cadmus.darwin-arm64.node`,
   then check `git diff --cached --quiet`. If the binary changed, commit with
   message `chore(release): prebuilt darwin-arm64 for upcoming release`. If
   not (rebuild produced an identical binary), log "darwin-arm64 binary
   unchanged, no commit needed" and skip to the push step (which becomes a
   no-op if there's nothing ahead of origin).
8. **Push** — `git push origin main`. Abort on non-zero exit.
9. **Trigger workflow** — `gh workflow run release.yml --field bump=<arg>`.
   Abort on non-zero exit and tell the Product Owner what to retry.
10. **Final message** — print "Release workflow dispatched. Watch progress
    with `gh run watch` or in the Actions UI." and exit 0.

Each shell-out uses `child_process.spawnSync` with `stdio: 'inherit'` for
build/git/gh, and `stdio: ['ignore', 'pipe', 'inherit']` for the small
read-only checks (`git rev-parse`, `git status --porcelain`, `git
diff --cached --quiet`). Trim stdout before comparing.

Make the file executable (`chmod +x scripts/release.mjs`). The Coder runs
`chmod +x` as part of this step — checked-in mode bit ensures cross-developer
usability.

### 2. Rewire `package.json` scripts

In `package.json`, replace:

```json
"release": "gh workflow run release.yml --field bump=patch",
"release:minor": "gh workflow run release.yml --field bump=minor",
"release:major": "gh workflow run release.yml --field bump=major"
```

with:

```json
"release": "node scripts/release.mjs patch",
"release:minor": "node scripts/release.mjs minor",
"release:major": "node scripts/release.mjs major"
```

No other `package.json` changes. Version is left at `2.0.3` — the workflow
bumps it.

### 3. Remove `darwin-arm64` from the workflow matrix (and fix stale comments)

In `.github/workflows/release.yml`:

**Matrix change.** In the `build` job's `matrix.include`, remove this entry
entirely:

```yaml
- os: macos-latest
  target: darwin-arm64
  artifact: cadmus.darwin-arm64.node
```

leaving only the `ubuntu-latest` (Linux) and `windows-latest` (Windows)
entries.

**Dead step.** Also remove the now-dead **`Install CMake (macOS)`** step
(lines 50–52 of the current file) — Linux and Windows do not use it and the
conditional `if: runner.os == 'macOS'` will never fire again.

**Stale comments.** Update the two header/inline comments that explicitly
mention macOS:

- The file-top header comment (lines 3–6) currently states the workflow
  "builds the prebuilt `.node` for all three platforms" and that the
  `npm run release*` scripts "call `gh workflow run`". After this plan both
  claims are false. Rewrite it to describe the new split: the workflow builds
  Linux + Windows on hosted runners; `darwin-arm64` is built locally by
  `scripts/release.mjs`, which pushes the binary to `main` and then calls
  `gh workflow run release.yml`.
- The `fail-fast: false` matrix comment (lines 32–34) explicitly mentions
  "the macOS/Linux outcome". Rewrite it to refer only to the two CI legs
  ("a Windows failure should not hide the Linux outcome" — or equivalent
  wording that drops the macOS mention).

**Unchanged on purpose.** Everything else in the file is left alone,
specifically:

- the existing `actions/checkout@v6` in the publish job (which picks up the
  freshly pushed `cadmus.darwin-arm64.node`),
- the `Verify all platform binaries present` loop (which still checks for all
  three `.node` files and will hard-fail the release if the macOS one is
  missing for any reason),
- the `git add cadmus.darwin-arm64.node cadmus.linux-x64-gnu.node
  cadmus.win32-x64-msvc.node` line in the commit step (the macOS file will be
  unchanged at this point, so `git add` is a safe no-op for it; the commit
  still includes the freshly downloaded Linux + Windows artifacts plus the
  version bump).

### 4. Update `docs/definition.md`

`docs/definition.md` still reflects the v1 reality and is now multiply stale —
this is pre-existing drift, **not introduced by this plan**, but it directly
contradicts the release flow we're shaping, and Rule 4 forbids leaving such
drift unflagged. Touching it here keeps the target-vision document honest
with both the current code and the new flow.

Required edits:

- **§6 "Out of Scope (v1)"** — remove the three lines that are now factually
  wrong: "Linux x86_64 build itself" (the binary is committed at v2.0.3),
  "Windows x86_64 build" (committed at v2.0.0 per the "Windows x86_64 build"
  backlog card), and "GitHub Actions / CI matrix" (delivered via
  `.github/workflows/release.yml`). The "Linux-arm64 and macOS-x64 builds"
  bullet stays — those genuinely remain out of scope and are still tracked in
  `docs/backlog.kanban.md`.
- **§7 Success Criterion 2** — drop the "(and Linux x64 once the follow-up
  build lands)" parenthetical: Linux x64 has landed. Make the criterion
  state plainly that the npm package installs on macOS arm64, Linux x86_64,
  and Windows x86_64.
- **§7 Success Criterion 3** — rewrite the verification half. It currently
  says "Verification is manual per the Release Runbook … no CI matrix."
  Replace with a description of the new hybrid: the bundled fixture asserts
  locally on each developer machine during `cargo test` / `npm test`, and the
  `Release` workflow (`.github/workflows/release.yml`) runs the build legs
  for Linux x86_64 and Windows x86_64 on hosted runners while the
  `darwin-arm64` leg builds locally on the Product Owner's Mac.

No other §6/§7 lines change. The remaining out-of-scope items (GPU,
streaming, word-level timestamps, etc.) are still genuinely out of scope.

### 5. Update `docs/architecture.md`

The section titled **"Verification is local, not CI"** (around line 226) is
already stale — it asserts "There are no GitHub Actions workflows", which has
been false since the v2.0.x release-workflow rollout. This is pre-existing
drift, **not introduced by this plan**, but since we're touching the release
flow we should fix it in the same change rather than leave it for later.

Rewrite that section to describe the current hybrid model:

- The `Release` workflow (`.github/workflows/release.yml`) is the one-button
  publish path, triggered via `npm run release` / `release:minor` /
  `release:major`.
- The `darwin-arm64` binary is built **locally** on the Product Owner's
  Mac by `scripts/release.mjs` and pushed to `main` before the workflow runs;
  the workflow builds only `linux-x64-gnu` (Ubuntu runner) and
  `win32-x64-msvc` (Windows runner), then commits the version bump together
  with all three binaries.
- The cost rationale (avoiding the `macos-latest` 10× billing multiplier with
  a capable local machine already on hand) is the explicit reason for the
  split.
- The pre-flight guarantees enforced by `scripts/release.mjs` (clean tree, on
  `main`, in sync with `origin/main`) are listed so a fresh reader understands
  why the local commit lands cleanly.

The "manual six-step Release Runbook" reference (which points into
`docs/archive/CONCEPT_v1_buildout.md`) is rewritten or removed — that runbook
is the v1 reality, the new section is the v2 reality. The archived concept
file is **not** edited (it's archived; archived means frozen).

### 6. Update `README.md`

`README.md` documents the release flow as user-facing guidance, not as
archival history. After this plan, three sentences become false:

- The **"Platforms"** section (around line 162) — opens with "Cadmus ships
  prebuilt `.node` binaries for three platforms, all built and published by
  the `Release` GitHub Actions workflow:". Rewrite the sentence so it
  describes the split: Linux x86_64 and Windows x86_64 are built by the
  `Release` workflow on hosted runners; macOS arm64 is built locally by
  `scripts/release.mjs` and pushed to `main` ahead of the workflow run. The
  three-bullet platform list below it (macOS / Linux / Windows with their
  BLAS backends) stays as-is.
- The **release paragraph in the lower section** (around line 268) — opens
  with "Releases run through GitHub Actions ([`.github/workflows/release.yml`]
  …): a manual `workflow_dispatch` builds all three binaries …". Rewrite so
  it reflects the hybrid model: the developer runs `npm run release` (which
  delegates to `scripts/release.mjs`), the script builds the `darwin-arm64`
  binary locally and pushes it, then triggers the workflow which builds the
  remaining two binaries, bumps the version, commits, tags, and publishes
  with provenance. Keep the existing pointer to the workflow file and to the
  three wiring locations (`Cargo.toml` ct2rs features, `package.json.files`
  allowlist, `index.ts` dispatch).
- The same paragraph also says "Each is built by the `Release` workflow on
  its native GitHub-hosted runner; cross-compilation is not used." Adjust to
  "Each is built on its native host — Linux and Windows on GitHub-hosted
  runners, macOS on the developer's local machine — and cross-compilation is
  not used." (or equivalent wording).

The README's two-half human/LLM structure and the "things that will bite
you" invariants block are not touched — these edits stay inside the existing
sections and preserve the existing tone. No new sections are added.

### 7. Update `docs/backlog.kanban.md` (housekeeping)

The card titled **"GitHub Actions / CI matrix migration"** (around line 292)
describes the workflow as already delivered. Append a short note to the
card's body explaining that the `darwin-arm64` leg now builds locally (with a
one-line link to `scripts/release.mjs`) — *do not* move the card; the Product
Owner already controls its status. Tone is "for future readers", not "done by
me".

No new cards are introduced. No `bug.kanban.md` changes.

## Verification

After implementation, the Reviewer (in §3) checks the plan; after acceptance,
the Coder runs the following:

### Local pre-flight sanity (no actual release)

These can be run on any clean checkout without publishing anything. They
prove the new script works end-to-end up to the point where it would mutate
the world.

1. `node scripts/release.mjs patch` on a **dirty** tree → fails fast with a
   clear error mentioning the dirty tree. Working tree unchanged.
2. `node scripts/release.mjs patch` on a **non-`main`** branch (clean tree)
   → fails fast with a clear error mentioning the branch. Working tree
   unchanged.
3. `node scripts/release.mjs patch` on a `main` branch that is **behind /
   ahead of `origin/main`** → fails fast with a clear error mentioning sync.
   Working tree unchanged.
4. `node scripts/release.mjs banana` → fails fast with "unknown bump"
   (or equivalent) and lists valid values.

For (1)–(3), the Coder uses test branches / temporary changes (e.g. `git
checkout -b release-test` for the branch check) and resets back to `main`
cleanly afterwards. None of these reach the `npm run build:napi` step.

### Dry-run-style build verification

5. Manually run `npm run build:napi` once (outside the release script).
   Confirm `cadmus.darwin-arm64.node` exists and `napi build` succeeds. This
   proves the local toolchain is working the same way the script will drive
   it.

### Workflow file structural check (manual inspection only)

6. Open `.github/workflows/release.yml` (the locally edited file) and visually
   confirm:
   - the `build` job's matrix has exactly two `include` entries, targeting
     `linux-x64-gnu` and `win32-x64-msvc`;
   - the `Install CMake (macOS)` step is gone;
   - the file-top header comment and the `fail-fast: false` matrix comment
     no longer mention macOS as a CI leg;
   - the publish job still references all three `.node` files in the
     `Verify all platform binaries present` loop and the commit `git add`.

   This is a **manual structural inspection only**. The plan deliberately
   does not specify a local YAML-parse step: no YAML parser is a declared
   dependency of this repo (the plan's Dependencies section is empty), and
   pulling one in just to lint a single workflow file is out of scope. The
   real YAML acceptance gate is GitHub itself, hit at first dispatch:
   `scripts/release.mjs` calls `gh workflow run release.yml`, which fails
   loudly with a parse error if the pushed file is malformed. Because the
   script pushes before triggering, a bad edit surfaces immediately on the
   first real `npm run release` — no half-state is published. Note also that
   `gh workflow view release.yml` is **not** a substitute: it queries the
   copy already stored on GitHub, not the locally edited working tree.

### Doc cross-check

The greps below are scoped to the live source-of-truth files this plan
edits. They deliberately do **not** recurse into `docs/archive/` (archived
docs are frozen — `docs/archive/POSTPONED_PLAN_crates_io_publish.md` still
references the old "no GitHub Actions workflows" claim by design, as
historical context) and they exclude this plan file itself (which describes
the stale phrasing in explanatory prose and will eventually be archived
with the same frozen-once-archived rule).

7. `grep -n "no GitHub Actions workflows" docs/architecture.md` returns no
   matches.
8. `grep -n "scripts/release.mjs" docs/architecture.md README.md
   docs/backlog.kanban.md` returns at least one match in each of the three
   files — confirming the new flow is named on every live release-flow
   surface.
9. `grep -n "Linux x86_64 build itself" docs/definition.md` returns no
   matches (the stale §6 bullet is gone). Same for `grep -n "Windows
   x86_64 build (.x86_64-pc-windows-msvc.)" docs/definition.md` — the §6
   bullet is gone. The backlog card entries about those builds are not
   touched, so `grep -n "Windows x86_64 build" docs/backlog.kanban.md`
   should still hit.

### Optional end-to-end (Product-Owner gated)

10. The only true end-to-end test is shipping a real release. Because that
    publishes to npm + GitHub, it is an irreversible external side effect and
    requires `<to_owner>` confirmation per the workflow rules. The Coder does
    **not** run this autonomously. The first real `npm run release` after
    merge is the live verification — running it produces v2.0.4 (or the next
    patch) and the Product Owner observes both commits (local macOS, CI
    Linux/Windows + bump) landing on `main`.

Build/lint: there is no separate lint step for the release script or the
workflow file; the verification steps above are the equivalent.
