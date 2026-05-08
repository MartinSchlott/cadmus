# PLAN_skeleton — Repository scaffolding for Cadmus 0.1.0

Plan #1 of CONCEPT_v1_buildout. Establishes the single-crate skeleton with all packaging, build, and platform plumbing in place. No business logic beyond `version()`.

Reference: [CONCEPT_v1_buildout.md](CONCEPT_v1_buildout.md), in particular **D7, D8, D17, D22–D27** (single crate + napi feature flag + root `package.json` + committed `.node` binaries + no CI + `node --test` + packaging allowlists).

---

## Context & Goal

The repository today contains only `docs/`, `CLAUDE.md`/`AGENTS.md`, and an inert `.gitignore`. To execute every later plan in CONCEPT_v1_buildout, we need a working build/test/pack pipeline on both target platforms.

The skeleton:

- Single Cargo crate `cadmus` at the repository root (D22).
- `[lib] crate-type = ["cdylib", "lib"]` so one source tree produces both the rlib (Rust consumers) and the cdylib (napi consumers).
- `napi` feature flag (`napi = ["dep:napi", "dep:napi-derive"]`); default features off (D22).
- `package.json` at the repository root (D23). Layout matches sibling project `endymion`.
- `LICENSE` (MIT, Copyright (c) 2026 Martin Schlott — Human-confirmed). Edition `2024` (D17).
- **ct2rs is a real dependency from day 1** (Variante B), with the per-platform CPU-only feature subset from D8 — so CTranslate2's CMake build is exercised immediately, not deferred.
- Public surface is exactly one function: `version() -> Version` (Rust + napi-feature-gated). Three string fields: `cadmus`, `ct2rs`, `ctranslate2`. Real values where reachable; otherwise empty string + Backlog card (no silent TODOs).
- Hand-written TS layer (`index.ts`) compiled by `tsc` directly to the **repository root** as `index.js` + `index.d.ts` (D23/D27 literal). `napi build --js false` so the napi macros do **not** emit a JS dispatcher — `index.ts` loads the per-platform `.node` itself. (`types.ts` arrives in Plan 5 when more types exist; for Plan 1 the only type lives inline in `index.ts`. This keeps the Plan-1 emission set to exactly two files and matches D27's `files` allowlist verbatim.)
- Stub `README.md` (one line; replaced at Concept Closeout).
- Fixture `fixtures/eins-zwei-drei.mp3` committed (used by later plans; verified present here).
- `.gitignore` extended for `target/`, `node_modules/`, plus the two tsc emissions at root (`/index.js`, `/index.d.ts`). `Cargo.lock` is **committed** (D27 allowlist). `cadmus.*.node` is **committed** (D24).
- Both prebuilt binaries (`cadmus.darwin-arm64.node`, `cadmus.linux-x64-gnu.node`) produced and committed before the plan ends.
- D27 packaging boundaries verifiable end-to-end via `cargo package --list` and `npm pack --dry-run` on both hosts.

Done state: a fresh checkout on either platform can `cargo build --release --features napi`, `npm run build`, and `npm test` and see green output. The skeleton is the foundation every subsequent plan extends.

## Breaking Changes

**None.** The repository contains no code today; this plan only adds.

## Reference Patterns

Sibling project `endymion` (Human-mentioned in concept D23 and during Discussion). If `../endymion/` is reachable, mirror these patterns:

- `Cargo.toml`: `[lib] crate-type = ["cdylib", "lib"]`, `napi` feature flag.
- `package.json` `scripts.build`: roughly `napi build --release --platform --js false && tsc`.
- TS source organisation: `index.ts` at the root; `tsc` outputs `index.js` + `index.d.ts` directly to the root (Architect: "tsc baut nach `dist/` (oder direkt)" — Plan 1 picks "direkt" so `package.json.files` matches D27 verbatim).
- The `.node` binary is loaded via a small platform-detection helper inside `index.ts` (since `--js false` suppresses napi's auto-generated dispatcher).

If `endymion` is not reachable, the patterns above are the binding spec; the Coder implements them directly.

## Dependencies

Approved by Human. **No additions during implementation without escalation (Hard Rule 11).**

Concrete versions, looked up against crates.io / npm registry on **2026-05-08** (plan-write date). The Coder uses these literally; if any has churned by implementation time, the Coder stops and reports rather than silently picking a newer release (Rule 11).

**Rust (`Cargo.toml [dependencies]`):**

| Crate | Version | Role / features |
|---|---|---|
| `ct2rs` | `=0.9.18` | Inference engine binding. `default-features = false`, `features = ["whisper"]` plus the platform-conditional CPU subset from D8 (see below). Exact-pinned because we ship binaries built against this — R1 (concept) requires fixed minor. |
| `napi` | `3.8.6` (caret) | napi-rs runtime. `optional = true`. Feature subset: minimum needed for Node ≥ 22 (`napi8` if exposed by 3.x; otherwise the highest level the major exposes — Coder reads `napi`'s docs and picks the lowest-level feature that compiles). |
| `napi-derive` | `3.5.5` (caret) | napi-rs proc macros. `optional = true`. |

**Rust (`Cargo.toml [build-dependencies]`):**

| Crate | Version | Role |
|---|---|---|
| `napi-build` | `2.3.1` (caret) | Runs napi-rs setup in `build.rs`. Stays on major `2.x` even though `napi`/`napi-derive` are at `3.x` — this is the upstream napi-rs convention; do not "upgrade" it to a non-existent 3.x. Always pulled (cargo cannot feature-gate build-deps cleanly); the actual `napi_build::setup()` call is gated inside `build.rs` via `CARGO_FEATURE_NAPI`. |

**npm (`package.json` `devDependencies`):**

| Package | Version | Role |
|---|---|---|
| `@napi-rs/cli` | `^3.6.2` | `napi build` CLI. Major matches the Rust `napi`/`napi-derive` major (3.x). |
| `typescript` | `^6.0.3` | TS compiler. Latest stable major as of plan-write. |

**npm `dependencies`:** none. Definition.md §3 ("Zero npm runtime deps") holds from day 1.

No other crates, no other npm packages. If a build failure during implementation seems to require an additional dependency, the Coder stops and reports — the plan was incomplete (Rule 11).

### Per-platform ct2rs feature subset (D8)

- `aarch64-apple-darwin`: `accelerate`, `ruy`
- `x86_64-unknown-linux-gnu`: `mkl`, `dnnl`, `openmp-runtime-comp`

Cargo's per-target dependency syntax is the cleanest mechanism. Two patterns are acceptable; the Coder picks whichever ct2rs supports without surprises and documents the choice in a `Cargo.toml` comment:

- **Pattern A** — single `[dependencies] ct2rs = ...` declaration with the union of features (`whisper`, `accelerate`, `ruy`, `mkl`, `dnnl`, `openmp-runtime-comp`); ct2rs's own `cfg(target_os)` gating activates only the relevant set per platform.
- **Pattern B** — `[target.'cfg(target_os = "macos")'.dependencies] ct2rs = ...` and `[target.'cfg(target_os = "linux")'.dependencies] ct2rs = ...` with disjoint feature lists.

If neither pattern compiles cleanly with the chosen ct2rs minor, escalate (Rule 7).

**Excluded explicitly (D7):** `cuda`, `cudnn`, `cuda-dynamic-loading`. No GPU features anywhere in `Cargo.toml`.

## Assumptions & Risks

- **A1.** Apple Silicon Mac is one host (Xcode CLI tools, `cmake`, Node ≥ 22, Rust stable ≥ 1.85). Linux x86_64 is a physically separate host (`build-essential`, `cmake`, Node ≥ 22, Rust stable ≥ 1.85). Both are the developer's machines — no CI runner involved (D25).
- **A2.** `fixtures/eins-zwei-drei.mp3` already exists or the Human can supply it before Phase A step 2. If missing, the Coder stops at step 2 and asks — fixture creation is out of scope for this plan.
- **A3.** `endymion` may or may not be reachable from the working directory; if it is not, the Coder follows the patterns spelled out in **Reference Patterns** verbatim.
- **R1 (concept R1).** ct2rs API surface for surfacing its own and CTranslate2's runtime version is not formally documented. Coder probes at impl time:
  - Preferred: a public `ct2rs::version()` / `ct2rs::ctranslate2_version()` or analogous constants.
  - Fallback A: build-script env-relay of `ct2rs`'s `CARGO_PKG_VERSION` for the `ct2rs` field.
  - Fallback B (CTranslate2 only): if no public surface exists, the field returns `String::new()` and the Coder adds a card to `backlog.kanban.md`: **"Expose CTranslate2 version through ct2rs upstream — track and adopt"**, severity normal. No silent TODO. No fabricated string.
- **R2.** napi-rs version pinning is now concrete (Dependencies). If between plan approval and implementation any of `ct2rs 0.9.18`, `napi 3.8.6`, `napi-derive 3.5.5`, `napi-build 2.3.1`, `@napi-rs/cli 3.6.2`, `typescript 6.0.3` is yanked or otherwise unobtainable, the Coder stops and reports — replacing a pinned dependency is a plan-level decision, not an implementation one (Rule 11).
- **R3.** First ct2rs build on each host pulls and builds CTranslate2 via CMake (5–20 min). Failures here mean a host prerequisite is missing — stop and report rather than improvise (Rule 7).
- **R4.** `backlog.kanban.md` does not exist yet. If R1's CTranslate2 fallback triggers, the Coder creates `docs/backlog.kanban.md` from the markdown-kanban skill template and adds the card there. This is a one-line side effect of the plan; no extra approval needed.

## Steps

The plan contains **one BREAK** between Phase A (macOS) and Phase B (Linux). Justification: after Phase A the macOS `.node` binary is committed and pushed; completing Phase B requires running `napi build` on a physically separate Linux host. No software-side action bridges a host change. Cross-compilation is excluded by D8. This is the canonical irreversible-external-side-effect case for BREAK (CLAUDE.md §2).

### Phase A — macOS host (`aarch64-apple-darwin`)

1. **Verify host prerequisites.**
   - `rustc --version` ≥ stable 1.85 (edition 2024 requirement)
   - `cmake --version` present
   - `node --version` ≥ 22
   - `xcode-select -p` returns a path
   - If `cmake` is missing: stop. Do not `brew install` without Human approval (Rule 11).

2. **Verify the audio fixture is present.** `fixtures/eins-zwei-drei.mp3` must exist. If not, stop and ask the Human to provide it.

3. **Scaffold the Rust crate.**
   - `Cargo.toml` at the root with: `[package]` block (`name = "cadmus"`, `version = "0.1.0"`, `edition = "2024"`, `license = "MIT"`, `description`, `readme = "README.md"`), `[package.include]` exactly matching D27's allowlist, `[lib] crate-type = ["cdylib", "lib"]`, `[features] napi = ["dep:napi", "dep:napi-derive"]`, `[dependencies]` and `[target.<triple>.dependencies]` per the Dependencies section above (with the exact pinned versions), `[build-dependencies] napi-build = "2.3.1"`. Add a comment line documenting the per-platform feature pattern (A or B) chosen and the lookup date `2026-05-08`.
   - `build.rs` at the root — **environment-variable gating is the primary mechanism**, not `cfg(feature = ...)`. Cargo does not propagate package features into the build script as `cfg` flags; it propagates them as `CARGO_FEATURE_<NAME>` env vars. Code:
     ```rust
     fn main() {
         if std::env::var_os("CARGO_FEATURE_NAPI").is_some() {
             napi_build::setup();
         }
     }
     ```
     Because `napi-build` is an unconditional `[build-dependencies]` entry (it must be — Cargo cannot feature-gate build-deps), this `use napi_build;` would be implicit; the `napi_build::setup()` path resolves regardless of feature state, and the env-var check decides whether it actually runs. No `extern crate` gymnastics needed. Coder verifies that `cargo build --release` (no features) and `cargo build --release --features napi` both compile.
   - `src/lib.rs` exposing:
     ```rust
     pub struct Version {
         pub cadmus: String,
         pub ct2rs: String,
         pub ctranslate2: String,
     }

     pub fn version() -> Version { /* ... */ }
     ```
     The three fields are populated per R1: `cadmus` from `env!("CARGO_PKG_VERSION")`; `ct2rs` and `ctranslate2` real if reachable, empty string + Backlog card otherwise.
   - Below that, `#[cfg(feature = "napi")] mod napi_bridge { ... }` with a `#[napi(object)] struct VersionJs` mirroring `Version` and a `#[napi] pub fn version() -> VersionJs` that wraps the pure-Rust `version()`. The bridge does no logic — it only converts types (D3).
   - Append a Rust unit-test module at the bottom of `src/lib.rs` (Reviewer-mandated, addresses the "Rust public API never executes" gap):
     ```rust
     #[cfg(test)]
     mod tests {
         use super::*;

         #[test]
         fn version_returns_three_string_fields() {
             let v = version();
             assert_eq!(v.cadmus, env!("CARGO_PKG_VERSION"));
             assert!(v.cadmus.starts_with("0.1.0"));
             // ct2rs and ctranslate2 may be empty (R1 fallback) but must be present and String-typed.
             let _: String = v.ct2rs;
             let _: String = v.ctranslate2;
         }
     }
     ```
     This test runs under both `cargo test` (no features) and `cargo test --features napi`, since the `napi_bridge` module is feature-gated separately and does not affect the pure-Rust `version()` path.

4. **Scaffold the npm/TS surface.**
   - `package.json` at the root:
     - `"name": "@ai-inquisitor/cadmus"`, `"version": "0.1.0"`, `"license": "MIT"`, `"type": "module"`, `"engines": { "node": ">=22" }`.
     - `"main": "index.js"`, `"types": "index.d.ts"` (root layout per D23/D27).
     - `"files": ["index.js", "index.d.ts", "cadmus.darwin-arm64.node", "cadmus.linux-x64-gnu.node", "LICENSE", "README.md"]` — verbatim D27. Plan 1 has exactly these two tsc emissions because the only type lives inline in `index.ts`. When `types.ts` enters in Plan 5 and adds further `*.js`/`*.d.ts` emissions, the plan that introduces them is responsible for extending this list.
     - `"napi": { "binaryName": "cadmus", "targets": ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] }`.
     - `"scripts"`: `"build:napi": "napi build --release --platform --js false"`, `"build:ts": "tsc"`, `"build": "npm run build:napi && npm run build:ts"`, `"test": "node --test tests/"`.
     - `"devDependencies"`: `@napi-rs/cli` and `typescript` at the pinned versions in the Dependencies section. **No `dependencies`.**
   - `tsconfig.json` at the root: `target: "ES2022"`, `module: "ESNext"`, `moduleResolution: "Bundler"` (or `"NodeNext"` if TS 6.x prefers it — Coder verifies once), `rootDir: "."`, `outDir: "."`, `declaration: true`, `strict: true`, `esModuleInterop: true`, `skipLibCheck: true`, `isolatedModules: true`, `include: ["index.ts"]` (only). The deliberate `outDir: "."` produces `index.js` + `index.d.ts` next to `index.ts` at the repository root.
   - `index.ts` at the root — single source file for Plan 1. Loads the per-platform `.node` via `createRequire(import.meta.url)`, dispatches on `process.platform` + `process.arch` (`darwin-arm64` → `./cadmus.darwin-arm64.node`, `linux-x64` → `./cadmus.linux-x64-gnu.node`, anything else → throw with a clear message), re-exports `version`. The `Version` interface is declared inline here for Plan 1 (no separate `types.ts` yet — Architect's Discussion answer ties `types.ts` to Plan 5 when more types arrive).

   The exact napi-cli flag set (`--js false`, `--dts <true|false>`) is verified by the Coder against `@napi-rs/cli 3.6.2`. Whatever combination produces "no auto-generated `index.js` at the root that would collide with the tsc-emitted one; type info comes from the hand-written `index.ts` via tsc" is acceptable. If napi-cli still emits an `index.d.ts` that conflicts with tsc's, the Coder picks `--dts false` as well. Document the final flags in `package.json`'s `scripts.build:napi`.

5. **Scaffold ancillary files.**
   - `LICENSE` at the root: standard MIT body, `Copyright (c) 2026 Martin Schlott`.
   - `README.md` at the root, one line: `# Cadmus 0.1.0\n\nSee [docs/](docs/) for project documentation. Full README ships at the v0.1.0 Concept Closeout.`
   - Extend `.gitignore`:
     ```
     target/
     node_modules/
     /index.js
     /index.d.ts
     ```
     Anchor the tsc emissions with leading `/` so only the root-level generated files are ignored, not any future nested ones. Do **not** add `*.node` or `cadmus.*.node` (D24). Do **not** add `Cargo.lock` (D27 includes it).

6. **Scaffold the Node test.**
   - `tests/version.test.mjs`:
     ```javascript
     import test from 'node:test';
     import assert from 'node:assert/strict';
     import { version } from '../index.js';

     test('version() returns three string fields', () => {
       const v = version();
       assert.equal(typeof v.cadmus, 'string');
       assert.equal(typeof v.ct2rs, 'string');
       assert.equal(typeof v.ctranslate2, 'string');
       assert.match(v.cadmus, /^0\.1\.0/);
     });
     ```
   - The single Rust unit test from step 3 (`version_returns_three_string_fields` in `src/lib.rs`) is the Rust-side counterpart and is mandatory — it is the only thing that exercises the public Rust API on each host. The concept's "Done when" phrasing "`cargo test` (no tests yet)" is interpreted here as the *baseline* expectation; the Reviewer requested one minimal unit test covering `version()` to close the verification gap, which is a strict improvement on the concept's intent (verify the surface works) without expanding scope.

7. **First Rust build (drives the CTranslate2 CMake build).**
   `cargo build --release --features napi`
   Expect 5–15 min on first run. Failure here means a host prerequisite is missing or the chosen ct2rs feature pattern is wrong — stop and report (Rule 7). On success, also run `cargo build --release` (no features) to confirm the rlib builds cleanly without napi.

8. **Build the macOS `.node` and the TS surface.**
   - `npm install` (installs `@napi-rs/cli` + `typescript` only).
   - `npm run build`.
   - Verify: `cadmus.darwin-arm64.node` exists at the repository root, and `index.js` + `index.d.ts` exist at the repository root (tsc-emitted, gitignored).

9. **Verify packaging boundaries (D27).**
   - `cargo package --list --allow-dirty` → contents are exactly: `Cargo.toml`, `Cargo.toml.orig` (cargo-emitted), `Cargo.lock`, `build.rs`, `src/lib.rs`, `fixtures/eins-zwei-drei.mp3`, `LICENSE`, `README.md`. No `package.json`, no `index.ts`, no `index.js`, no `index.d.ts`, no `tsconfig.json`, no `cadmus.*.node`, no `tests/*.mjs`, no `node_modules/`, no `docs/`.
   - `npm pack --dry-run` → contents are: `index.js`, `index.d.ts`, `cadmus.darwin-arm64.node`, `LICENSE`, `README.md`. `cadmus.linux-x64-gnu.node` does not yet exist on this host; npm may warn or silently skip — both are acceptable mid-plan. The check is repeated and tightened in Phase B.

10. **Run tests.**
    - `npm test` → green (one test passing).
    - `cargo test --features napi` → `1 passed; 0 failed` (the inline `version_returns_three_string_fields` test from step 3).
    - `cargo test` (no features) → `1 passed; 0 failed` (same test, exercised on the rlib path without napi).

11. **Commit and push.** One commit titled `feat(skeleton): single-crate scaffold + macOS prebuilt binary` (or similar). Stage all scaffolding files individually (no `git add .`) plus `cadmus.darwin-arm64.node`. Push the branch.

### `BREAK` — Hand-off to Linux host

**Why this BREAK is justified:** the macOS `.node` binary has been committed and pushed. Phase B requires running `napi build` on a physically separate Linux host. No software-side action substitutes for the host switch. Cross-compilation is excluded by D8. (CLAUDE.md §2: BREAKs are reserved for irreversible external side effects — committing-and-pushing the binary plus needing a different physical machine qualifies.)

**Coder reports at the BREAK:**
- macOS build / pack / test status (all green).
- Branch name and the SHA of the macOS-binary commit.
- Pattern chosen for ct2rs platform features (A or B, per Dependencies section) and exact ct2rs version pinned.
- Whether the CTranslate2 / ct2rs version-string fallback (R1) had to engage; if so, confirmation that the Backlog card was created.
- Anything surprising during build (long compile time noted, harmless napi-cli warnings, etc.) that the Reviewer should know about.

**Wait for Human confirmation before Phase B.**

### Phase B — Linux host (`x86_64-unknown-linux-gnu`)

12. **Pull and verify host prerequisites.**
    - `git pull` the branch.
    - `rustc --version` ≥ stable 1.85, `cmake --version`, `node --version` ≥ 22, `cc --version` (gcc ≥ 11 or recent clang).
    - Distro packages expected: `build-essential`, `cmake`, `pkg-config`. If any missing: stop. Do not `apt install` without Human approval (Rule 11).

13. **Build Rust on Linux.** `cargo build --release --features napi`. This pulls + builds CTranslate2 with `intel-onemkl-prebuild` + `dnnl` + `openmp-runtime-comp`. Expect 10–25 min on first run. Also run `cargo build --release` to confirm the rlib path is clean.

14. **Build the Linux `.node`.** `npm install && npm run build`. Verify `cadmus.linux-x64-gnu.node` exists at the root.

15. **Verify packaging boundaries on Linux.** Repeat step 9 with this tightening:
    - `npm pack --dry-run` now lists **both** `cadmus.darwin-arm64.node` and `cadmus.linux-x64-gnu.node`. Anything else outside D27's allowlist → fix and re-verify.

16. **Run tests on Linux.**
    - `npm test` → green (the same test, now executed against the Linux `.node`).
    - `cargo test --features napi` → `1 passed; 0 failed`.
    - `cargo test` (no features) → `1 passed; 0 failed`.

17. **Commit and push.** Single commit titled `feat(skeleton): Linux x86_64 prebuilt binary`. Stage exactly `cadmus.linux-x64-gnu.node` (plus any minor adjustments forced by Linux-side discoveries — those should be rare and called out at validation if any). Push.

## Verification

After Phase B completes, branch HEAD must satisfy:

- Both `cadmus.darwin-arm64.node` and `cadmus.linux-x64-gnu.node` are committed at the repository root.
- On macOS:
  - `cargo build --release --features napi` ✓
  - `cargo build --release` (no napi) ✓
  - `cargo test` (no features) → `1 passed; 0 failed` ✓
  - `cargo test --features napi` → `1 passed; 0 failed` ✓
  - `npm install && npm run build && npm test` ✓
  - `cargo package --list --allow-dirty` matches D27 Rust allowlist ✓
  - `npm pack --dry-run` matches D27 npm allowlist with both `.node` files present ✓
- On Linux: same seven checks ✓.
- `version()` is exercised on each host both from Rust (via the inline unit test) and from JS (via `node --test`). Three string fields returned. `cadmus` matches `/^0\.1\.0/`. `ct2rs` and `ctranslate2` are either real version strings or empty strings backed by a card in `docs/backlog.kanban.md`.
- No new TODOs in the code without a corresponding card in `docs/backlog.kanban.md`. No new `severity: accepted` cards in `bug.kanban.md`.
- No `severity: accepted` cards added; if R1's CTranslate2 fallback engaged, the card is severity normal in `backlog.kanban.md`.

### Reviewer focus points

- **D8 feature subset**: `Cargo.toml` enables only the per-platform CPU features listed; `cuda`, `cudnn`, `cuda-dynamic-loading` are absent.
- **D22 single crate**: one `Cargo.toml`, one `[lib]` with `crate-type = ["cdylib", "lib"]`, `napi` feature flag wires both `napi` and `napi-derive`.
- **D24 binary commits**: both `.node` files in the working tree and in the latest commits; `.gitignore` does not exclude them.
- **D27 packaging boundaries**: both allowlists hold; neither artifact bleeds the other ecosystem's noise.
- **BREAK justification**: irreversible host-switch — accept or reject.
- **R1 outcome**: whether all three `version()` fields are real, and if not, whether the Backlog card exists with the prescribed wording.
- **Self-containment**: a fresh checkout on either host can complete the verification list without further documentation.
