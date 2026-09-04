import { type ReactNode, useState } from "react";
import type { AdapterDescription } from "../api";
import { useT } from "../i18n";
import { LABEL } from "../styles";
import { cn } from "@/lib/utils";
import { Check, Drawer, Field, Num, Select, Text } from "./widgets";

// The designer renders every source/stream/sink form from the adapter's own
// declaration (`#[derive(AdapterConfig)]` → a draft-07 JSON schema plus an
// example and the secret paths). Nothing here names an adapter: a new adapter
// registered in the CLI gets a form the moment it ships.

type Schema = Record<string, unknown>;
type Options = Record<string, unknown>;

/// Follow a `$ref` into the description's `definitions`, unwrap schemars' single
/// `allOf` wrapper (a `$ref` with its own description), and drop a `null`
/// alternative (an `Option<T>`), so callers see the property's real shape.
function resolve(schema: Schema, root: Schema): Schema {
  let current = schema;
  for (let i = 0; i < 8; i += 1) {
    const allOf = current.allOf;
    if (Array.isArray(allOf) && allOf.length === 1) {
      const inner = allOf[0] as Schema;
      current = { ...inner, description: current.description ?? inner.description };
      continue;
    }
    const anyOf = current.anyOf;
    if (Array.isArray(anyOf)) {
      const nonNull = anyOf.filter((a) => (a as Schema).type !== "null");
      if (nonNull.length === 1 && nonNull.length !== anyOf.length) {
        current = { ...(nonNull[0] as Schema), description: current.description, default: current.default };
        continue;
      }
      if (nonNull.length !== anyOf.length) {
        current = { ...current, anyOf: nonNull };
      }
    }
    const ref = current.$ref;
    if (typeof ref === "string" && ref.startsWith("#/definitions/")) {
      const name = ref.slice("#/definitions/".length);
      const definitions = (root.definitions ?? {}) as Record<string, Schema>;
      const target = definitions[name];
      if (!target) return current;
      current = { ...target, description: current.description ?? target.description, refName: name };
      continue;
    }
    return current;
  }
  return current;
}

function isSecret(schema: Schema): boolean {
  return schema["x-flusso-secret"] === true || schema.refName === "Secret";
}

/// Unit-variant enums come as one `const` alternative per variant (or a plain
/// `enum` list); either way, the tokens.
function enumTokens(schema: Schema): string[] | null {
  if (Array.isArray(schema.enum)) return schema.enum.filter((v): v is string => typeof v === "string");
  const alternatives = (schema.oneOf ?? schema.anyOf) as Schema[] | undefined;
  if (!alternatives) return null;
  const tokens = alternatives.map((a) => a.const).filter((c): c is string => typeof c === "string");
  return tokens.length === alternatives.length && tokens.length > 0 ? tokens : null;
}

function jsonType(schema: Schema): string | undefined {
  const t = schema.type;
  if (typeof t === "string") return t;
  if (Array.isArray(t)) return t.find((x) => x !== "null") as string | undefined;
  return undefined;
}

/// Renders every option of one adapter entry. Required options and secrets sit
/// inline; options with a default fold into a drawer that shows how many are set.
export function AdapterForm({
  description,
  value,
  onChange,
}: {
  description: AdapterDescription;
  value: Options;
  onChange: (next: Options) => void;
}) {
  const { t } = useT();
  const root = description.schema as Schema;
  const properties = (root.properties ?? {}) as Record<string, Schema>;
  const required = new Set((root.required ?? []) as string[]);
  const set = (key: string, v: unknown) => {
    const next = { ...value };
    if (v === undefined || v === "") delete next[key];
    else next[key] = v;
    onChange(next);
  };
  const entries = Object.entries(properties).filter(([key]) => key !== "type");
  const primary = entries.filter(
    ([key, schema]) => required.has(key) || description.secrets.includes(key) || !("default" in resolve(schema, root)),
  );
  const tuning = entries.filter(([key]) => !primary.some(([k]) => k === key));
  const setCount = tuning.filter(([key]) => value[key] !== undefined).length;

  return (
    <div className="adapter-form">
      {primary.map(([key, schema]) => (
        <OptionField
          key={key}
          name={key}
          schema={schema}
          root={root}
          required={required.has(key)}
          secret={description.secrets.includes(key)}
          value={value[key]}
          onChange={(v) => set(key, v)}
        />
      ))}
      {tuning.length > 0 && (
        <Drawer title={t("config.options")} count={setCount || undefined}>
          {tuning.map(([key, schema]) => (
            <OptionField
              key={key}
              name={key}
              schema={schema}
              root={root}
              required={false}
              secret={description.secrets.includes(key)}
              value={value[key]}
              onChange={(v) => set(key, v)}
            />
          ))}
        </Drawer>
      )}
    </div>
  );
}

function OptionField({
  name,
  schema,
  root,
  required,
  secret,
  value,
  onChange,
}: {
  name: string;
  schema: Schema;
  root: Schema;
  required: boolean;
  secret: boolean;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const { t } = useT();
  const resolved = resolve(schema, root);
  const description = typeof resolved.description === "string" ? resolved.description : undefined;
  const label = required ? `${name} · ${t("config.required")}` : name;
  return (
    <div title={description}>
      <Field label={label}>
        <OptionInput
          schema={resolved}
          root={root}
          secret={secret || isSecret(resolved)}
          value={value}
          onChange={onChange}
        />
      </Field>
    </div>
  );
}

/// One option's editor, chosen from its (resolved) schema shape.
function OptionInput({
  schema,
  root,
  secret,
  value,
  onChange,
}: {
  schema: Schema;
  root: Schema;
  secret: boolean;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const { t } = useT();
  const alternatives = (schema.anyOf as Schema[] | undefined)?.map((a) => resolve(a, root));
  const defaultValue = schema.default;

  if (secret && !alternatives) {
    return <SecretInput value={value} onChange={onChange} />;
  }

  // `string or { env } or a table of parts`: a secret-shaped alternative beside
  // an object one. A mode picker selects the shape; each shape has its editor.
  if (alternatives && alternatives.length > 1) {
    const objectAlt = alternatives.find((a) => jsonType(a) === "object" && a.properties);
    const secretAlt = alternatives.find(isSecret);
    if (objectAlt && secretAlt) {
      const isObject = !!value && typeof value === "object" && !("env" in (value as Options));
      const isEnv = !!value && typeof value === "object" && "env" in (value as Options);
      const mode = isObject ? "parts" : isEnv ? "env" : "literal";
      const example = exampleFor(objectAlt);
      return (
        <div className="flex flex-col gap-1.5">
          <Select<"literal" | "env" | "parts">
            value={mode}
            options={[
              { value: "literal", label: t("config.literal") },
              { value: "env", label: t("config.env") },
              { value: "parts", label: t("config.parts") },
            ]}
            onChange={(m) => {
              if (m === "literal") onChange("");
              else if (m === "env") onChange({ env: "" });
              else onChange(example);
            }}
            className="w-40"
          />
          {mode === "parts" ? (
            <ObjectInput schema={objectAlt} root={root} value={(value ?? {}) as Options} onChange={onChange} />
          ) : (
            <SecretInput value={value} onChange={onChange} />
          )}
        </div>
      );
    }
  }

  const tokens = enumTokens(schema);
  if (tokens) {
    const current = typeof value === "string" ? value : "";
    const options = [
      ...(typeof defaultValue === "string" && !required(schema)
        ? [{ value: "", label: `${t("config.default")} · ${defaultValue}` }]
        : []),
      ...tokens.map((token) => ({ value: token, label: token })),
    ];
    return <Select value={current} options={options} onChange={(v) => onChange(v || undefined)} className="w-48" />;
  }

  switch (jsonType(schema)) {
    case "boolean": {
      const checked = typeof value === "boolean" ? value : defaultValue === true;
      return <Check value={checked} label={value === undefined ? t("config.default") : ""} onChange={onChange} />;
    }
    case "integer":
    case "number":
      return (
        <Num
          value={typeof value === "number" ? value : undefined}
          onChange={onChange}
          placeholder={typeof defaultValue === "number" ? String(defaultValue) : undefined}
        />
      );
    case "object":
      if (schema.properties) {
        return <ObjectInput schema={schema} root={root} value={(value ?? {}) as Options} onChange={onChange} />;
      }
      return <JsonInput value={value} onChange={onChange} />;
    case "array":
      return <JsonInput value={value} onChange={onChange} />;
    default:
      return (
        <Text
          value={typeof value === "string" ? value : ""}
          onChange={(v) => onChange(v || undefined)}
          placeholder={typeof defaultValue === "string" ? defaultValue : undefined}
        />
      );
  }
}

function required(schema: Schema): boolean {
  return !("default" in schema);
}

/// A nested table of options (a parts table): its properties, inline.
function ObjectInput({
  schema,
  root,
  value,
  onChange,
}: {
  schema: Schema;
  root: Schema;
  value: Options;
  onChange: (v: Options) => void;
}) {
  const properties = (schema.properties ?? {}) as Record<string, Schema>;
  const required = new Set((schema.required ?? []) as string[]);
  const set = (key: string, v: unknown) => {
    const next = { ...value };
    if (v === undefined || v === "") delete next[key];
    else next[key] = v;
    onChange(next);
  };
  return (
    <div className="ml-2 flex flex-wrap gap-x-3 border-l border-border pl-3">
      {Object.entries(properties).map(([key, property]) => (
        <div key={key} className="min-w-40">
          <OptionField
            name={key}
            schema={property}
            root={root}
            required={required.has(key)}
            secret={false}
            value={value[key]}
            onChange={(v) => set(key, v)}
          />
        </div>
      ))}
    </div>
  );
}

/// A literal or an environment reference, as the file writes it: `"value"` or
/// `{ env = "VAR" }`.
function SecretInput({ value, onChange }: { value: unknown; onChange: (v: unknown) => void }) {
  const { t } = useT();
  const isEnv = !!value && typeof value === "object" && "env" in (value as Options);
  const rawEnv = isEnv ? (value as Options).env : undefined;
  const env = typeof rawEnv === "string" ? rawEnv : "";
  const literal = typeof value === "string" ? value : "";
  return (
    <div className="flex items-center gap-1.5">
      <div className="inline-flex shrink-0 rounded-md border border-border bg-background p-0.5">
        {(["literal", "env"] as const).map((mode) => (
          <button
            key={mode}
            type="button"
            aria-pressed={isEnv ? mode === "env" : mode === "literal"}
            onClick={() => onChange(mode === "env" ? { env } : literal)}
            className={cn(
              "cursor-pointer rounded px-2 py-1 text-2xs font-medium transition-colors",
              (isEnv ? mode === "env" : mode === "literal")
                ? "bg-primary/15 text-primary"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {mode === "env" ? t("config.env") : t("config.literal")}
          </button>
        ))}
      </div>
      {isEnv ? (
        <Text value={env} onChange={(v) => onChange({ env: v })} placeholder="VAR_NAME" className="font-mono" />
      ) : (
        <Text value={literal} onChange={(v) => onChange(v || undefined)} />
      )}
    </div>
  );
}

/// A value the form has no dedicated editor for, edited as JSON.
function JsonInput({ value, onChange }: { value: unknown; onChange: (v: unknown) => void }) {
  const [draft, setDraft] = useState(value === undefined ? "" : JSON.stringify(value));
  const [invalid, setInvalid] = useState(false);
  return (
    <Text
      value={draft}
      onChange={(v) => {
        setDraft(v);
        if (!v.trim()) {
          setInvalid(false);
          onChange(undefined);
          return;
        }
        try {
          onChange(JSON.parse(v));
          setInvalid(false);
        } catch {
          setInvalid(true);
        }
      }}
      invalid={invalid}
      className="font-mono"
    />
  );
}

/// The example an adapter declares for a nested table, so switching a mode
/// picker to "parts" starts from something realistic rather than empty.
function exampleFor(schema: Schema): Options {
  const properties = (schema.properties ?? {}) as Record<string, Schema>;
  const out: Options = {};
  for (const [key, property] of Object.entries(properties)) {
    if ("default" in property) out[key] = property.default;
  }
  return out;
}

/// A kind picker for a port: one segmented button per registered adapter of
/// that port, with a caller-supplied icon per kind (a generic one otherwise).
export function KindToggle({
  kinds,
  value,
  onChange,
  icon,
}: {
  kinds: string[];
  value: string;
  onChange: (kind: string) => void;
  icon: (kind: string) => ReactNode;
}) {
  return (
    <div className="inline-flex rounded-md border border-border bg-background p-0.5">
      {kinds.map((kind) => (
        <button
          key={kind}
          type="button"
          onClick={() => onChange(kind)}
          aria-pressed={value === kind}
          className={cn(
            "inline-flex cursor-pointer items-center gap-1.5 rounded px-2 py-1 text-2xs font-medium transition-colors",
            value === kind ? "bg-primary/15 text-primary" : "text-muted-foreground hover:text-foreground",
          )}
        >
          {icon(kind)}
          {kind}
        </button>
      ))}
    </div>
  );
}

/// A small label for the kind when only one adapter exists for a port.
export function KindBadge({ kind }: { kind: string }) {
  return <span className={cn(LABEL, "rounded bg-background px-1.5 py-0.5")}>{kind}</span>;
}
