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

/// The field-type colour families, for the sidebar legend. `varKey` is the
/// `--t-*` CSS var suffix (the same hue `typeClass` colours a label with), so a
/// row's swatch matches every type label in that family across the UI.
export const TYPE_FAMILIES: { varKey: string; label: string }[] = [
  { varKey: "string", label: "string" },
  { varKey: "number", label: "number" },
  { varKey: "temporal", label: "date" },
  { varKey: "bool", label: "boolean" },
  { varKey: "uuid", label: "uuid" },
  { varKey: "geo", label: "geo" },
];

/// Edge stroke colour by relation verb (matches the per-kind hues in CSS).
export function edgeColor(label: string): string {
  switch (label) {
    case "object":
      return "#94a3b8"; // slate
    case "belongs_to":
      return "#60a5fa"; // blue
    case "has_one":
      return "#38bdf8"; // sky
    case "has_many":
      return "#2dd4bf"; // teal
    case "many_to_many":
      return "#22c55e"; // green
    default:
      return "#8a93a3";
  }
}
