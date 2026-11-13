// `GenericValue` is the core's externally-tagged value enum — a string is
// `{"String":"x"}`, an integer `{"BigInt":5}`, null the bare string `"Null"`.
// The schema posts straight to the server as that wire form (field `options`,
// a column `default`, a `constant` are all `GenericValue`), so the inspector
// must speak it. These helpers turn that tagged form into what a user reads;
// what a user types goes the other way through the backend (`api.value`), which
// reads it exactly as the saved file will.

/// A `GenericValue` in its externally-tagged JSON form.
export type Generic = unknown;

/// Tagged `GenericValue` → plain JS value, lossy only on the numeric width tag,
/// which the UI doesn't surface. Passes through
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
