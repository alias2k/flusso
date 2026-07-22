import type { ColumnShape, Filter, FilterOp, FilterValue } from "../api";
import { useT, type Translate } from "../i18n";
import { AddButton, ColumnPicker, RemoveButton, Select, Text } from "./widgets";

type FilterKind = "raw" | "null_check" | "value_op";

// The option builders switch on literal `t("filters.*")` calls (not a dynamic
// lookup) so the i18n key-usage checker can see every key.
function kindOptions(t: Translate) {
  return [
    { value: "raw" as const, label: t("filters.kindRaw"), description: t("filters.kindRawDesc") },
    { value: "null_check" as const, label: t("filters.kindNullCheck"), description: t("filters.kindNullCheckDesc") },
    { value: "value_op" as const, label: t("filters.kindValueOp"), description: t("filters.kindValueOpDesc") },
  ];
}

function valueOpOptions(t: Translate) {
  return [
    { value: "eq" as const, label: "=", description: t("filters.opEq") },
    { value: "neq" as const, label: "!=", description: t("filters.opNeq") },
    { value: "lt" as const, label: "<", description: t("filters.opLt") },
    { value: "lte" as const, label: "<=", description: t("filters.opLte") },
    { value: "gt" as const, label: ">", description: t("filters.opGt") },
    { value: "gte" as const, label: ">=", description: t("filters.opGte") },
    { value: "in" as const, label: "IN", description: t("filters.opIn") },
    { value: "not_in" as const, label: "NOT IN", description: t("filters.opNotIn") },
    { value: "like" as const, label: "LIKE", description: t("filters.opLike") },
    { value: "ilike" as const, label: "ILIKE", description: t("filters.opIlike") },
    { value: "between" as const, label: "BETWEEN", description: t("filters.opBetween") },
  ];
}

function nullOpOptions(t: Translate) {
  return [
    { value: "is_null" as const, label: "IS NULL", description: t("filters.opIsNull") },
    { value: "is_not_null" as const, label: "IS NOT NULL", description: t("filters.opIsNotNull") },
  ];
}

function kindOf(f: Filter): FilterKind {
  if ("raw" in f) return "raw";
  if ("null_check" in f) return "null_check";
  return "value_op";
}

function blank(kind: FilterKind): Filter {
  if (kind === "raw") return { raw: { raw: "" } };
  if (kind === "null_check") return { null_check: { column: "", op: "is_null" } };
  return { value_op: { column: "", op: "eq", value: { single: "" } } };
}

function opArity(op: FilterOp): "single" | "range" | "list" {
  if (op === "in" || op === "not_in") return "list";
  if (op === "between") return "range";
  return "single";
}

// Carries the operands over when the operator's arity changes, so switching
// `=` → `BETWEEN` keeps what was already typed as the first bound.
function reshapeValue(op: FilterOp, prev: FilterValue): FilterValue {
  const flat = "single" in prev ? [prev.single] : "list" in prev ? prev.list : prev.range;
  const arity = opArity(op);
  if (arity === "list") return { list: flat.length ? flat : [""] };
  if (arity === "range") return { range: [flat[0] ?? "", flat[1] ?? ""] };
  return { single: flat[0] ?? "" };
}

function ValueOpEditor({
  filter,
  cols,
  onChange,
}: {
  filter: { column: string; op: FilterOp; value: FilterValue };
  cols: ColumnShape[];
  onChange: (f: { column: string; op: FilterOp; value: FilterValue }) => void;
}) {
  const { t } = useT();
  const { op, value } = filter;
  return (
    <>
      <div className="flex items-center gap-1.5">
        <ColumnPicker
          value={filter.column}
          columns={cols}
          onChange={(column) => onChange({ ...filter, column })}
          placeholder={t("common.column")}
          className="min-w-0 flex-1"
        />
        <Select
          value={op}
          onChange={(nextOp) => onChange({ ...filter, op: nextOp, value: reshapeValue(nextOp, value) })}
          options={valueOpOptions(t)}
          className="w-32 shrink-0"
        />
      </div>
      {"single" in value && (
        <Text
          value={value.single}
          onChange={(s) => onChange({ ...filter, value: { single: s } })}
          placeholder={t("filters.value")}
        />
      )}
      {"range" in value && (
        <div className="flex items-center gap-1.5">
          <Text
            value={value.range[0]}
            onChange={(lo) => onChange({ ...filter, value: { range: [lo, value.range[1]] } })}
            placeholder={t("filters.rangeFrom")}
            className="min-w-0 flex-1"
          />
          <span className="text-2xs text-muted-foreground">{t("filters.and")}</span>
          <Text
            value={value.range[1]}
            onChange={(hi) => onChange({ ...filter, value: { range: [value.range[0], hi] } })}
            placeholder={t("filters.rangeTo")}
            className="min-w-0 flex-1"
          />
        </div>
      )}
      {"list" in value && (
        <div className="flex flex-col gap-1.5">
          {value.list.map((item, j) => (
            <div className="flex items-center gap-1.5" key={j}>
              <Text
                value={item}
                onChange={(s) => onChange({ ...filter, value: { list: value.list.map((x, k) => (k === j ? s : x)) } })}
                placeholder={t("filters.value")}
                className="min-w-0 flex-1"
              />
              {j > 0 ? (
                <RemoveButton
                  label={t("common.remove")}
                  onClick={() => onChange({ ...filter, value: { list: value.list.filter((_, k) => k !== j) } })}
                />
              ) : (
                <div className="size-8 shrink-0" aria-hidden />
              )}
            </div>
          ))}
          <AddButton
            label={t("filters.value")}
            onClick={() => onChange({ ...filter, value: { list: [...value.list, ""] } })}
          />
        </div>
      )}
    </>
  );
}

export function Filters({
  value,
  onChange,
  columns,
}: {
  value: Filter[];
  onChange: (v: Filter[] | undefined) => void;
  columns?: ColumnShape[];
}) {
  const { t } = useT();
  const set = (i: number, f: Filter) => {
    const next = value.slice();
    next[i] = f;
    onChange(next);
  };
  const remove = (i: number) => {
    const next = value.slice();
    next.splice(i, 1);
    onChange(next.length ? next : undefined);
  };
  const cols = columns ?? [];

  return (
    <div className="filters">
      <p className="hint">{t("filters.help")}</p>
      {value.map((f, i) => {
        const kind = kindOf(f);
        return (
          <div className="my-1.5 flex flex-col gap-1.5 rounded-lg border border-border p-2" key={i}>
            <div className="flex items-center gap-1.5">
              <Select<FilterKind>
                value={kind}
                onChange={(k) => set(i, blank(k))}
                options={kindOptions(t)}
                className="flex-1"
              />
              <RemoveButton label={t("common.remove")} onClick={() => remove(i)} />
            </div>
            {"raw" in f && (
              <Text
                value={f.raw.raw}
                onChange={(raw) => set(i, { raw: { raw } })}
                placeholder={t("filters.rawPlaceholder")}
              />
            )}
            {"null_check" in f && (
              <div className="flex items-center gap-1.5">
                <ColumnPicker
                  value={f.null_check.column}
                  columns={cols}
                  onChange={(column) => set(i, { null_check: { ...f.null_check, column } })}
                  placeholder={t("common.column")}
                  className="min-w-0 flex-1"
                />
                <Select
                  value={f.null_check.op}
                  onChange={(op) => set(i, { null_check: { ...f.null_check, op } })}
                  options={nullOpOptions(t)}
                  className="flex-1"
                />
              </div>
            )}
            {"value_op" in f && (
              <ValueOpEditor filter={f.value_op} cols={cols} onChange={(vo) => set(i, { value_op: vo })} />
            )}
          </div>
        );
      })}
      <AddButton label={t("filters.filter")} onClick={() => onChange([...value, blank("value_op")])} />
    </div>
  );
}
