import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { Cadmus, transcribe } from '../index.js';
import { defaultCadmusConfig, ensureTinyDownloaded } from './_helpers/cache.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const fixtureMp3 = readFileSync(resolve(here, '..', 'fixtures', 'eins-zwei-drei.mp3'));

// tiny normalises spoken numbers to digits at will. Same loose check
// as the Rust suite (see src/inference.rs::assert_eins_zwei_drei).
function assertEinsZweiDrei(text) {
  const t = text.toLowerCase();
  const one = t.includes('eins') || t.includes('1');
  const two = t.includes('zwei') || t.includes('2');
  const three = t.includes('drei') || t.includes('3');
  assert.ok(one && two && three, `transcript missing 1/2/3 markers: ${JSON.stringify(text)}`);
}

test('handle path: load tiny → transcribe fixture mp3 → segments + AlreadyFreed', async () => {
  const cadmus = new Cadmus(defaultCadmusConfig());
  await ensureTinyDownloaded(cadmus);
  const model = await cadmus.loadModel({ name: 'tiny' });
  try {
    const result = await model.transcribe(fixtureMp3, { language: 'de' });
    assert.ok(result.segments.length > 0, 'no segments returned');
    assert.equal(result.language, 'de');
    assertEinsZweiDrei(result.text);
  } finally {
    model.free();
  }
  // After free, a fresh transcribe must reject synchronously with AlreadyFreed.
  await assert.rejects(
    async () => model.transcribe(fixtureMp3, { language: 'de' }),
    (err) => err instanceof Error && err.code === 'AlreadyFreed',
  );
});

test('one-shot transcribe(audio, modelPath, opts) against cached tiny', async () => {
  const cadmus = new Cadmus(defaultCadmusConfig());
  await ensureTinyDownloaded(cadmus);
  const dir = cadmus.findModel('tiny');
  assert.ok(dir, 'tiny not cached after ensureTinyDownloaded');
  const result = await transcribe(fixtureMp3, dir, { language: 'de' });
  assert.ok(result.segments.length > 0);
  assertEinsZweiDrei(result.text);
});
