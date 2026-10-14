/// <reference types="vitest/config" />
import { defineConfig, loadEnv } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { svelteTesting } from '@testing-library/svelte/vite'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const backendUrl = env.VITE_BACKEND_URL || 'http://127.0.0.1:3000';

  return {
    plugins: [svelte(), svelteTesting()],
    css: {
      modules: {
        localsConvention: 'camelCaseOnly',
        generateScopedName: '[name]__[local]___[hash:base64:5]',
      },
    },
    test: {
      include: ['src/**/*.test.ts'],
      environment: 'jsdom',
      globals: true,
      // The component tests mount the full Svelte dashboard surface and
      // synchronously walk the export payload of every tab. On a
      // memory-constrained host vitest's 5s default expires while the worker
      // is still transforming modules, producing a wall-clock timeout rather
      // than a real assertion failure — and the same tests pass when the file
      // is run alone. 30s keeps the suite deterministic without masking a
      // genuine hang, which would still fail on this ceiling.
      testTimeout: 30_000,
    },
    server: {
      proxy: {
        '/api': {
          target: backendUrl,
          changeOrigin: true,
        },
        '/ws': {
          target: backendUrl.replace('http', 'ws'),
          changeOrigin: true,
          ws: true,
        },
      },
    },
  };
})
