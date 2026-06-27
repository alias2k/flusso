import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Build the SPA into the crate's `dist/`, which `rust-embed` embeds into the
// `flusso` binary. During `npm run dev`, proxy the JSON API to a locally-running
// `flusso design` server (default port 7700).
export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:7700",
    },
  },
});
