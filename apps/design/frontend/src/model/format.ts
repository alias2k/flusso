// Client-side YAML auto-format for the raw editor. `parseDocument` keeps the
// AST's comments and key order, so `toString` re-emits the same document with
// normalized indentation/spacing — a formatter, not a re-serialization.
//
// The `failsafe` schema reads every scalar as a string, so each keeps the exact
// text it was written as: the default schema would read `9007199254740993`,
// `0.1000000000000000000001` or `007` as JS numbers and print them back as
// other values (`…992`, `0.1`, `7`), changing what the file means.

import { parseDocument } from "yaml";

export function formatYaml(text: string): { ok: true; text: string } | { ok: false; error: string } {
  const doc = parseDocument(text, { schema: "failsafe" });
  const err = doc.errors[0];
  if (err) return { ok: false, error: err.message };
  return { ok: true, text: doc.toString({ lineWidth: 100 }) };
}
