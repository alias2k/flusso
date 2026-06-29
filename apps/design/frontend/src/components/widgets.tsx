import { useEffect, useState, type KeyboardEvent, type ReactNode } from "react";
import { fromGeneric, type Generic, toGeneric } from "../model/generic";

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

// (the IDENTITY/SOURCE/MAPPING Section bands were replaced by the Source/
// Document Block pair below.)

/// A titled block in the inspector. `src` (warm) states where a value comes
/// from; `doc` (accent) holds what you author. The pair, with a [`Bridge`]
/// between, is the "source ⟷ document" reading the panel is built around.
export function Block({ variant, title, children }: { variant: "src" | "doc"; title: string; children: ReactNode }) {
  return (
    <div className={`blk ${variant}`}>
      <div className="blk-h">{title}</div>
      {children}
    </div>
  );
}

/// The rule a source imposes on a choice, shown between a [`Block`] pair —
/// e.g. "NOT NULL → required, locked". A cyan connector, cause above, effect
/// below.
export function Bridge({ children }: { children: ReactNode }) {
  return (
    <div className="bridge">
      <span className="arrow">↓</span>
      <span className="rule">{children}</span>
    </div>
  );
}

/// A collapsible "expert" drawer (advanced mapping knobs, filters): quieter
/// than the source/document blocks — slate, monospace-leaning, closed by
/// default — so secondary tuning never competes with the primary choices.
export function Drawer({ title, count, defaultOpen, children }: { title: string; count?: number; defaultOpen?: boolean; children: ReactNode }) {
  return (
    <details className="drawer" open={defaultOpen}>
      <summary className="drawer-h">
        <span className="chev" aria-hidden="true" />
        <span className="dh-name">{title}</span>
        {count !== undefined && <span className="count">{count}</span>}
      </summary>
      <div className="drawer-body">{children}</div>
    </details>
  );
}

/// The one text-input primitive — every text field in the designer goes through
/// it, so styling/behaviour stay consistent (and no raw `<input>` can drift off
/// the theme). `list` adds a datalist; `onKeyDown` covers Enter-to-submit boxes;
/// `className` lets a caller size it (e.g. a compact filter).
export function Text({
  value,
  onChange,
  placeholder,
  list,
  invalid,
  className,
  onKeyDown,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  list?: string[];
  invalid?: boolean;
  className?: string;
  onKeyDown?: (e: KeyboardEvent<HTMLInputElement>) => void;
}) {
  const id = list ? nextId() : undefined;
  const classes = [className, invalid && "invalid"].filter(Boolean).join(" ") || undefined;
  return (
    <>
      <input
        type="text"
        className={classes}
        value={value}
        placeholder={placeholder}
        list={id}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
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

/// A JSON value box for a `GenericValue`-typed field (`options`, a column
/// `default`, a `constant`). Shows the plain decoded value, posts the tagged
/// form. Keeps a local text buffer so half-typed JSON isn't reverted by the
/// controlled value — the model only updates on a parse, and empty maps to
/// `undefined` (a cleared default) unless `emptyTo` overrides it.
export function GenericInput({
  value,
  onChange,
  placeholder,
  invalid,
  emptyTo,
}: {
  value: Generic | undefined;
  onChange: (v: Generic | undefined) => void;
  placeholder?: string;
  invalid?: boolean;
  emptyTo?: Generic;
}) {
  const external = value === undefined ? "" : JSON.stringify(fromGeneric(value));
  const [text, setText] = useState(external);
  // Resync from the model only when the model itself changes (undo, switching
  // fields) — not on every keystroke, so partial JSON survives typing.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => setText(external), [external]);
  const parseable = text.trim() === "" || (() => { try { JSON.parse(text); return true; } catch { return false; } })();
  const handle = (s: string) => {
    setText(s);
    if (s.trim() === "") return onChange(emptyTo);
    try {
      onChange(toGeneric(JSON.parse(s)));
    } catch {
      /* keep typing until valid JSON */
    }
  };
  return <Text value={text} onChange={handle} placeholder={placeholder} invalid={invalid || !parseable} />;
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
