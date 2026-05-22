import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { Cadmus } from '../index.js';

function buildSpec(name, files) {
  const totalBytes = files.reduce((sum, f) => sum + f.size, 0);
  return {
    name,
    description: 'local file:// fixture',
    sizeBytes: totalBytes,
    family: 'whisper',
    multilingual: true,
    files: files.map((f) => ({ filename: f.filename, url: f.url })),
  };
}

test('downloadModel copies a single file:// source byte-for-byte', async () => {
  const stage = mkdtempSync(join(tmpdir(), 'cadmus-file-url-stage-'));
  const cache = mkdtempSync(join(tmpdir(), 'cadmus-file-url-cache-'));
  try {
    const payload = Buffer.from(Array.from({ length: 4096 }, (_, i) => i % 256));
    const src = join(stage, 'fixture.bin');
    writeFileSync(src, payload);

    const spec = buildSpec('local-fixture', [
      { filename: 'fixture.bin', url: pathToFileURL(src).href, size: payload.length },
    ]);

    const cadmus = new Cadmus({ modelCache: cache, models: [spec] });
    const dir = await cadmus.downloadModel('local-fixture');
    const copied = readFileSync(join(dir, 'fixture.bin'));
    assert.equal(Buffer.compare(copied, payload), 0,
      'cached file must match source byte-for-byte');
  } finally {
    rmSync(stage, { recursive: true, force: true });
    rmSync(cache, { recursive: true, force: true });
  }
});

test('downloadModel handles percent-encoded file:// URLs (paths with spaces)', async () => {
  const stage = mkdtempSync(join(tmpdir(), 'cadmus-file-url-encoded-'));
  const cache = mkdtempSync(join(tmpdir(), 'cadmus-file-url-encoded-cache-'));
  try {
    const subdir = join(stage, 'has space');
    mkdirSync(subdir, { recursive: true });
    const payload = Buffer.from('hello world');
    const src = join(subdir, 'file name.bin');
    writeFileSync(src, payload);

    const href = pathToFileURL(src).href;
    assert.ok(href.includes('%20'), `expected percent-encoded URL, got: ${href}`);

    const spec = buildSpec('encoded-fixture', [
      { filename: 'payload.bin', url: href, size: payload.length },
    ]);

    const cadmus = new Cadmus({ modelCache: cache, models: [spec] });
    const dir = await cadmus.downloadModel('encoded-fixture');
    const copied = readFileSync(join(dir, 'payload.bin'));
    assert.equal(Buffer.compare(copied, payload), 0,
      'percent-encoded path must decode to the source file');
  } finally {
    rmSync(stage, { recursive: true, force: true });
    rmSync(cache, { recursive: true, force: true });
  }
});
