import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');

// Same path the Rust suite (src/storage.rs::test_cache_dir) populates,
// so a single `tiny` download lives across both test runners.
export function sharedCache() {
  return join(repoRoot, 'target', 'cadmus-test-cache');
}

export async function ensureTinyDownloaded(cadmus) {
  const tiny = cadmus.listAvailableModels().find((m) => m.name === 'tiny');
  if (!tiny) throw new Error('catalog missing tiny entry');
  if (tiny.cached) return;
  await cadmus.downloadModel('tiny');
}
