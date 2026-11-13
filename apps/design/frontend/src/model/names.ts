// Names as flusso reads them. `IndexName` and `SinkName` are trimmed (Rust's
// `str::trim`) and lowercased, then must be lowercase letters, digits and `_`
// not starting with a digit, at most 63 characters; a `FieldName` is only
// trimmed. The page applies a name the way flusso will read it, so it never
// shows (or keys a save's identity by) a spelling the file does not have.
//
// ```ts
// identifier(" Orders") === "orders";
// identifierProblem("my users", []) === "pattern";
// identifierProblem("AUDIT", ["audit"]) === "taken";
// trimmed("\u0085name ") === "name";
// ```

// Rust's `str::trim` set (Unicode White_Space), which JS `trim` doesn't match:
// U+0085 is trimmed there and not here, U+FEFF the other way round.
const SPACE = "\\u0009-\\u000D\\u0020\\u0085\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000";
const EDGE_SPACE = new RegExp(`^[${SPACE}]+|[${SPACE}]+$`, "g");

/// `name` trimmed as Rust's `str::trim` trims it: how flusso reads a field name.
export const trimmed = (name: string): string => name.replace(EDGE_SPACE, "");

/// An index or sink name as flusso reads it: trimmed and lowercased.
export const identifier = (name: string): string => trimmed(name).toLowerCase();

/// Why flusso would refuse `name` for an index or a sink (another one in
/// `taken` reads the same, it isn't lowercase letters, digits and `_` not
/// starting with a digit, or it is longer than 63 characters), or `null` when
/// it is free and valid. An empty name is `null`: leaving the box keeps the
/// old name.
export function identifierProblem(name: string, taken: string[]): "taken" | "pattern" | "length" | null {
  const norm = identifier(name);
  if (!norm) return null;
  if (taken.some((n) => identifier(n) === norm)) return "taken";
  if (!/^[a-z_][a-z0-9_]*$/.test(norm)) return "pattern";
  if ([...norm].length > 63) return "length";
  return null;
}
