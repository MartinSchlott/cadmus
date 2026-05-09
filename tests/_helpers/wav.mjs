// Synthesises a longer PCM WAV by prepending zero-filled silence to an
// existing PCM WAV. The fmt chunk is copied verbatim so sample rate,
// channel count, and bit depth cannot drift; only the RIFF length and
// the data chunk length are rewritten. Used by lifecycle tests that
// need a transcribe call long enough to be cancelable mid-flight.

const RIFF = 0x46464952; // 'RIFF'
const WAVE = 0x45564157; // 'WAVE'
const FMT_ = 0x20746d66; // 'fmt '
const DATA = 0x61746164; // 'data'

function readChunk(buf, offset) {
  const id = buf.readUInt32LE(offset);
  const size = buf.readUInt32LE(offset + 4);
  return { id, size, body: offset + 8 };
}

export function padWavWithSilence(srcWavBytes, totalSeconds) {
  if (srcWavBytes.readUInt32LE(0) !== RIFF) throw new Error('not a RIFF file');
  if (srcWavBytes.readUInt32LE(8) !== WAVE) throw new Error('not a WAVE file');

  let cursor = 12;
  let fmt = null;
  let data = null;
  while (cursor + 8 <= srcWavBytes.length) {
    const c = readChunk(srcWavBytes, cursor);
    if (c.id === FMT_) fmt = c;
    else if (c.id === DATA) data = c;
    // chunks are padded to even length
    cursor = c.body + c.size + (c.size & 1);
  }
  if (!fmt) throw new Error('missing fmt chunk');
  if (!data) throw new Error('missing data chunk');

  const audioFormat = srcWavBytes.readUInt16LE(fmt.body + 0);
  const numChannels = srcWavBytes.readUInt16LE(fmt.body + 2);
  const sampleRate = srcWavBytes.readUInt32LE(fmt.body + 4);
  const bitsPerSample = srcWavBytes.readUInt16LE(fmt.body + 14);
  if (audioFormat !== 1) {
    throw new Error(`expected PCM (audio_format=1), got ${audioFormat}`);
  }

  const bytesPerSample = (bitsPerSample / 8) * numChannels;
  const srcPayloadSeconds = data.size / (sampleRate * bytesPerSample);
  const silenceSeconds = totalSeconds - srcPayloadSeconds;
  if (silenceSeconds <= 0) return Buffer.from(srcWavBytes);

  const silenceBytes = Math.round(silenceSeconds * sampleRate) * bytesPerSample;

  const fmtChunkLen = 8 + fmt.size + (fmt.size & 1);
  const newDataSize = silenceBytes + data.size;
  const newRiffSize = 4 /* 'WAVE' */ + fmtChunkLen + 8 + newDataSize;

  const out = Buffer.alloc(8 + newRiffSize);
  let p = 0;
  out.writeUInt32LE(RIFF, p); p += 4;
  out.writeUInt32LE(newRiffSize, p); p += 4;
  out.writeUInt32LE(WAVE, p); p += 4;

  // Copy fmt chunk header + body verbatim (incl. padding byte if any).
  srcWavBytes.copy(out, p, fmt.body - 8, fmt.body + fmt.size + (fmt.size & 1));
  p += fmtChunkLen;

  // data chunk header.
  out.writeUInt32LE(DATA, p); p += 4;
  out.writeUInt32LE(newDataSize, p); p += 4;

  // Zero-filled silence. Buffer.alloc above already zeroed the region;
  // skip past it.
  p += silenceBytes;

  // Original PCM payload.
  srcWavBytes.copy(out, p, data.body, data.body + data.size);

  return out;
}
