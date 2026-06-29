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

/// Text-colour class for a field/relation kind, for colour-coding the kind
/// pickers. Relations use their "Kinds"-legend hue; the aggregates all share
/// the number hue (they yield numbers); the structured kinds take a distinct
/// type hue each so every option in the menu is colour-coded.
const KIND_HUE: Record<string, string> = {
  // relations (match the Kinds legend)
  belongs_to: "k-belongs_to",
  has_one: "k-has_one",
  has_many: "k-has_many",
  many_to_many: "k-many_to_many",
  object: "k-object",
  // structured leaf kinds
  geo: "t-geo",
  map: "t-temporal",
  custom: "t-uuid",
  constant: "t-string",
  // aggregates → the number hue (they reduce to a number / array of numbers)
  count: "t-number",
  sum: "t-number",
  avg: "t-number",
  min: "t-number",
  max: "t-number",
  ids: "t-number",
};
export function kindColorClass(kind: string): string {
  return KIND_HUE[kind] ?? "";
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
