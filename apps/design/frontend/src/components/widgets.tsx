import { useEffect, useId, useState, type KeyboardEvent, type ReactNode } from "react";
import { CheckIcon, ChevronDownIcon, ChevronRight, Plus, X } from "lucide-react";
import type { ColumnShape } from "../api";
import { fromGeneric, type Generic, toGeneric } from "../model/generic";
import { typeClass } from "../theme";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Command, CommandEmpty, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Select as SelectRoot, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { cn } from "@/lib/utils";

/// A panel title (the top heading of a settings panel). Renders an `<h2>`.
export function PanelTitle({ children, className }: { children: ReactNode; className?: string }) {
  return <h2 className={cn("mb-3 text-lg", className)}>{children}</h2>;
}

/// A muted section sub-heading inside a panel. Renders an `<h3>`; pass
/// `className` to tweak spacing/layout (e.g. `mt-0` when it leads a block, or
/// flex utilities when it carries an inline action).
export function SectionTitle({ children, className }: { children: ReactNode; className?: string }) {
  return <h3 className={cn("mt-4 mb-2 text-sm text-muted-foreground", className)}>{children}</h3>;
}

// A div, not a <label>: it wraps Radix controls (Select is a button) where a
// wrapping label's click-to-focus would fight the control's own behaviour.
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="field mb-2 flex flex-col gap-1">
      <span className="field-label text-3xs font-semibold uppercase tracking-[0.05em] text-muted-foreground">
        {label}
      </span>
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
        src ? "src bg-string/10 border-l-string" : "doc bg-card border-l-primary",
      )}
    >
      <div
        className={cn(
          "blk-h mb-2 text-3xs font-bold uppercase tracking-[0.08em]",
          src ? "text-string" : "text-primary",
        )}
      >
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
/// The shared "+ add a row" affordance — a ghost button with a Plus icon in the
/// brand accent. Used for every add-another action (a filter, an order_by, a
/// sink, an index, an option) so they all read the same.
export function AddButton({ label, disabled, onClick }: { label: string; disabled?: boolean; onClick: () => void }) {
  return (
    <Button variant="ghost" size="sm" className="text-primary" disabled={disabled} onClick={onClick}>
      <Plus />
      {label}
    </Button>
  );
}

/// The paired "remove this row" affordance — a ghost icon button that stays
/// muted until hovered, then tints destructive. For the loud, primary delete of
/// a whole node, use the inspector header action instead.
export function RemoveButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      className="shrink-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
      aria-label={label}
      onClick={onClick}
    >
      <X />
    </Button>
  );
}

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
export function Drawer({
  title,
  count,
  defaultOpen,
  children,
}: {
  title: string;
  count?: number;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  return (
    <details
      className="expert-drawer group mt-2.5 w-full overflow-hidden rounded-lg border border-border"
      open={defaultOpen}
    >
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
  // A *stable* id (not the per-render counter) — otherwise every keystroke
  // re-renders with a new id, orphaning the open datalist so picks do nothing.
  const generatedId = useId();
  const id = list ? generatedId : undefined;
  return (
    <>
      <Input
        type="text"
        className={cn(invalid && "invalid", className)}
        aria-invalid={invalid ? true : undefined}
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
  // Generic is `unknown` (any JSON value, incl. undefined), so no `| undefined`.
  value: Generic;
  onChange: (v: Generic) => void;
  placeholder?: string;
  invalid?: boolean;
  emptyTo?: Generic;
}) {
  const external = value === undefined ? "" : JSON.stringify(fromGeneric(value));
  const [text, setText] = useState(external);
  // Resync from the model only when the model itself changes (undo, switching
  // fields) — not on every keystroke, so partial JSON survives typing.
  useEffect(() => setText(external), [external]);
  const parseable =
    text.trim() === "" ||
    (() => {
      try {
        JSON.parse(text);
        return true;
      } catch {
        return false;
      }
    })();
  const handle = (s: string) => {
    setText(s);
    if (s.trim() === "") return onChange(emptyTo);
    try {
      onChange(toGeneric(JSON.parse(s)));
    } catch {
      /* keep typing until valid JSON */
    }
  };
  return <Text value={text} onChange={handle} placeholder={placeholder} invalid={!!invalid || !parseable} />;
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

export function Check({ value, onChange, label }: { value: boolean; onChange: (v: boolean) => void; label: string }) {
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
interface Opt<T extends string> {
  label: string;
  value: T;
  description?: string;
  className?: string;
}

export function Select<T extends string>({
  value,
  onChange,
  options,
  placeholder,
  className,
}: {
  value: T;
  onChange: (v: T) => void;
  // Plain string options, or rich ones with a per-item `description` (shown
  // under the label) and `className` (e.g. a type-family colour).
  options: readonly T[] | Opt<T>[];
  placeholder?: string;
  className?: string;
}) {
  const opts: Opt<T>[] = options.map((o) => (typeof o === "string" ? { label: o, value: o } : o));
  return (
    <SelectRoot value={value || undefined} onValueChange={(v) => onChange(v as T)}>
      <SelectTrigger className={cn("w-full", className)}>
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {opts.map((o) => (
          <SelectItem key={o.value} value={o.value} description={o.description}>
            <span className={o.className}>{o.label}</span>
          </SelectItem>
        ))}
      </SelectContent>
    </SelectRoot>
  );
}

/// A searchable dropdown (shadcn combobox: Popover + cmdk). Like [`Select`] but
/// type-to-filter, and — with `allowCustom` — you can enter a value the list
/// doesn't have (so it replaces a free-text + datalist field). Options carry the
/// same `description`/`className` as `Select`, shown as a trailing detail and a
/// label colour.
export function Combobox({
  value,
  onChange,
  options,
  placeholder,
  allowCustom = false,
  className,
}: {
  value: string;
  onChange: (v: string) => void;
  options: Opt<string>[];
  placeholder?: string;
  allowCustom?: boolean;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const selected = options.find((o) => o.value === value);
  const pick = (v: string) => {
    onChange(v);
    setOpen(false);
    setQuery("");
  };
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(
            "flex w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-border bg-secondary px-2.5 py-1 text-sm whitespace-nowrap transition-colors outline-none hover:border-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50",
            className,
          )}
        >
          <span className={cn("truncate", selected?.className)}>
            {selected?.label ?? value ?? ""}
            {!selected && !value && <span className="text-muted-foreground">{placeholder}</span>}
          </span>
          <ChevronDownIcon className="size-3.5 shrink-0 opacity-50" />
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-(--radix-popover-trigger-width) p-0">
        <Command>
          <CommandInput value={query} onValueChange={setQuery} placeholder={placeholder} />
          <CommandList>
            <CommandEmpty>{allowCustom ? "" : "No match"}</CommandEmpty>
            {allowCustom && query && !options.some((o) => o.value === query) && (
              <CommandItem value={query} onSelect={() => pick(query)}>
                Use “{query}”
              </CommandItem>
            )}
            {options.map((o) => (
              <CommandItem key={o.value} value={o.value} onSelect={() => pick(o.value)}>
                <span className={cn("font-mono", o.className)}>{o.label}</span>
                {o.description && (
                  <span className="truncate pl-2 font-mono text-2xs text-muted-foreground">{o.description}</span>
                )}
                <CheckIcon
                  className={cn(
                    "ml-auto size-3.5 shrink-0 text-primary",
                    value === o.value ? "opacity-100" : "opacity-0",
                  )}
                />
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

/// The canonical column picker — a searchable [`Combobox`] over the given
/// catalog columns, each coloured by its type family (via `typeClass`) with the
/// SQL type as a trailing detail. `allowCustom`, so an offline / hand-typed name
/// still works. Use this everywhere a column is chosen, so they all look alike.
export function ColumnPicker({
  value,
  columns,
  onChange,
  placeholder,
  className,
}: {
  value: string;
  columns: ColumnShape[];
  onChange: (v: string) => void;
  placeholder?: string;
  className?: string;
}) {
  const options = columns.map((c) => ({
    value: c.name,
    label: c.name,
    description: c.sql_type,
    className: typeClass((c.suggested_type ?? c.sql_type) as string),
  }));
  return (
    <Combobox
      value={value}
      options={options}
      allowCustom
      onChange={onChange}
      placeholder={placeholder}
      className={className}
    />
  );
}
