import type { ReactNode } from "react";

let uid = 0;
const nextId = () => `w${uid++}`;

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
    </label>
  );
}

export function Text({
  value,
  onChange,
  placeholder,
  list,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  list?: string[];
}) {
  const id = list ? nextId() : undefined;
  return (
    <>
      <input
        type="text"
        value={value}
        placeholder={placeholder}
        list={id}
        onChange={(e) => onChange(e.target.value)}
      />
      {list && (
        <datalist id={id}>
          {list.map((o) => (
            <option key={o} value={o} />
          ))}
        </datalist>
      )}
    </>
  );
}

export function Num({
  value,
  onChange,
  placeholder,
}: {
  value: number | undefined;
  onChange: (v: number | undefined) => void;
  placeholder?: string;
}) {
  return (
    <input
      type="number"
      value={value ?? ""}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value === "" ? undefined : Number(e.target.value))}
    />
  );
}

export function Check({
  value,
  onChange,
  label,
}: {
  value: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <label className="check">
      <input type="checkbox" checked={value} onChange={(e) => onChange(e.target.checked)} />
      {label}
    </label>
  );
}

export function Select<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: readonly T[] | { label: string; value: T }[];
}) {
  const opts = options.map((o) => (typeof o === "string" ? { label: o, value: o } : o));
  return (
    <select value={value} onChange={(e) => onChange(e.target.value as T)}>
      {opts.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}
