import type { Filter } from "../api";
import { useT } from "../i18n";
import { AddButton, Combobox, RemoveButton, Select, Text } from "./widgets";

type FilterKind = "raw" | "null_check" | "value_op";

const VALUE_OPS = ["eq", "neq", "lt", "lte", "gt", "gte", "in", "not_in", "like", "ilike", "between"] as const;

function kindOf(f: Filter): FilterKind {
  if ("raw" in f) return "raw";
  if ("op" in f && (f.op === "is_null" || f.op === "is_not_null")) return "null_check";
  return "value_op";
}

function blank(kind: FilterKind): Filter {
  if (kind === "raw") return { raw: "" };
  if (kind === "null_check") return { column: "", op: "is_null" };
  return { column: "", op: "eq", value: "" };
}

/// Interpret the free-text value box per operator: `in`/`not_in` split on
/// commas into an array, `between` into a two-element range, others stay scalar.
function coerceValue(op: string, text: string): unknown {
  if (op === "in" || op === "not_in") {
    return text
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
  }
  if (op === "between") {
    const parts = text.split(",").map((s) => s.trim());
    return [parts[0] ?? "", parts[1] ?? ""];
  }
  return text;
}

function valueText(f: Filter): string {
  if ("value" in f) {
    const v = (f as { value: unknown }).value;
    if (Array.isArray(v)) return v.join(", ");
    if (v == null) return "";
    if (typeof v === "string") return v;
    if (typeof v === "number" || typeof v === "bigint" || typeof v === "boolean") return String(v);
    return JSON.stringify(v) ?? "";
  }
  return "";
}

export function Filters({
  value,
  onChange,
  columns,
}: {
  value: Filter[];
  onChange: (v: Filter[] | undefined) => void;
  columns?: string[];
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
  const colOpts = (columns ?? []).map((c) => ({ value: c, label: c }));

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
            {kind === "raw" && (
              <Text
                value={"raw" in f ? f.raw : ""}
                onChange={(raw) => set(i, { raw })}
                placeholder="status <> 'archived'"
              />
            )}
            {kind === "null_check" && "op" in f && (
              <div className="flex items-center gap-1.5">
                <Combobox
                  value={"column" in f ? f.column : ""}
                  options={colOpts}
                  allowCustom
                  onChange={(column) => set(i, { ...f, column })}
                  placeholder={t("common.column")}
                  className="min-w-0 flex-1"
                />
                <Select
                  value={f.op as "is_null" | "is_not_null"}
                  onChange={(op) => set(i, { ...f, op })}
                  options={["is_null", "is_not_null"]}
                  className="flex-1"
                />
              </div>
            )}
            {kind === "value_op" && "value" in f && (
              <>
                <Combobox
                  value={"column" in f ? f.column : ""}
                  options={colOpts}
                  allowCustom
                  onChange={(column) => set(i, { ...f, column })}
                  placeholder={t("common.column")}
                />
                <div className="flex items-center gap-1.5">
                  <Select
                    value={f.op as (typeof VALUE_OPS)[number]}
                    onChange={(op) => set(i, { ...f, op, value: coerceValue(op, valueText(f)) })}
                    options={VALUE_OPS}
                    className="w-28 shrink-0"
                  />
                  <Text
                    value={valueText(f)}
                    onChange={(text) => set(i, { ...f, value: coerceValue(f.op, text) })}
                    placeholder={
                      f.op === "between" ? t("filters.loHi") : f.op === "in" ? t("filters.abc") : t("filters.value")
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
