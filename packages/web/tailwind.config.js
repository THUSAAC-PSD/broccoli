import { broccoliPreset } from '@broccoli/web-sdk/tailwind-preset';
import path from 'path';
import { createRequire } from 'module';
const require = createRequire(import.meta.url);

const sdkPath = path.dirname(require.resolve('@broccoli/web-sdk/package.json'));

/** @type {import('tailwindcss').Config} */
export default {
  presets: [broccoliPreset],

  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
    path.join(sdkPath, 'src/**/*.{js,ts,jsx,tsx}'),
  ],
};
