// `GenericValue` is the core's externally-tagged value enum — a string is
// `{"String":"x"}`, an integer `{"BigInt":5}`, null the bare string `"Null"`.
// The schema posts straight to the server as that wire form (field `options`,
// a column `default`, a `constant` are all `GenericValue`), so the inspector
// must speak it. These two helpers bridge a plain JS value (what a user types
// and reads) and that tagged form (what the model holds and posts).

/// A `GenericValue` in its externally-tagged JSON form.
export type Generic = unknown;

/// A JSON number with the text it was typed as, kept by [`parseGeneric`].
class TypedNumber {
  constructor(
    readonly value: number,
    readonly text: string,
  ) {}
}

/// A number as the YAML parser reads it, so a value typed here equals the one
/// its file reads back as: an integer in i64 is a `BigInt`, any other number a
/// `Decimal` (of its typed digits when it is an integer past what a JS number
/// holds exactly). A number JS only prints with an exponent stays a `Double`,
/// which the file writes as that float.
function numberGeneric(value: number, text = String(value)): Generic {
  if (/^-?\d+$/.test(text)) return Number.isSafeInteger(value) ? { BigInt: value } : { Decimal: text };
  if (Number.isSafeInteger(value)) return { BigInt: value };
  const shortest = String(value);
  return /e/i.test(shortest) ? { Double: value } : { Decimal: shortest };
}

/// Plain JS value → tagged `GenericValue`. Numbers map as the YAML parser reads
/// them (see `numberGeneric`); everything else maps by JS type.
export function toGeneric(v: unknown): Generic {
  if (v === null || v === undefined) return "Null";
  if (v instanceof TypedNumber) return numberGeneric(v.value, v.text);
  if (typeof v === "boolean") return { Bool: v };
  if (typeof v === "number") return numberGeneric(v);
  if (typeof v === "string") return { String: v };
  if (Array.isArray(v)) return { Array: v.map(toGeneric) };
  if (typeof v === "object") {
    const m: Record<string, Generic> = {};
    for (const [k, val] of Object.entries(v as Record<string, unknown>)) m[k] = toGeneric(val);
    return { Map: m };
  }
  // Exotic leftovers (bigint/symbol/function) — primitives/objects handled above.
  if (typeof v === "bigint") return { String: v.toString() };
  return { String: JSON.stringify(v) ?? "" };
}

/// Tagged `GenericValue` → plain JS value (the inverse of [`toGeneric`], lossy
/// only on the numeric width tag, which the UI doesn't surface). Passes through
/// a value that's already plain, so it's safe on mixed input.
export function fromGeneric(g: Generic): unknown {
  if (g === "Null" || g === null || g === undefined) return null;
  if (typeof g !== "object") return g;
  const o = g as Record<string, unknown>;
  for (const k of [
    "Bool",
    "String",
    "Uuid",
    "SmallInt",
    "Int",
    "BigInt",
    "Float",
    "Double",
    "Decimal",
    "Date",
    "Time",
    "Timestamp",
    "TimestampTz",
  ]) {
    if (k in o) return o[k];
  }
  if ("Array" in o && Array.isArray(o.Array)) return o.Array.map(fromGeneric);
  if ("Map" in o && o.Map && typeof o.Map === "object") {
    const m: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(o.Map as Record<string, unknown>)) m[k] = fromGeneric(v);
    return m;
  }
  return g;
}

/// Typed JSON text → tagged `GenericValue`, keeping each number's digits where
/// the browser exposes them (`JSON.parse` source text), so an integer past
/// 2^53 isn't rounded. Throws on invalid JSON.
export function parseGeneric(text: string): Generic {
  const reviver = (_key: string, value: unknown, context?: { source?: string }) =>
    typeof value === "number" ? new TypedNumber(value, context?.source ?? String(value)) : value;
  return toGeneric(JSON.parse(text, reviver));
}

/// Tagged `GenericValue` → the JSON text a user reads and edits: a decimal shows
/// as its digits, a number, never as the string it is stored as.
export function genericText(g: Generic): string {
  if (g && typeof g === "object") {
    const o = g as Record<string, unknown>;
    if ("Decimal" in o) return String(o.Decimal);
    if ("Array" in o && Array.isArray(o.Array)) return `[${o.Array.map(genericText).join(",")}]`;
    if ("Map" in o && o.Map && typeof o.Map === "object") {
      const entries = Object.entries(o.Map as Record<string, unknown>);
      return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${genericText(v)}`).join(",")}}`;
    }
  }
  return JSON.stringify(fromGeneric(g)) ?? "";
}
