// Playwright `webServer`: build a throwaway project from the dev fixtures and
// launch the real `flusso design` binary against it. Tests hit the served
// (embedded) SPA; the pipeline test runs `flusso check` on the same project.

import { spawn } from "node:child_process";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../../.."); // e2e → frontend → design → apps → root
const dev = resolve(repoRoot, "dev");
const project = resolve(here, ".project");
const bin = process.env.FLUSSO_BIN || resolve(repoRoot, "target/debug/flusso");
const port = process.env.DESIGN_PORT || "7791";

rmSync(project, { recursive: true, force: true });
mkdirSync(project, { recursive: true });
for (const f of ["flusso.toml", "users.schema.yml", "products.schema.yml", "orders.schema.yml"]) {
  cpSync(resolve(dev, f), resolve(project, f));
}

const child = spawn(
  bin,
  ["design", "--config", resolve(project, "flusso.toml"), "--address", `127.0.0.1:${port}`, "--no-open"],
  { stdio: "inherit" },
);
child.on("exit", (code) => process.exit(code ?? 0));
process.on("SIGTERM", () => child.kill("SIGTERM"));
process.on("SIGINT", () => child.kill("SIGINT"));
