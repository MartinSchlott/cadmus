import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { Cadmus, defaultModels } from '../index.js';

test('downloadModel happy path with progress (fresh temp cache)', async () => {
  const cache = mkdtempSync(join(tmpdir(), 'cadmus-download-'));
  try {
    const cadmus = new Cadmus({ modelCache: cache, models: defaultModels() });
    const tinyExpected = defaultModels().find((m) => m.name === 'tiny');
    let total = null;
    let lastReceived = 0;
    let count = 0;
    const onProgress = (received, totalArg) => {
      count += 1;
      if (total === null) total = totalArg;
      else assert.equal(totalArg, total, 'total changed across callbacks');
      assert.ok(received >= lastReceived, `received went backwards: ${lastReceived} → ${received}`);
      assert.ok(received <= totalArg, `received ${received} > total ${totalArg}`);
      lastReceived = received;
    };
    const dir = await cadmus.downloadModel('tiny', { onProgress });
    assert.equal(typeof dir, 'string');
    assert.ok(existsSync(dir), `download dir missing: ${dir}`);
    assert.ok(count > 0, 'no progress callbacks fired');
    assert.equal(cadmus.findModel('tiny'), dir);
    assert.equal(total, tinyExpected.sizeBytes,
      'progress total must match the spec sizeBytes');
  } finally {
    rmSync(cache, { recursive: true, force: true });
  }
});
