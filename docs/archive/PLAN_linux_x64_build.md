# PLAN: Linux x86_64 Build & Verification

**Backlog card:** `ggjoacipz9pvwfmwn1evjsp1` (Linux x86_64 follow-up build)

## Context & Goal

Cadmus 1.0.0 still ships a macOS arm64 binary only. The Linux x86_64 path is
still wired in the current codebase (`Cargo.toml` per-target ct2rs feature
set, `package.json` `files` + `napi.targets`, `index.ts` platform dispatch),
and `cadmus.linux-x64-gnu.node` is still absent from the repository.

The tracked Linux build host already completed prerequisite setup
(`cmake 3.28.3`, `pkg-config 1.8.1`, Rust 1.95.0, Node 22, gcc 13) and
contains partial `target/` artifacts from a cancelled
`cargo build --release --features napi`. This plan resumes that host if it is
available; otherwise it reruns the same build from scratch on another Linux
x86_64 machine, then executes the current release verification suite and
commits the resulting binary.

Execution machine: Linux x86_64 (`x86_64-unknown-linux-gnu`). Reusing the
original host is preferred because the partially-built DNNL/CMake artifacts can
be reused from `target/`.

## Breaking Changes

No. Adding a prebuilt binary for a second platform is purely additive.

## Reference Patterns

- `README.md` — current build pipeline, test layout, packaging invariants
- `Cargo.toml` — per-target Linux ct2rs features and crate packaging allowlist
- `package.json` — `napi.targets`, npm `files` allowlist, build/test scripts
- `index.ts` — current Linux x64 runtime dispatch
- `tests/public_api.rs` — 7 Rust integration tests gated out under
  `--features napi`

## Dependencies

No repository dependency changes.

Approved build-host prerequisites:

| Package / Tool | Source | Purpose |
|---|---|---|
| `cmake` | apt | Required by ct2rs/CTranslate2 CMake build |
| `pkg-config` | apt | Required by Cargo build scripts |
| Rust stable toolchain | rustup.rs | Provides `rustc`, `cargo`, `rustup` |
| Node.js `>= 22` | existing host provisioning | Required for `npm ci`, `npm run build`, `npm test` |
| `cc` toolchain (gcc 13 or compatible) | existing host provisioning | Required by Cargo / ct2rs native build |

If the original host is reused, all prerequisites above are already present and
Step 1 is verification-only. On a fresh host, install any missing prerequisite
before Step 2.

## Assumptions & Risks

- Resuming on the original host is fastest because Cargo/CMake can reuse
  partial `target/` outputs from the cancelled Linux build.
- A cold Linux build can still take 10–25 minutes on an 8+ core machine and
  much longer on a 1–2 core server because DNNL/CTranslate2 is compiled from
  source.
- `cargo test --features napi` remains part of release verification even though
  `tests/public_api.rs` resolves to zero tests in that mode; the unit-test
  surface in `src/` still differs from the non-`napi` run.

## Steps

### Step 1 — Verify host prerequisites and resume state

```bash
uname -m          # must be x86_64
source "$HOME/.cargo/env" 2>/dev/null || true
rustc --version   # tracked host: 1.95.0
cargo --version
cmake --version
pkg-config --version
node --version    # must be >= 22
cc --version      # gcc 13 or compatible
```

All seven must resolve without error.

If `cmake` or `pkg-config` is missing on a fresh Ubuntu host:

```bash
sudo apt update
sudo apt install -y cmake pkg-config
```

If Rust is missing on a fresh host:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

If `node --version` is below 22 or no working `cc` toolchain is present, stop
and reprovision the host before continuing.

If resuming on the original host, keep the existing `target/` directory so the
partial DNNL/CMake build is reused.

### Step 2 — Rust build (resume if possible, otherwise cold)

First build triggers the CTranslate2 + Intel oneMKL + DNNL CMake compilation.
On the original host, Cargo/CMake should resume from the partial artifacts in
`target/`. On a fresh host this is a cold build.

```bash
source "$HOME/.cargo/env"
cargo build --release --features napi
cargo build --release
```

Both must exit 0.

### Step 3 — npm build (produces the Linux .node binary and JS/TS surface)

```bash
npm ci
npm run build
```

Verify the generated release artifacts exist at the repository root:

```bash
ls -lh cadmus.linux-x64-gnu.node index.js index.d.ts types.js types.d.ts
```

### Step 4 — Cargo test suite (rlib path, no `napi` feature)

Current code layout: `cargo test` runs the Rust unit tests in `src/` plus the
7 public API integration tests in `tests/public_api.rs`.

```bash
cargo test
```

All 7 tests in `tests/public_api.rs` must pass:
- `cadmus_new_creates_cache_dir`
- `cadmus_new_io_error_when_cache_path_blocked`
- `list_available_models_has_seventeen_entries`
- `unknown_model_returns_unknown_model_error`
- `tiny_round_trip_via_cadmus_handle`
- `one_shot_transcribe_via_path`
- `language_none_yields_empty_language_until_ct2rs_exposes_detection`

Note: `tiny_round_trip_via_cadmus_handle` and `one_shot_transcribe_via_path`
download the tiny Whisper model on first run (~75 MB) and cache it. Subsequent
runs are idempotent.

### Step 5 — Cargo test suite (`napi` feature)

Current code layout: `cargo test --features napi` runs the `src/` unit tests
(28 at time of writing: decode 8, inference 12, storage 5, napi 2, lib 1).
`tests/public_api.rs` is deliberately gated out in this mode via
`#![cfg(not(feature = "napi"))]` because the `napi`-flavoured rlib references
N-API symbols that only resolve inside Node.

```bash
cargo test --features napi
```

All unit tests that compile in this mode must pass.

### Step 6 — Packaging boundary check

Both packaging boundaries must be verified (D27). The `--allow-dirty` flag is
required because the active `docs/PLAN_*.md` keeps the worktree dirty until
Archive (Step 7 of the workflow), which comes after this verification step.

```bash
cargo package --list --allow-dirty
```

Output must contain only Rust source, `Cargo.toml`/`Cargo.lock`, `build.rs`,
`tests/**/*.rs`, `fixtures/**`, and licence/README files. Must **not** contain
`.node` files, `package.json`, `index.ts`, or `tests/**/*.mjs`.

```bash
npm pack --dry-run
```

Output must list the current npm allowlist artifacts and must **not** contain
`Cargo.toml`, `src/`, `tests/`, or `fixtures/`:

```
index.js
index.d.ts
types.js
types.d.ts
cadmus.darwin-arm64.node
cadmus.linux-x64-gnu.node
LICENSE
LICENSE-THIRD-PARTY
README.md
```

### Step 7 — npm test suite against the Linux binary

```bash
npm test
```

Runs `node --test --test-concurrency=1 tests/*.test.mjs`. All 16 current test
cases across these files must pass:
- `version.test.mjs`
- `catalog.test.mjs`
- `transcribe.test.mjs` (downloads tiny model if not cached; asserts the
  transcript contains loose 1/2/3 markers — each of "eins"/"1", "zwei"/"2",
  "drei"/"3" must appear in any form)
- `lifecycle.test.mjs` (free-after-free, free-during-inflight, concurrent
  transcribe)
- `download.test.mjs`
- `wav_helper.test.mjs`

### Step 8 — Commit the Linux binary

```bash
git add cadmus.linux-x64-gnu.node
git commit -m "feat(build): Linux x86_64 prebuilt binary

Adds cadmus.linux-x64-gnu.node built on x86_64-unknown-linux-gnu.
All cargo and npm verifications green on Linux.

Closes backlog card ggjoacipz9pvwfmwn1evjsp1."
```

Stage only `cadmus.linux-x64-gnu.node`. Do not stage `node_modules`, `target/`,
generated `index.{js,d.ts}` / `types.{js,d.ts}`, or anything else.

## Verification

Done when all of the following are true:

- [x] `cadmus.linux-x64-gnu.node` present in the working tree and committed
- [x] `npm run build` emits `cadmus.linux-x64-gnu.node` plus `index.{js,d.ts}` and `types.{js,d.ts}`
- [x] `cargo test` — 26 unit tests + 7 `tests/public_api.rs` tests pass
- [x] `cargo test --features napi` — 28 `src/` unit tests pass; `tests/public_api.rs` resolves to zero tests by design
- [x] `cargo package --list --allow-dirty` — no `.node`, no `package.json`, no `.mjs` files
- [x] `npm pack --dry-run` — lists `index.{js,d.ts}`, `types.{js,d.ts}`, both `.node` files, `LICENSE`, `LICENSE-THIRD-PARTY`, and `README.md`, with no `src/`, `tests/`, `fixtures/`, or `Cargo.toml`
- [x] `npm test` — 15/16 pass; see Closure Notes

## Closure Notes (2026-05-11)

Execution diverged from the recorded plan and is documented here so
the historical trail stays honest.

**Original session abandoned.** A prior `claude --print` session
ran the build to completion (binary at `cadmus.linux-x64-gnu.node`,
2026-05-11 05:57) but then made out-of-scope changes against
`src/napi.rs` while attempting to fix `npm test` failures, and was
killed before it could commit or archive. The session left the
working tree dirty across `src/napi.rs`, `src/inference.rs`,
`src/storage.rs`, and `docs/backlog.kanban.md`. The Rule-7/Rule-8
violations were rolled back during closeout.

**Kept from the abandoned session.** The `#[cfg(test)]`-only
`test_cache_lock()` in `src/storage.rs` and the corresponding
`ensure_tiny()` recovery logic in `src/inference.rs`. These fix a
genuine test-only race on the shared `target/cadmus-test-cache/tiny`
directory; the fix touches no production code. Filed retroactively
as a Done card in `docs/bug.kanban.md` for traceability.

**Reverted from the abandoned session.** The `AtomicUsize inflight`
patch in `src/napi.rs` (deferred `free()` until in-flight transcribes
drain). Reverted because (a) it is a production semantics change
clearly outside the build plan's scope (Rule 8) and (b) the patch
itself contains a race between the `freed`-check in `transcribe()`
and `inflight.fetch_add(1)`. The patch is preserved at
`docs/archive/PLAN_linux_x64_build.napi_rogue_attempt.diff` for the
future plan that addresses this properly.

**Verification result.** `npm test` 15/16. The one failure
(`tests/lifecycle.test.mjs:40` `free-during-inflight`) is a
pre-existing v1 bug surfaced — not a Linux regression. The JS-path
decode happens before the `freed` check in
`InferenceHandle::transcribe_with_options`, so `free()` landing
mid-decode causes the in-flight Promise to reject with
`AlreadyFreed`. On macOS the race window is hidden by faster cores;
on the Linux build host (1–2 cores) it opens reliably. Filed as a
high-severity Open card in `docs/bug.kanban.md`. A separate plan
will fix it; the Linux binary itself is shipped because the bug is
present on both platforms with identical behaviour.
