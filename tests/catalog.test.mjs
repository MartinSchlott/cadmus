import test from 'node:test';
import assert from 'node:assert/strict';
import { Cadmus, defaultModels } from '../index.js';
import { defaultCadmusConfig } from './_helpers/cache.mjs';

const cadmus = new Cadmus(defaultCadmusConfig());

const expectedNames = ['tiny', 'base', 'small', 'medium', 'large-v3', 'large-v3-turbo'];

test('defaultModels returns the 6 multilingual entries in order', () => {
  const models = defaultModels();
  assert.equal(models.length, 6);
  assert.deepEqual(models.map((m) => m.name), expectedNames);
  for (const m of models) {
    assert.equal(m.family, 'whisper');
    assert.equal(m.multilingual, true);
    assert.equal(m.files.length, 5);
    const filenames = m.files.map((f) => f.filename).sort();
    assert.deepEqual(filenames, ['config.json', 'model.bin', 'preprocessor_config.json', 'tokenizer.json', 'vocabulary.json']);
    for (const f of m.files) {
      assert.ok(f.url.startsWith('https://huggingface.co/'), `unexpected URL: ${f.url}`);
    }
  }
});

test('listAvailableModels returns the configured 6 entries', () => {
  const models = cadmus.listAvailableModels();
  assert.equal(models.length, 6);
  assert.deepEqual(models.map((m) => m.name), expectedNames);
});

test('every entry has populated metadata', () => {
  for (const m of cadmus.listAvailableModels()) {
    assert.ok(m.description.length > 0, `empty description for ${m.name}`);
    assert.ok(m.files.length > 0, `empty files for ${m.name}`);
    assert.ok(m.sizeBytes > 0, `non-positive sizeBytes for ${m.name}`);
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

test('empty models config yields empty list', () => {
  const empty = new Cadmus({ modelCache: defaultCadmusConfig().modelCache, models: [] });
  assert.deepEqual(empty.listAvailableModels(), []);
});
