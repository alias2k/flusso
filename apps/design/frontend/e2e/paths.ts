import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
export const repoRoot = resolve(here, "../../../..");
export const projectDir = resolve(here, ".project");
export const flussoBin = process.env.FLUSSO_BIN || resolve(repoRoot, "target/debug/flusso");
