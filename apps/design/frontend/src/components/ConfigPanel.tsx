import { type ReactNode, useState } from "react";
import { Copy, Plus, Search, Terminal } from "lucide-react";
import type { ConfigToml, IndexEntry } from "../api";
import { useT } from "../i18n";
import { LABEL } from "../styles";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Check, Combobox, Drawer, Field, Num, PanelTitle, RemoveButton, Select, Text } from "./widgets";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

type Sink = Record<string, unknown>;
type Source = Record<string, unknown>;

// Shared column track for the index table header + rows: name · schema file ·
// on_error · state · duplicate · remove. Every non-flex column is a fixed width
// (no `auto`) so the leftover space — and thus the two `fr` columns — resolves
// identically in the header grid and each row grid, keeping the labels aligned.
const INDEX_COLS = "grid grid-cols-[minmax(6rem,1fr)_minmax(9rem,1.6fr)_8.5rem_6rem_2rem_2rem] items-center gap-x-2";

export function ConfigPanel({
  config,
  onChange,
  onDuplicate,
}: {
  config: ConfigToml;
  onChange: (c: ConfigToml) => void;
  onDuplicate: (i: number) => void;
}) {
  const { t } = useT();
  const index = config.index ?? [];
  const sinks = (config.sinks ?? {}) as Record<string, Sink>;
  const [pendingRemove, setPendingRemove] = useState<number | null>(null);

  const setEntry = (i: number, e: IndexEntry) => {
    const next = index.slice();
    next[i] = e;
    onChange({ ...config, index: next });
  };
  const setSink = (name: string, sink: Sink) => onChange({ ...config, sinks: { ...sinks, [name]: sink } });
  const removeSink = (name: string) => {
    const next = { ...sinks };
    delete next[name];
    onChange({ ...config, sinks: next });
  };
  // Rename a sink (a config-map key) in place, preserving declaration order. A
  // no-op on empty / unchanged / colliding names, so the editor can commit
  // freely on blur without clobbering another sink.
  const renameSink = (from: string, to: string) => {
    const name = to.trim();
    if (!name || name === from || name in sinks) return;
    const next: Record<string, Sink> = {};
    for (const [k, v] of Object.entries(sinks)) next[k === from ? name : k] = v;
    onChange({ ...config, sinks: next });
  };

  return (
    <div className="config-panel max-w-3xl">
      <PanelTitle>{t("sidebar.deployment")}</PanelTitle>

      <div className="mb-5 flex flex-wrap items-end gap-x-4 gap-y-1 rounded-lg border border-border bg-secondary px-3 py-2">
        <div className="w-40">
          <Field label={t("config.indexPrefix")}>
            <Text
              value={config.prefix ?? ""}
              onChange={(prefix) => onChange({ ...config, prefix })}
              placeholder={t("config.none")}
            />
          </Field>
        </div>
        <div className="w-28">
          <Field label="on_error">
            <Select
              value={((config.on_error as string) ?? "stop") as "stop" | "skip"}
              options={["stop", "skip"]}
              onChange={(v) => onChange({ ...config, on_error: v })}
            />
          </Field>
        </div>
        <div className="w-44">
          <Field label="public_address">
            <Text
              value={(config.server?.public_address as string) ?? ""}
              onChange={(v) => onChange({ ...config, server: { ...config.server, public_address: v || undefined } })}
              placeholder="127.0.0.1:9464"
            />
          </Field>
        </div>
        <div className="w-44">
          <Field label="private_address">
            <Text
              value={(config.server?.private_address as string) ?? ""}
              onChange={(v) => onChange({ ...config, server: { ...config.server, private_address: v || undefined } })}
              placeholder="127.0.0.1:9465"
            />
          </Field>
        </div>
      </div>

      <Stage step={1} tone="bg-kind-root" title={t("config.source")} hint={t("config.stageSourceHint")} lead>
        <div className="rounded-lg border border-l-2 border-border border-l-kind-root bg-secondary p-3">
          <ConnectionEditor source={config.source} onChange={(source) => onChange({ ...config, source })} />
          <Check
            value={config.source?.manage_publication !== false}
            label="manage_publication"
            onChange={(v) => onChange({ ...config, source: { ...config.source, manage_publication: v } })}
          />
        </div>
      </Stage>

      <Stage step={2} tone="bg-accent2" title={t("sidebar.indexes")} hint={t("config.stageIndexesHint")}>
        <div className="overflow-hidden rounded-lg border border-l-2 border-border border-l-accent2">
          <div className={cn(INDEX_COLS, "bg-secondary px-3 py-1.5")}>
            <span className={LABEL}>{t("config.name")}</span>
            <span className={LABEL}>{t("config.schemaFile")}</span>
            <span className={LABEL}>{t("config.onError")}</span>
            <span className={LABEL}>{t("config.state")}</span>
            <span />
            <span />
          </div>
          {index.map((e, i) => {
            const suggestion = e.name ? `${e.name}.schema.yml` : "";
            const schemaOpts = Array.from(new Set([suggestion, ...index.map((x) => x.schema)].filter(Boolean))).map(
              (p) => ({ value: p, label: p }),
            );
            return (
              <div key={i} className={cn(INDEX_COLS, "px-3 py-1.5")}>
                <Text value={e.name} onChange={(name) => setEntry(i, { ...e, name })} placeholder={t("config.name")} />
                <Combobox
                  value={e.schema}
                  onChange={(schema) => setEntry(i, { ...e, schema })}
                  options={schemaOpts}
                  allowCustom
                  placeholder={suggestion || "x.schema.yml"}
                />
                <Select
                  value={(e.on_error as string) ?? "default"}
                  options={[
                    // "default" = inherit the deployment-wide policy — say which
                    // one, so the row reads without cross-checking the runbar.
                    { value: "default", label: `default · ${(config.on_error as string) ?? "stop"}` },
                    { value: "stop", label: "stop" },
                    { value: "skip", label: "skip" },
                  ]}
                  onChange={(v) => setEntry(i, { ...e, on_error: v === "default" ? undefined : v })}
                />
                <button
                  type="button"
                  aria-pressed={e.enabled}
                  onClick={() => setEntry(i, { ...e, enabled: !e.enabled })}
                  className={cn(
                    "inline-flex cursor-pointer items-center gap-1.5 justify-self-start rounded-md border px-2 py-1 text-2xs font-medium transition-colors",
                    e.enabled
                      ? "border-primary/40 bg-primary/10 text-primary hover:bg-primary/15"
                      : "border-border text-muted-foreground hover:text-foreground",
                  )}
                >
                  <span className={cn("size-1.5 rounded-full", e.enabled ? "bg-primary" : "bg-muted-foreground")} />
                  {e.enabled ? t("config.enabled") : t("config.disabled")}
                </button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="shrink-0 text-muted-foreground hover:text-foreground"
                  title={t("config.duplicate")}
                  aria-label={t("config.duplicate")}
                  onClick={() => onDuplicate(i)}
                >
                  <Copy />
                </Button>
                <RemoveButton label={t("common.remove")} onClick={() => setPendingRemove(i)} />
              </div>
            );
          })}
        </div>
        <AddDashed
          label={t("config.index")}
          onClick={() =>
            onChange({
              ...config,
              index: [...index, { name: "new_index", schema: "new_index.schema.yml", enabled: true }],
            })
          }
        />
      </Stage>

      <Stage step={3} tone="bg-primary" title={t("config.sinks")} hint={t("config.stageSinksHint")}>
        {Object.entries(sinks).map(([name, sink]) => (
          <SinkEditor
            key={name}
            name={name}
            sink={sink}
            taken={Object.keys(sinks).filter((n) => n !== name)}
            onChange={(s) => setSink(name, s)}
            onRename={(to) => renameSink(name, to)}
            onRemove={() => removeSink(name)}
          />
        ))}
        <AddDashed
          label={t("config.sink")}
          onClick={() =>
            setSink(`sink${Object.keys(sinks).length + 1}`, { type: "opensearch", url: "http://127.0.0.1:9200" })
          }
        />
      </Stage>

      <Dialog open={pendingRemove !== null} onOpenChange={(o) => !o && setPendingRemove(null)}>
        <DialogContent showCloseButton={false} className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("common.remove")}</DialogTitle>
            <DialogDescription>
              {pendingRemove !== null ? t("config.removeIndex", { name: index[pendingRemove]?.name ?? "" }) : ""}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={() => setPendingRemove(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => {
                onChange({ ...config, index: index.filter((_, j) => j !== pendingRemove) });
                setPendingRemove(null);
              }}
            >
              {t("common.remove")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/// The prominent full-width dashed "add a row" button under a stage's list.
function AddDashed({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="mt-1.5 flex w-full cursor-pointer items-center justify-center gap-1.5 rounded-lg border border-dashed border-border py-2 text-sm font-medium text-primary transition-colors hover:border-primary/50 hover:bg-primary/5"
    >
      <Plus className="size-4" />
      {label}
    </button>
  );
}

/// A numbered stage in the deployment pipeline (Source → Indexes → Sinks). The
/// `tone` is a background utility for the step badge (a node-kind hue), tying
/// the stage to the flow's colour language.
function Stage({
  step,
  tone,
  title,
  hint,
  lead,
  children,
}: {
  step: number;
  tone: string;
  title: string;
  hint: string;
  lead?: boolean;
  children: ReactNode;
}) {
  return (
    <section className={cn("flow-stage", !lead && "mt-6")}>
      <div className="mb-2 flex items-center gap-2">
        <span className={cn("grid size-5 place-items-center rounded-full text-2xs font-bold text-background", tone)}>
          {step}
        </span>
        <span className="text-2xs font-bold uppercase tracking-caps-wide text-slate">{title}</span>
        <span className="text-2xs text-muted-foreground">· {hint}</span>
      </div>
      {children}
    </section>
  );
}

/// The source connection in any of its three forms: a plain URL, an env ref
/// (`{ env = "VAR" }`), or host/port/user/password/database parts.
function ConnectionEditor({ source, onChange }: { source: Source; onChange: (s: Source) => void }) {
  const { t } = useT();
  const cu = source.connection_url;
  const isObj = !!cu && typeof cu === "object";
  const obj = (cu ?? {}) as Record<string, unknown>;
  const mode: "url" | "env" | "parts" =
    typeof cu === "string" ? "url" : isObj && "env" in obj ? "env" : isObj && "host" in obj ? "parts" : "url";
  const setCu = (v: unknown) => onChange({ ...source, connection_url: v });

  return (
    <div className="connection-editor">
      <Field label={t("config.connection")}>
        <Select
          value={mode}
          options={["url", "env", "parts"]}
          onChange={(m) => {
            if (m === "url") setCu("postgres://user:pass@127.0.0.1:5432/db");
            else if (m === "env") setCu({ env: "DATABASE_URL" });
            else setCu({ host: "127.0.0.1", port: 5432, user: "postgres", password: "", database: "postgres" });
          }}
        />
      </Field>
      {mode === "url" && (
        <Field label="url">
          <Text
            value={typeof cu === "string" ? cu : ""}
            onChange={setCu}
            placeholder="postgres://user:pass@host:5432/db"
          />
        </Field>
      )}
      {mode === "env" && (
        <Field label={t("config.envVar")}>
          <Text value={(obj.env as string) ?? ""} onChange={(v) => setCu({ env: v })} placeholder="DATABASE_URL" />
        </Field>
      )}
      {mode === "parts" && (
        <>
          <div className="flex flex-wrap gap-3">
            <Field label={t("config.host")}>
              <Text value={(obj.host as string) ?? ""} onChange={(v) => setCu({ ...obj, host: v })} />
            </Field>
            <Field label={t("config.port")}>
              <Num
                value={typeof obj.port === "number" ? obj.port : undefined}
                onChange={(v) => setCu({ ...obj, port: v })}
              />
            </Field>
          </div>
          <Field label={t("config.user")}>
            <Text value={(obj.user as string) ?? ""} onChange={(v) => setCu({ ...obj, user: v })} />
          </Field>
          <Field label={t("config.password")}>
            <Text
              value={typeof obj.password === "string" ? obj.password : ""}
              onChange={(v) => setCu({ ...obj, password: v || undefined })}
            />
          </Field>
          <Field label={t("config.database")}>
            <Text value={(obj.database as string) ?? ""} onChange={(v) => setCu({ ...obj, database: v })} />
          </Field>
        </>
      )}
    </div>
  );
}

/// The sink kind picker: a two-option segmented toggle (OpenSearch / stdout)
/// with icons — sized to content, so it never stretches the header the way a
/// full-width select did.
function SinkTypeToggle({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const opts = [
    { v: "opensearch", label: "OpenSearch", Icon: Search },
    { v: "stdout", label: "stdout", Icon: Terminal },
  ];
  return (
    <div className="inline-flex rounded-md border border-border bg-background p-0.5">
      {opts.map(({ v, label, Icon }) => (
        <button
          key={v}
          type="button"
          onClick={() => onChange(v)}
          aria-pressed={value === v}
          className={cn(
            "inline-flex cursor-pointer items-center gap-1.5 rounded px-2 py-1 text-2xs font-medium transition-colors",
            value === v ? "bg-primary/15 text-primary" : "text-muted-foreground hover:text-foreground",
          )}
        >
          <Icon className="size-3.5" />
          {label}
        </button>
      ))}
    </div>
  );
}

/// Edits one sink. Common fields are typed inputs; everything else round-trips
/// untouched (the sink object is preserved and only the edited keys change).
/// OpenSearch's rarely-touched index knobs live in a collapsed "tuning" drawer;
/// stdout has only `pretty`, so its card stays a single row.
function SinkEditor({
  name,
  sink,
  taken,
  onChange,
  onRename,
  onRemove,
}: {
  name: string;
  sink: Sink;
  taken: string[];
  onChange: (s: Sink) => void;
  onRename: (to: string) => void;
  onRemove: () => void;
}) {
  const { t } = useT();
  const type = (sink.type as string) ?? "opensearch";
  const os = type === "opensearch";
  const set = (key: string, value: unknown) => onChange({ ...sink, [key]: value });
  const str = (key: string) => (typeof sink[key] === "string" ? sink[key] : "");
  const num = (key: string) => (typeof sink[key] === "number" ? sink[key] : undefined);
  const bool = (key: string) => sink[key] === true;

  // Local draft so a half-typed name doesn't rename the config-map key on every
  // keystroke (which would remount this card and drop focus). Commit on blur /
  // Enter; revert to the current name on an empty / duplicate entry.
  const [draft, setDraft] = useState(name);
  const commitName = () => {
    const v = draft.trim();
    if (!v || v === name || taken.includes(v)) setDraft(name);
    else onRename(v);
  };

  return (
    <div
      className={cn(
        "sink-editor my-1.5 rounded-lg border border-l-2 border-border bg-secondary p-2.5",
        os ? "border-l-primary" : "border-l-slate",
      )}
    >
      <div className="sink-head flex items-center gap-2.5">
        <Text
          value={draft}
          onChange={setDraft}
          onBlur={commitName}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          invalid={taken.includes(draft.trim()) && draft.trim() !== name}
          placeholder={t("config.name")}
          className="w-40 font-semibold"
        />
        <SinkTypeToggle value={type} onChange={(ty) => set("type", ty)} />
        <div className="flex-1" />
        <RemoveButton label={t("common.remove")} onClick={onRemove} />
      </div>
      {os ? (
        <div className="mt-2">
          <Field label="url">
            <Text value={str("url")} onChange={(v) => set("url", v)} placeholder="http://127.0.0.1:9200" />
          </Field>
          <Field label="username">
            <Text value={str("username")} onChange={(v) => set("username", v || undefined)} />
          </Field>
          <Check value={bool("tls_verify")} label="tls_verify" onChange={(v) => set("tls_verify", v)} />
          <Drawer title={t("config.indexTuning")}>
            <div className="flex flex-wrap gap-3">
              <Field label="batch_size">
                <Num value={num("batch_size")} onChange={(v) => set("batch_size", v)} />
              </Field>
              <Field label="shards">
                <Num value={num("number_of_shards")} onChange={(v) => set("number_of_shards", v)} />
              </Field>
              <Field label="replicas">
                <Num value={num("number_of_replicas")} onChange={(v) => set("number_of_replicas", v)} />
              </Field>
            </div>
            <Field label="refresh_interval">
              <Text
                value={str("refresh_interval")}
                onChange={(v) => set("refresh_interval", v || undefined)}
                placeholder="1s"
              />
            </Field>
          </Drawer>
        </div>
      ) : (
        <div className="mt-2">
          <Check value={bool("pretty")} label="pretty" onChange={(v) => set("pretty", v)} />
        </div>
      )}
    </div>
  );
}
