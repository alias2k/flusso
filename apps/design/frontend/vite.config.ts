import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Build the SPA into the crate's `dist/`, which `rust-embed` embeds into the
// `flusso` binary. During `npm run dev`, proxy the JSON API to a locally-running
// `flusso design` server (default port 7700).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
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
