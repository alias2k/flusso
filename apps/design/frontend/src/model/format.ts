// Client-side YAML auto-format for the raw editor. `parseDocument` keeps the
// AST's comments and key order, so `toString` re-emits the same document with
// normalized indentation/spacing — a formatter, not a re-serialization.

import { parseDocument } from "yaml";

export function formatYaml(text: string): { ok: true; text: string } | { ok: false; error: string } {
  const doc = parseDocument(text);
  const err = doc.errors[0];
  if (err) return { ok: false, error: err.message };
  return { ok: true, text: doc.toString({ lineWidth: 100 }) };
}
