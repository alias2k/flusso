import { type ReactNode, useState } from "react";
import { Copy, Plug, Plus, Search, Terminal } from "lucide-react";
import type { AdapterDescription, ConfigToml, IndexEntry } from "../api";
import { useT } from "../i18n";
import { LABEL } from "../styles";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useAdapters } from "../model/adapters";
import { AdapterForm, KindBadge, KindToggle } from "./AdapterForm";
import { Check, Drawer, Field, PanelTitle, RemoveButton, Select, Text } from "./widgets";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

type Sink = Record<string, unknown>;

/// The icon for a sink kind; adapters the designer doesn't know get a plug.
function kindIcon(kind: string): ReactNode {
  if (kind === "opensearch") return <Search className="size-3.5" />;
  if (kind === "stdout") return <Terminal className="size-3.5" />;
  return <Plug className="size-3.5" />;
}

/// Split a port entry into its `type` and the adapter's options.
function splitEntry(entry: Record<string, unknown> | undefined): { kind: string; options: Record<string, unknown> } {
  const { type, ...options } = entry ?? {};
  return { kind: typeof type === "string" ? type : "", options };
}

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
  const adapters = useAdapters() ?? [];
  const index = config.index ?? [];
  const sinks = (config.sinks ?? {}) as Record<string, Sink>;
  const [pendingRemove, setPendingRemove] = useState<number | null>(null);
  const sourceAdapters = adapters.filter((a) => a.port === "source");
  const streamAdapters = adapters.filter((a) => a.port === "stream");
  const sinkAdapters = adapters.filter((a) => a.port === "sink");
  const source = splitEntry(config.source);
  const stream = splitEntry(config.stream);
  const sourceDescription = sourceAdapters.find((a) => a.kind === source.kind);
  const streamDescription = streamAdapters.find((a) => a.kind === (stream.kind || "channel"));

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

      <Stage
        step={1}
        tone="bg-kind-root"
        title={t("config.source")}
        hint={source.kind || t("config.loadingAdapters")}
        lead
      >
        <div className="rounded-lg border border-l-2 border-border border-l-kind-root bg-secondary p-3">
          <div className="mb-2 flex items-center gap-2">
            {sourceAdapters.length > 1 ? (
              <KindToggle
                kinds={sourceAdapters.map((a) => a.kind)}
                value={source.kind}
                onChange={(kind) => onChange({ ...config, source: { type: kind } })}
                icon={kindIcon}
              />
            ) : (
              <KindBadge kind={source.kind} />
            )}
          </div>
          {sourceDescription && (
            <AdapterForm
              description={sourceDescription}
              value={source.options}
              onChange={(options) => onChange({ ...config, source: { type: source.kind, ...options } })}
            />
          )}
        </div>
        <Drawer title={t("config.stream")} count={Object.keys(stream.options).length || undefined}>
          <div className="rounded-lg border border-border bg-secondary p-3">
            <div className="mb-2 flex items-center gap-2">
              {streamAdapters.length > 1 ? (
                <KindToggle
                  kinds={streamAdapters.map((a) => a.kind)}
                  value={stream.kind || "channel"}
                  onChange={(kind) => onChange({ ...config, stream: { type: kind } })}
                  icon={kindIcon}
                />
              ) : (
                <KindBadge kind={stream.kind || "channel"} />
              )}
              <span className="text-2xs text-muted-foreground">{t("config.streamHint")}</span>
            </div>
            {streamDescription && (
              <AdapterForm
                description={streamDescription}
                value={stream.options}
                onChange={(options) =>
                  onChange({
                    ...config,
                    stream: Object.keys(options).length ? { type: streamDescription.kind, ...options } : undefined,
                  })
                }
              />
            )}
          </div>
        </Drawer>
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
            // Existing files + the name-derived suggestion, offered as datalist
            // autocomplete on an editable path field (not a fixed picker).
            const schemaList = Array.from(new Set([suggestion, ...index.map((x) => x.schema)].filter(Boolean)));
            return (
              <div key={i} className={cn(INDEX_COLS, "px-3 py-1.5")}>
                <Text value={e.name} onChange={(name) => setEntry(i, { ...e, name })} placeholder={t("config.name")} />
                <Text
                  value={e.schema}
                  onChange={(schema) => setEntry(i, { ...e, schema })}
                  list={schemaList}
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
            adapters={sinkAdapters}
            taken={Object.keys(sinks).filter((n) => n !== name)}
            onChange={(s) => setSink(name, s)}
            onRename={(to) => renameSink(name, to)}
            onRemove={() => removeSink(name)}
          />
        ))}
        <AddDashed
          label={t("config.sink")}
          onClick={() => {
            const first = sinkAdapters[0];
            setSink(`sink${Object.keys(sinks).length + 1}`, first ? { type: first.kind, ...first.example } : {});
          }}
        />
      </Stage>

      <Dialog open={pendingRemove !== null} onOpenChange={(o) => !o && setPendingRemove(null)}>
        <DialogContent showCloseButton={false} className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("config.removeIndexTitle")}</DialogTitle>
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

/// Edits one sink: its name, its kind (one toggle per registered sink adapter),
/// and that adapter's options, rendered from the adapter's own declaration.
function SinkEditor({
  name,
  sink,
  adapters,
  taken,
  onChange,
  onRename,
  onRemove,
}: {
  name: string;
  sink: Sink;
  adapters: AdapterDescription[];
  taken: string[];
  onChange: (s: Sink) => void;
  onRename: (to: string) => void;
  onRemove: () => void;
}) {
  const { t } = useT();
  const { kind, options: entry } = splitEntry(sink);
  // `backfill` is the one universal sink key: it belongs to the sink engine, not
  // to the adapter, so it never reaches the adapter form.
  const { backfill: rawBackfill, ...options } = entry;
  const backfill = rawBackfill !== false;
  const withBackfill = (next: Record<string, unknown>, on: boolean) => (on ? next : { ...next, backfill: false });
  const description = adapters.find((a) => a.kind === kind);

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
        description ? "border-l-primary" : "border-l-slate",
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
        <KindToggle
          kinds={adapters.map((a) => a.kind)}
          value={kind}
          onChange={(next) => {
            const target = adapters.find((a) => a.kind === next);
            onChange(withBackfill({ type: next, ...(target?.example ?? {}) }, backfill));
          }}
          icon={kindIcon}
        />
        <div className="flex-1" />
        <RemoveButton label={t("common.remove")} onClick={onRemove} />
      </div>
      <div className="mt-2">
        {description ? (
          <AdapterForm
            description={description}
            value={options}
            onChange={(next) => onChange(withBackfill({ type: kind, ...next }, backfill))}
          />
        ) : (
          <span className="text-2xs text-warn">{t("config.unknownAdapter", { kind })}</span>
        )}
      </div>
      <div className="mt-2 flex items-center gap-2">
        <Check
          value={backfill}
          onChange={(on) => onChange(withBackfill({ type: kind, ...options }, on))}
          label={t("config.backfill")}
        />
        <span className="text-2xs text-muted-foreground">{t("config.backfillHint")}</span>
      </div>
    </div>
  );
}
