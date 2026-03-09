import { reactRouter } from '@react-router/dev/vite';
import path from 'path';
import { defineConfig } from 'vite';

import { sharedDepsPlugin } from './plugins/shared-deps';

// https://vite.dev/config/
export default defineConfig({
  plugins: [sharedDepsPlugin(), reactRouter()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  optimizeDeps: {
    include: [
      'react',
      'react-dom',
      'react-router',
      '@monaco-editor/react',
      'monaco-editor',
      'react-markdown',
      'katex',
      'rehype-katex',
      'remark-gfm',
      'remark-math',
    ],
  },
});
