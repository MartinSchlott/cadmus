import { createRequire } from 'node:module';
import { platform, arch } from 'node:process';

export interface Version {
  cadmus: string;
  ct2rs: string;
  ctranslate2: string;
}

interface NativeBinding {
  version(): Version;
}

const require = createRequire(import.meta.url);

let binding: NativeBinding;
if (platform === 'darwin' && arch === 'arm64') {
  binding = require('./cadmus.darwin-arm64.node') as NativeBinding;
} else if (platform === 'linux' && arch === 'x64') {
  binding = require('./cadmus.linux-x64-gnu.node') as NativeBinding;
} else {
  throw new Error(
    `Cadmus: unsupported platform ${platform}-${arch}. Supported: darwin-arm64, linux-x64.`
  );
}

export const version = binding.version;
