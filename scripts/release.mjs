#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import process from 'node:process';

const VALID_BUMPS = ['patch', 'minor', 'major'];

function run(cmd, args) {
  const result = spawnSync(cmd, args, { stdio: 'inherit' });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function capture(cmd, args) {
  const result = spawnSync(cmd, args, { stdio: ['ignore', 'pipe', 'inherit'] });
  return (result.stdout ?? '').toString().trim();
}

// 1. Parse bump argument
const bump = process.argv[2] ?? 'patch';
if (!VALID_BUMPS.includes(bump)) {
  console.error(`Unknown bump "${bump}". Valid values: ${VALID_BUMPS.join(', ')}`);
  process.exit(1);
}

// 2. Pre-flight: branch must be main
const branch = capture('git', ['rev-parse', '--abbrev-ref', 'HEAD']);
if (branch !== 'main') {
  console.error(`Must be on branch "main" to release; currently on "${branch}". Aborting.`);
  process.exit(1);
}

// 3. Pre-flight: working tree clean
const porcelain = capture('git', ['status', '--porcelain']);
if (porcelain !== '') {
  console.error('Working tree is not clean. Commit or stash all changes before releasing.\n' + porcelain);
  process.exit(1);
}

// 4. Pre-flight: in sync with origin/main
run('git', ['fetch', 'origin', 'main']);
const localHead = capture('git', ['rev-parse', 'HEAD']);
const remoteHead = capture('git', ['rev-parse', 'origin/main']);
if (localHead !== remoteHead) {
  console.error(`Local main (${localHead.slice(0, 7)}) differs from origin/main (${remoteHead.slice(0, 7)}). Pull or push to sync first.`);
  process.exit(1);
}

// 5. Build the macOS binary
console.log('Building darwin-arm64 binary...');
run('npm', ['run', 'build:napi']);

// 6. Verify build output
if (!existsSync('cadmus.darwin-arm64.node')) {
  console.error('Build exited 0 but cadmus.darwin-arm64.node is missing. Aborting.');
  process.exit(1);
}

// 7. Stage and conditionally commit
run('git', ['add', 'cadmus.darwin-arm64.node']);
const diff = spawnSync('git', ['diff', '--cached', '--quiet'], { stdio: ['ignore', 'pipe', 'inherit'] });
if (diff.status !== 0) {
  run('git', ['commit', '-m', 'chore(release): prebuilt darwin-arm64 for upcoming release']);
} else {
  console.log('darwin-arm64 binary unchanged, no commit needed.');
}

// 8. Push
run('git', ['push', 'origin', 'main']);

// 9. Trigger workflow
const trigger = spawnSync('gh', ['workflow', 'run', 'release.yml', '--field', `bump=${bump}`], { stdio: 'inherit' });
if (trigger.status !== 0) {
  console.error(`\nWorkflow trigger failed. To retry manually:\n  gh workflow run release.yml --field bump=${bump}`);
  process.exit(trigger.status ?? 1);
}

// 10. Final message
console.log("Release workflow dispatched. Watch progress with `gh run watch` or in the Actions UI.");
