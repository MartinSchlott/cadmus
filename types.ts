// Public TypeScript surface for @ai-inquisitor/cadmus.
// Mirrors `napi-binding.d.ts` but uses the published names (no `...Js`
// suffix), a discriminated-union `ModelRef`, and the proper resolution
// types that napi-derive emits as `Promise<unknown>`.

export interface Version {
  cadmus: string;
  ct2rs: string;
  ctranslate2: string;
}

export type ModelFamily = 'whisper' | 'distil_whisper';

export interface FileSpec {
  filename: string;
  url: string;
}

export interface ModelSpec {
  name: string;
  description: string;
  sizeBytes: number;
  family: ModelFamily;
  multilingual: boolean;
  files: FileSpec[];
}

export interface ModelInfo {
  name: string;
  description: string;
  sizeBytes: number;
  family: ModelFamily;
  multilingual: boolean;
  cached: boolean;
  files: string[];
}

export interface Segment {
  start: number;
  end: number;
  text: string;
}

export interface TranscriptResult {
  text: string;
  language: string;
  segments: Segment[];
}

export type ComputeType = 'auto' | 'int8' | 'int8_float16' | 'float16' | 'float32';

export interface LoadModelOptions {
  threads?: number;
  computeType?: ComputeType;
}

export interface TranscribeOptions {
  language?: string;
  beamSize?: number;
}

export interface DownloadModelOptions {
  onProgress?: (received: number, total: number) => void;
  signal?: AbortSignal;
}

export type ModelRef = { name: string } | { path: string };

export interface CadmusConfig {
  modelCache: string;
  models: ModelSpec[];
}

// CadmusError is a runtime `Error` whose `code` field carries the variant
// tag (e.g. `"AlreadyFreed"`, `"UnknownModel"`, `"InvalidArgument"`,
// `"Decode"`, …). Surfaced as a TS type rather than a runtime class —
// `instanceof Error` plus an `err.code` check is the canonical narrowing
// pattern.
export interface CadmusError extends Error {
  code: string;
}
