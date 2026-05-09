import test from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as sleep } from 'node:timers/promises';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { Cadmus } from '../index.js';
import { sharedCache, ensureTinyDownloaded } from './_helpers/cache.mjs';
import { padWavWithSilence } from './_helpers/wav.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const fixtureMp3 = readFileSync(resolve(here, '..', 'fixtures', 'eins-zwei-drei.mp3'));
const fixtureWav = readFileSync(resolve(here, '..', 'fixtures', 'eins-zwei-drei.wav'));

function assertEinsZweiDrei(text) {
  const t = text.toLowerCase();
  const one = t.includes('eins') || t.includes('1');
  const two = t.includes('zwei') || t.includes('2');
  const three = t.includes('drei') || t.includes('3');
  assert.ok(one && two && three, `transcript missing markers: ${JSON.stringify(text)}`);
}

async function loadTinyModel() {
  const cadmus = new Cadmus({ modelCache: sharedCache() });
  await ensureTinyDownloaded(cadmus);
  return cadmus.loadModel({ name: 'tiny' });
}

test('free-after-free: idempotent, then transcribe rejects with AlreadyFreed', async () => {
  const model = await loadTinyModel();
  model.free();
  model.free(); // second call must not throw
  await assert.rejects(
    async () => model.transcribe(fixtureMp3, { language: 'de' }),
    (err) => err instanceof Error && err.code === 'AlreadyFreed',
  );
});

test('free-during-inflight: in-flight Promise resolves; subsequent transcribe rejects', async () => {
  const model = await loadTinyModel();
  let releasedAfter = false;
  try {
    const longWav = padWavWithSilence(fixtureWav, 30);
    const inflight = model.transcribe(longWav, { language: 'de' });
    // Hand-off: let compute() begin (decode + first generate steps).
    await sleep(50);
    model.free();
    releasedAfter = true;

    const result = await inflight;
    assert.ok(result.segments.length > 0, 'in-flight result has no segments');
    assertEinsZweiDrei(result.text);

    await assert.rejects(
      async () => model.transcribe(fixtureMp3, { language: 'de' }),
      (err) => err instanceof Error && err.code === 'AlreadyFreed',
    );
  } finally {
    if (!releasedAfter) model.free();
  }
});

test('concurrent transcribe: two parallel calls both succeed', async () => {
  const model = await loadTinyModel();
  try {
    const [a, b] = await Promise.all([
      model.transcribe(fixtureMp3, { language: 'de' }),
      model.transcribe(fixtureMp3, { language: 'de' }),
    ]);
    assert.ok(a.segments.length > 0);
    assert.ok(b.segments.length > 0);
    assertEinsZweiDrei(a.text);
    assertEinsZweiDrei(b.text);
  } finally {
    model.free();
  }
});
