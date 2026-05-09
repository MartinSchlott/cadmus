import test from 'node:test';
import assert from 'node:assert/strict';
import { version } from '../index.js';

test('version() returns three string fields', () => {
  const v = version();
  assert.equal(typeof v.cadmus, 'string');
  assert.equal(typeof v.ct2rs, 'string');
  assert.equal(typeof v.ctranslate2, 'string');
  assert.ok(v.cadmus.length > 0);
  assert.ok(v.ct2rs.length > 0);
});
