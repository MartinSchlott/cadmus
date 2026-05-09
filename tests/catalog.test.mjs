import test from 'node:test';
import assert from 'node:assert/strict';
import { Cadmus } from '../index.js';
import { sharedCache } from './_helpers/cache.mjs';

const cadmus = new Cadmus({ modelCache: sharedCache() });

test('listAvailableModels returns the 17 catalog entries', () => {
  const models = cadmus.listAvailableModels();
  assert.equal(models.length, 17);
});

test('catalog has 12 whisper + 5 distil_whisper entries', () => {
  const models = cadmus.listAvailableModels();
  const whisper = models.filter((m) => m.family === 'whisper').length;
  const distil = models.filter((m) => m.family === 'distil_whisper').length;
  assert.equal(whisper, 12);
  assert.equal(distil, 5);
});

test('every entry has populated metadata', () => {
  for (const m of cadmus.listAvailableModels()) {
    assert.ok(m.description.length > 0, `empty description for ${m.name}`);
    assert.ok(m.repo.length > 0, `empty repo for ${m.name}`);
    assert.ok(m.files.length > 0, `empty files for ${m.name}`);
    assert.ok(m.sizeBytes > 0, `non-positive sizeBytes for ${m.name}`);
  }
});

test('English-only entries are flagged multilingual=false', () => {
  for (const m of cadmus.listAvailableModels()) {
    if (m.name.endsWith('.en')) {
      assert.equal(m.multilingual, false, `${m.name} should be English-only`);
    }
  }
});

test('findModel returns null for unknown name', () => {
  assert.equal(cadmus.findModel('definitely-not-a-model'), null);
});

test('loadModel rejects with code=UnknownModel for unknown name', async () => {
  await assert.rejects(
    async () => cadmus.loadModel({ name: 'definitely-not-a-model' }),
    (err) => err instanceof Error && err.code === 'UnknownModel',
  );
});

test('loadModel rejects with code=InvalidArgument when both fields are set', async () => {
  await assert.rejects(
    async () => cadmus.loadModel({ name: 'tiny', path: '/x' }),
    (err) => err instanceof Error && err.code === 'InvalidArgument',
  );
});

test('loadModel rejects with code=InvalidArgument when neither field is set', async () => {
  await assert.rejects(
    async () => cadmus.loadModel({}),
    (err) => err instanceof Error && err.code === 'InvalidArgument',
  );
});
