import { createRequire } from 'node:module';
import { platform, arch } from 'node:process';

import type {
  CadmusConfig,
  DownloadModelOptions,
  LoadModelOptions,
  ModelInfo,
  ModelRef,
  ModelSpec,
  TranscribeOptions,
  TranscriptResult,
  Version,
} from './types.js';

export type {
  CadmusConfig,
  CadmusError,
  ComputeType,
  DownloadModelOptions,
  FileSpec,
  LoadModelOptions,
  ModelFamily,
  ModelInfo,
  ModelRef,
  ModelSpec,
  Segment,
  TranscribeOptions,
  TranscriptResult,
  Version,
} from './types.js';

interface NativeCadmusModel {
  transcribe(audio: Buffer, options?: TranscribeOptions): Promise<TranscriptResult>;
  free(): void;
}

interface NativeCadmus {
  listAvailableModels(): ModelInfo[];
  findModel(name: string): string | null;
  downloadModel(name: string, options?: DownloadModelOptions): Promise<string>;
  loadModel(modelRef: ModelRef, options?: LoadModelOptions): Promise<NativeCadmusModel>;
}

interface NativeBinding {
  version(): Version;
  defaultModels(): ModelSpec[];
  Cadmus: { new (config: CadmusConfig): NativeCadmus };
  transcribe(
    audio: Buffer,
    modelPath: string,
    options?: TranscribeOptions,
  ): Promise<TranscriptResult>;
}

const require = createRequire(import.meta.url);

let binding: NativeBinding;
if (platform === 'darwin' && arch === 'arm64') {
  binding = require('./cadmus.darwin-arm64.node') as NativeBinding;
} else if (platform === 'linux' && arch === 'x64') {
  binding = require('./cadmus.linux-x64-gnu.node') as NativeBinding;
} else if (platform === 'win32' && arch === 'x64') {
  binding = require('./cadmus.win32-x64-msvc.node') as NativeBinding;
} else {
  throw new Error(
    `Cadmus: unsupported platform ${platform}-${arch}. Supported: darwin-arm64, linux-x64, win32-x64.`,
  );
}

export const version = binding.version;
export const defaultModels = binding.defaultModels;

export type CadmusModel = NativeCadmusModel;

export interface Cadmus {
  listAvailableModels(): ModelInfo[];
  findModel(name: string): string | null;
  downloadModel(name: string, options?: DownloadModelOptions): Promise<string>;
  loadModel(modelRef: ModelRef, options?: LoadModelOptions): Promise<CadmusModel>;
}

export const Cadmus = binding.Cadmus as unknown as new (config: CadmusConfig) => Cadmus;

export const transcribe: (
  audio: Buffer,
  modelPath: string,
  options?: TranscribeOptions,
) => Promise<TranscriptResult> = binding.transcribe;
