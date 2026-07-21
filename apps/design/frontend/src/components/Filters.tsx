import type { ColumnShape, Filter, FilterOp, FilterValue } from "../api";
import { useT } from "../i18n";
import { AddButton, ColumnPicker, RemoveButton, Select, Text } from "./widgets";

type FilterKind = "raw" | "null_check" | "value_op";

const VALUE_OPS: FilterOp[] = ["eq", "neq", "lt", "lte", "gt", "gte", "in", "not_in", "like", "ilike", "between"];

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

/// Interpret the free-text value box per operator into a tagged `FilterValue`:
/// `in`/`not_in` split on commas → `list`, `between` → a two-element `range`,
/// everything else → a scalar `single`.
function coerceValue(op: FilterOp, text: string): FilterValue {
  if (op === "in" || op === "not_in") {
    return {
      list: text
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
    };
  }
  if (op === "between") {
    const parts = text.split(",").map((s) => s.trim());
    return { range: [parts[0] ?? "", parts[1] ?? ""] };
  }
  return { single: text };
}

/// The value box's text for a `FilterValue` — the inverse of [`coerceValue`].
function valueText(v: FilterValue): string {
  if ("single" in v) return v.single;
  if ("list" in v) return v.list.join(", ");
  return v.range.join(", ");
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
                options={["raw", "null_check", "value_op"]}
                className="flex-1"
              />
              <RemoveButton label={t("common.remove")} onClick={() => remove(i)} />
            </div>
            {"raw" in f && (
              <Text value={f.raw.raw} onChange={(raw) => set(i, { raw: { raw } })} placeholder="status <> 'archived'" />
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
                  options={["is_null", "is_not_null"]}
                  className="flex-1"
                />
              </div>
            )}
            {"value_op" in f && (
              <>
                <ColumnPicker
                  value={f.value_op.column}
                  columns={cols}
                  onChange={(column) => set(i, { value_op: { ...f.value_op, column } })}
                  placeholder={t("common.column")}
                />
                <div className="flex items-center gap-1.5">
                  <Select
                    value={f.value_op.op}
                    onChange={(op) =>
                      set(i, { value_op: { ...f.value_op, op, value: coerceValue(op, valueText(f.value_op.value)) } })
                    }
                    options={VALUE_OPS}
                    className="w-28 shrink-0"
                  />
                  <Text
                    value={valueText(f.value_op.value)}
                    onChange={(text) =>
                      set(i, { value_op: { ...f.value_op, value: coerceValue(f.value_op.op, text) } })
                    }
                    placeholder={
                      f.value_op.op === "between"
                        ? t("filters.loHi")
                        : f.value_op.op === "in"
                          ? t("filters.abc")
                          : t("filters.value")
                    }
                    className="flex-1"
                  />
                </div>
              </>
            )}
          </div>
        );
      })}
      <AddButton label={t("filters.filter")} onClick={() => onChange([...value, blank("value_op")])} />
    </div>
  );
}
