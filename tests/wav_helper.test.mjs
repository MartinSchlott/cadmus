import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { Cadmus } from '../index.js';
import { sharedCache, ensureTinyDownloaded } from './_helpers/cache.mjs';
import { padWavWithSilence } from './_helpers/wav.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const fixtureWav = readFileSync(resolve(here, '..', 'fixtures', 'eins-zwei-drei.wav'));

test('padWavWithSilence produces a transcribable WAV', async () => {
  const padded = padWavWithSilence(fixtureWav, 5);
  // A 5 s WAV = ~5 × 44.1k × 2 bytes mono = ~440 kB. Sanity-check.
  assert.ok(padded.length > fixtureWav.length, 'padded WAV is not larger');

  const cadmus = new Cadmus({ modelCache: sharedCache() });
  await ensureTinyDownloaded(cadmus);
  const model = await cadmus.loadModel({ name: 'tiny' });
  try {
    const r = await model.transcribe(padded, { language: 'de' });
    const t = r.text.toLowerCase();
    const one = t.includes('eins') || t.includes('1');
    const two = t.includes('zwei') || t.includes('2');
    const three = t.includes('drei') || t.includes('3');
    assert.ok(one && two && three, `padded transcript missing markers: ${r.text}`);
  } finally {
    model.free();
  }
});
