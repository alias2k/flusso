import { defineConfig } from "@playwright/test";

// The e2e suite drives the *real* `flusso design` binary (it serves the embedded
// SPA), so it exercises exactly what ships. `e2e/server.mjs` prepares a throwaway
// project from the dev fixtures and launches the binary; tests hit it, and the
// pipeline test then runs `flusso check` against the files the UI saved.
const PORT = process.env.DESIGN_PORT || "7791";
const BASE = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: "e2e",
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: BASE,
    headless: true,
    viewport: { width: 1680, height: 1000 },
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "node e2e/server.mjs",
    url: BASE,
    timeout: 60_000,
    reuseExistingServer: !process.env.CI,
    env: { DESIGN_PORT: PORT },
  },
});
