import { defineConfig } from '@playwright/test';

// Smoke tests run the UI in a plain browser against the in-memory mock backend
// (src/lib/mock.ts). Native integration is covered by Rust tests.
export default defineConfig({
  testDir: 'e2e',
  use: { baseURL: 'http://localhost:5173', channel: 'msedge' },
  webServer: { command: 'npm run dev', url: 'http://localhost:5173', reuseExistingServer: true },
});
