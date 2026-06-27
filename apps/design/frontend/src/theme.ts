// Shared visual mappings: field-type → colour class, and relation-kind → edge
// colour. Keeping them here means the node, the preview, and the canvas all
// speak the same palette.

const STRING = new Set(["text", "keyword", "identifier", "enum"]);
const NUMBER = new Set([
  "short",
  "integer",
  "long",
  "float",
  "double",
  "decimal",
  "byte",
  "half_float",
  "scaled_float",
]);
const TEMPORAL = new Set(["date", "timestamp"]);

/// A CSS class colouring a type label by family (so a schema is scannable).
/// Handles `map<text>` by looking at the outer kind.
export function typeClass(label: string): string {
  const base = label.replace(/<.*/, "").trim();
  if (STRING.has(base)) return "t-string";
  if (NUMBER.has(base)) return "t-number";
  if (TEMPORAL.has(base)) return "t-temporal";
  if (base === "boolean") return "t-bool";
  if (base === "uuid") return "t-uuid";
  if (base === "geo_point" || base === "geo") return "t-geo";
  return "t-other";
}

/// Edge stroke colour by relation verb (matches the node-kind hues in CSS).
export function edgeColor(label: string): string {
  switch (label) {
    case "object":
      return "#a78bfa";
    case "belongs_to":
    case "has_one":
      return "#38bdf8";
    case "has_many":
    case "many_to_many":
      return "#34d399";
    default:
      return "#8a93a3";
  }
}
