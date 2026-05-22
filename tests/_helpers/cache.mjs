import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { defaultModels } from '../../index.js';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');

// Same path the Rust suite (src/storage.rs::test_cache_dir) populates,
// so a single `tiny` download lives across both test runners.
export function sharedCache() {
  return join(repoRoot, 'target', 'cadmus-test-cache');
}

export function defaultCadmusConfig() {
  return { modelCache: sharedCache(), models: defaultModels() };
}

export async function ensureTinyDownloaded(cadmus) {
  const tiny = cadmus.listAvailableModels().find((m) => m.name === 'tiny');
  if (!tiny) throw new Error('configured models missing tiny entry');
  if (tiny.cached) return;
  await cadmus.downloadModel('tiny');
}
