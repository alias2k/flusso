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

// Free-form string sinks: any value coerces to them, so they never warn.
const FREE_SINK = new Set(["text", "keyword", "identifier"]);
// String-ish sources that can plausibly feed a constrained string type
// (uuid/enum) — but a number/date/bool cannot.
const STRINGISH = new Set([...STRING, "uuid"]);

/// The source types a given document `target` can accept without a drastic
/// reinterpretation. `null` means "anything" (a free-form string sink) or "we
/// don't second-guess this target" (map/custom/object/unknown).
function compatibleSources(target: string): Set<string> | null {
  if (FREE_SINK.has(target)) return null;
  if (target === "uuid" || target === "enum") return STRINGISH;
  if (NUMBER.has(target)) return NUMBER;
  if (TEMPORAL.has(target)) return TEMPORAL;
  if (target === "boolean") return new Set(["boolean"]);
  if (target === "geo_point" || target === "geo") return new Set(["geo_point", "geo"]);
  if (target === "binary") return new Set(["binary"]);
  return null;
}

/// Is mapping a source column of `sourceType` to the document `chosenType` a
/// *drastic* change — a target whose values the source can't plausibly satisfy
/// (e.g. a timestamp forced to uuid, or a text column to integer)? Coercing to a
/// free-form string (text/keyword/identifier) is always fine, so it never warns.
export function drasticTypeChange(sourceType: string, chosenType: string): boolean {
  const src = sourceType.replace(/<.*/, "").trim();
  const tgt = chosenType.replace(/<.*/, "").trim();
  if (src === tgt) return false;
  const ok = compatibleSources(tgt);
  return ok !== null && !ok.has(src);
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
