/// <reference types="vitest/config" />
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// Tauri expects a fixed dev port and must not clear the terminal.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  // The card preview uses the same bundled fonts as the PDF (repo-level assets/fonts).
  server: { port: 5173, strictPort: true, fs: { allow: ['..'] } },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: { target: 'es2022', sourcemap: true },
  resolve: process.env.VITEST ? { conditions: ['browser'] } : undefined,
  test: {
    environment: 'jsdom',
    pool: 'threads',
    include: ['src/**/*.test.ts'],
  },
});
