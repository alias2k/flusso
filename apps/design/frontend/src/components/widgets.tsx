import { useEffect, useId, useState, type KeyboardEvent, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { fromGeneric, type Generic, toGeneric } from "../model/generic";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Select as SelectRoot, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { cn } from "@/lib/utils";

let uid = 0;
const nextId = () => `w${uid++}`;

// A div, not a <label>: it wraps Radix controls (Select is a button) where a
// wrapping label's click-to-focus would fight the control's own behaviour.
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="field mb-2 flex flex-col gap-1">
      <span className="field-label text-3xs font-semibold uppercase tracking-[0.05em] text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}

// (the IDENTITY/SOURCE/MAPPING Section bands were replaced by the Source/
// Document Block pair below.)

/// A titled block in the inspector. `src` (warm) states where a value comes
/// from; `doc` (accent) holds what you author. The pair, with a [`Bridge`]
/// between, is the "source ⟷ document" reading the panel is built around.
export function Block({ variant, title, children }: { variant: "src" | "doc"; title: string; children: ReactNode }) {
  const src = variant === "src";
  return (
    <div
      className={cn(
        "blk mt-1 rounded-lg border border-l-2 border-border p-3 first:mt-0",
        src
          ? "src bg-string/10 border-l-string"
          : "doc bg-card border-l-primary",
      )}
    >
      <div className={cn("blk-h mb-2 text-3xs font-bold uppercase tracking-[0.08em]", src ? "text-string" : "text-primary")}>
        {src ? "◧ " : "◨ "}
        {title}
      </div>
      {children}
    </div>
  );
}

/// The rule a source imposes on a choice, shown between a [`Block`] pair —
/// e.g. "NOT NULL → required, locked". A cyan connector, cause above, effect
/// below.
export function Bridge({ children }: { children: ReactNode }) {
  return (
    <div className="bridge my-0.5 flex items-start gap-2 px-2.5 py-1.5 text-2xs leading-snug text-muted-foreground">
      <span className="shrink-0 font-bold text-accent2">↓</span>
      <span>{children}</span>
    </div>
  );
}

/// A collapsible "expert" drawer (advanced mapping knobs, filters): quieter
/// than the source/document blocks — slate, monospace-leaning, closed by
/// default — so secondary tuning never competes with the primary choices.
export function Drawer({ title, count, defaultOpen, children }: { title: string; count?: number; defaultOpen?: boolean; children: ReactNode }) {
  return (
    <details className="expert-drawer group mt-2.5 w-full overflow-hidden rounded-lg border border-border" open={defaultOpen}>
      <summary className="drawer-h flex cursor-pointer list-none items-center gap-2 bg-secondary px-3 py-2 [&::-webkit-details-marker]:hidden">
        <ChevronRight className="size-3 text-slate transition-transform group-open:rotate-90" aria-hidden="true" />
        <span className="text-2xs font-bold uppercase tracking-[0.07em] text-slate">{title}</span>
        {count !== undefined && <span className="count ml-auto font-mono text-2xs text-muted-foreground">{count}</span>}
      </summary>
      <div className="border-t border-border bg-slate/5 p-3">{children}</div>
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
  return (
    <>
      <Input
        type="text"
        className={cn(invalid && "invalid", className)}
        aria-invalid={invalid || undefined}
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
    <Input
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
  const id = useId();
  return (
    <div className="check inline-flex items-center gap-1.5 text-xs">
      <Checkbox id={id} checked={value} onCheckedChange={(c) => onChange(c === true)} />
      <Label htmlFor={id} className="cursor-pointer font-normal">
        {label}
      </Label>
    </div>
  );
}

/// A select. Keeps its plain `{ value, onChange, options }` API while rendering
/// shadcn's Radix select underneath (portalled list). `placeholder` shows when
/// `value` is empty (also makes it an action-menu: a `value=""` + `onChange`
/// picks without storing). `className` sizes the trigger (defaults full-width).
export function Select<T extends string>({
  value,
  onChange,
  options,
  placeholder,
  className,
}: {
  value: T;
  onChange: (v: T) => void;
  options: readonly T[] | { label: string; value: T }[];
  placeholder?: string;
  className?: string;
}) {
  const opts = options.map((o) => (typeof o === "string" ? { label: o, value: o } : o));
  return (
    <SelectRoot value={value || undefined} onValueChange={(v) => onChange(v as T)}>
      <SelectTrigger className={cn("w-full", className)}>
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {opts.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </SelectRoot>
  );
}
