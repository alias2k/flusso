#!/usr/bin/env node
// Guards the designer's translations: every `t("…")` key the UI uses must exist
// in the English base catalog, and every other locale must define exactly the
// same key set. Run in CI (designer-frontend job) and via `npm run check:i18n`.
//
// This is what keeps "a new feature must ship its translations" honest: add a
// string without a key, or a key without translating it in every locale, and
// this fails.

import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const SRC = join(here, "..", "src");
const LOCALES = join(SRC, "locales");
const BASE = "en";

// Keys are namespaced (`topbar.save`); values almost never match that shape at
// token start, so this reliably extracts the catalog's keys without parsing TS.
const keyRe = /["']([a-z][a-zA-Z]*\.[a-zA-Z_]+)["']\s*:/g;

function catalogKeys(file) {
  const keys = new Set();
  for (const m of readFileSync(file, "utf8").matchAll(keyRe)) keys.add(m[1]);
  return keys;
}

function usedKeys() {
  const used = new Set();
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== "locales") walk(p);
      } else if (/\.tsx?$/.test(entry.name) && entry.name !== "i18n.tsx") {
        const s = readFileSync(p, "utf8");
        // `t("ns.key"` translator calls (word boundary excludes set("…") etc.)…
        for (const m of s.matchAll(/(?<![A-Za-z0-9_])t\(\s*["']([a-z][a-zA-Z]*\.[a-zA-Z_]+)["']/g)) used.add(m[1]);
        // …plus keys referenced indirectly through a lookup table (KIND_HELP).
        for (const m of s.matchAll(/["'](kindHelp\.[a-z_]+)["']/g)) used.add(m[1]);
      }
    }
  };
  walk(SRC);
  return used;
}

const locales = readdirSync(LOCALES)
  .filter((f) => /\.ts$/.test(f))
  .map((f) => f.replace(/\.ts$/, ""));

const base = catalogKeys(join(LOCALES, `${BASE}.ts`));
const used = usedKeys();
const errors = [];
const warnings = [];

for (const key of used) if (!base.has(key)) errors.push(`used in UI but missing from ${BASE}.ts: "${key}"`);
for (const key of base) if (!used.has(key)) warnings.push(`in ${BASE}.ts but unused in the UI: "${key}"`);

for (const loc of locales) {
  if (loc === BASE) continue;
  const keys = catalogKeys(join(LOCALES, `${loc}.ts`));
  for (const key of base) if (!keys.has(key)) errors.push(`missing translation in ${loc}.ts: "${key}"`);
  for (const key of keys) if (!base.has(key)) errors.push(`extra key in ${loc}.ts (not in ${BASE}.ts): "${key}"`);
}

if (warnings.length) {
  console.warn(`⚠ ${warnings.length} unused key(s):`);
  for (const w of warnings) console.warn(`  ${w}`);
}
if (errors.length) {
  console.error(`\n✘ i18n check failed (${errors.length}):`);
  for (const e of errors) console.error(`  ${e}`);
  console.error(`\nAdd the key to every locale in apps/design/frontend/src/locales/.`);
  process.exit(1);
}
console.log(`✓ i18n: ${used.size} keys used, ${base.size} in ${BASE}, ${locales.length} locales in lockstep.`);
