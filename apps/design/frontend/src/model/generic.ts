// `GenericValue` is the core's externally-tagged value enum — a string is
// `{"String":"x"}`, an integer `{"BigInt":5}`, null the bare string `"Null"`.
// The schema posts straight to the server as that wire form (field `options`,
// a column `default`, a `constant` are all `GenericValue`), so the inspector
// must speak it. These two helpers bridge a plain JS value (what a user types
// and reads) and that tagged form (what the model holds and posts).

/// A `GenericValue` in its externally-tagged JSON form.
export type Generic = unknown;

/// Plain JS value → tagged `GenericValue`. Integers widen to `BigInt` (i64),
/// non-integers to `Double`; everything else maps by JS type.
export function toGeneric(v: unknown): Generic {
  if (v === null || v === undefined) return "Null";
  if (typeof v === "boolean") return { Bool: v };
  if (typeof v === "number") return Number.isInteger(v) ? { BigInt: v } : { Double: v };
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
