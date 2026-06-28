import type { Filter } from "../api";
import { useT } from "../i18n";
import { Select, Text } from "./widgets";

type FilterKind = "raw" | "null_check" | "value_op";

const VALUE_OPS = [
  "eq",
  "neq",
  "lt",
  "lte",
  "gt",
  "gte",
  "in",
  "not_in",
  "like",
  "ilike",
  "between",
] as const;

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
    return text.split(",").map((s) => s.trim()).filter(Boolean);
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
    return v == null ? "" : String(v);
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

  return (
    <div className="filters">
      {value.map((f, i) => {
        const kind = kindOf(f);
        return (
          <div className="filter-row" key={i}>
            <Select<FilterKind>
              value={kind}
              onChange={(k) => set(i, blank(k))}
              options={["raw", "null_check", "value_op"]}
            />
            {kind === "raw" && (
              <Text
                value={"raw" in f ? f.raw : ""}
                onChange={(raw) => set(i, { raw })}
                placeholder="status <> 'archived'"
              />
            )}
            {kind === "null_check" && "op" in f && (
              <>
                <Text
                  value={"column" in f ? f.column : ""}
                  onChange={(column) => set(i, { ...f, column })}
                  list={columns}
                  placeholder={t("column")}
                />
                <Select
                  value={f.op as "is_null" | "is_not_null"}
                  onChange={(op) => set(i, { ...f, op })}
                  options={["is_null", "is_not_null"]}
                />
              </>
            )}
            {kind === "value_op" && "value" in f && (
              <>
                <Text
                  value={"column" in f ? (f.column as string) : ""}
                  onChange={(column) => set(i, { ...f, column })}
                  list={columns}
                  placeholder={t("column")}
                />
                <Select
                  value={f.op as (typeof VALUE_OPS)[number]}
                  onChange={(op) => set(i, { ...f, op, value: coerceValue(op, valueText(f)) })}
                  options={VALUE_OPS}
                />
                <Text
                  value={valueText(f)}
                  onChange={(text) => set(i, { ...f, value: coerceValue(f.op as string, text) })}
                  placeholder={f.op === "between" ? t("lo, hi") : f.op === "in" ? t("a, b, c") : t("value")}
                />
              </>
            )}
            <button className="link danger" onClick={() => remove(i)}>
              {t("remove")}
            </button>
          </div>
        );
      })}
      <button className="link" onClick={() => onChange([...value, blank("value_op")])}>
        + {t("filter")}
      </button>
    </div>
  );
}
