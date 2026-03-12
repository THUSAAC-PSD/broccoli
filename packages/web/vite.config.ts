import { reactRouter } from '@react-router/dev/vite';
import path from 'path';
import { defineConfig } from 'vite';

import { SHARED_DEPS, sharedDepsPlugin } from './plugins/shared-deps';

// https://vite.dev/config/
export default defineConfig({
  plugins: [sharedDepsPlugin(), reactRouter()],
  resolve: {
    dedupe: ['react', 'react-dom', '@broccoli/sdk'],
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  optimizeDeps: {
    include: [
      ...SHARED_DEPS,
      '@broccoli/sdk/plugin',
      '@broccoli/sdk/sidebar',
      '@broccoli/sdk/theme',
      'monaco-editor',
      'react-markdown',
      'katex',
      'rehype-katex',
      'remark-gfm',
      'remark-math',
    ],
  },
});
