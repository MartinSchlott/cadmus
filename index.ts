import { createRequire } from 'node:module';
import { platform, arch } from 'node:process';

import type {
  CadmusConfig,
  DownloadModelOptions,
  LoadModelOptions,
  ModelInfo,
  ModelRef,
  TranscribeOptions,
  TranscriptResult,
  Version,
} from './types.js';

export type {
  CadmusConfig,
  CadmusError,
  ComputeType,
  DownloadModelOptions,
  LoadModelOptions,
  ModelFamily,
  ModelInfo,
  ModelRef,
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
} else {
  throw new Error(
    `Cadmus: unsupported platform ${platform}-${arch}. Supported: darwin-arm64, linux-x64.`,
  );
}

export const version = binding.version;

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
