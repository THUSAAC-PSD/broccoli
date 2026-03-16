import { reactRouter } from '@react-router/dev/vite';
import path from 'path';
import { fileURLToPath } from 'url';
import { defineConfig } from 'vite';

import {
  SDK_SHARED_DEPS,
  SHARED_DEPS,
  sharedDepsPlugin,
} from './plugins/shared-deps';

function resolveSdkDep(dep: string): string {
  try {
    return fileURLToPath(import.meta.resolve(dep));
  } catch {
    const relPath =
      dep === '@broccoli/web-sdk'
        ? 'index.js'
        : `${dep.slice('@broccoli/web-sdk/'.length)}/index.js`;

    return path.resolve(__dirname, `../sdk/dist/${relPath}`);
  }
}

// https://vite.dev/config/
export default defineConfig(() => {
  const sdkAliases = [
    ...SDK_SHARED_DEPS.filter((dep) => dep !== '@broccoli/web-sdk').map(
      (dep) => ({
        find: dep,
        replacement: resolveSdkDep(dep),
      }),
    ),
    {
      find: /^@broccoli\/web-sdk$/,
      replacement: resolveSdkDep('@broccoli/web-sdk'),
    },
  ];

  return {
    plugins: [sharedDepsPlugin(), reactRouter()],
    resolve: {
      dedupe: ['react', 'react-dom'],
      alias: [
        { find: '@', replacement: path.resolve(__dirname, './src') },
        ...sdkAliases,
      ],
    },
    optimizeDeps: {
      include: [
        ...SHARED_DEPS.filter((dep) => !dep.startsWith('@broccoli/web-sdk')),
        'monaco-editor',
        'react-markdown',
        'katex',
        'rehype-katex',
        'remark-gfm',
        'remark-math',
      ],
      exclude: [...SDK_SHARED_DEPS],
    },
  };
});
