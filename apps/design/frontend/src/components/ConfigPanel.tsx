import type { ConfigToml, IndexEntry } from "../api";
import { useT } from "../i18n";
import { Button } from "@/components/ui/button";
import { Check, Field, Num, PanelTitle, SectionTitle, Select, Text } from "./widgets";

type Sink = Record<string, unknown>;
type Source = Record<string, unknown>;

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

  return (
    <div className="config-panel max-w-3xl">
      <PanelTitle>{t("sidebar.deployment")}</PanelTitle>
      <Field label={t("config.indexPrefix")}>
        <Text value={config.prefix ?? ""} onChange={(prefix) => onChange({ ...config, prefix })} placeholder={t("config.none")} />
      </Field>
      <ConnectionEditor source={config.source} onChange={(source) => onChange({ ...config, source })} />
      <Check
        value={(config.source)?.manage_publication !== false}
        label="manage_publication"
        onChange={(v) => onChange({ ...config, source: { ...config.source, manage_publication: v } })}
      />
      <Field label="on_error (global)">
        <Select
          value={((config.on_error as string) ?? "stop") as "stop" | "skip"}
          options={["stop", "skip"]}
          onChange={(v) => onChange({ ...config, on_error: v })}
        />
      </Field>
      <div className="flex flex-wrap gap-3">
        <Field label="public_address">
          <Text
            value={((config.server)?.public_address as string) ?? ""}
            onChange={(v) => onChange({ ...config, server: { ...config.server, public_address: v || undefined } })}
            placeholder="127.0.0.1:9464"
          />
        </Field>
        <Field label="private_address">
          <Text
            value={((config.server)?.private_address as string) ?? ""}
            onChange={(v) => onChange({ ...config, server: { ...config.server, private_address: v || undefined } })}
            placeholder="127.0.0.1:9465"
          />
        </Field>
      </div>

      <SectionTitle>{t("config.sinks")}</SectionTitle>
      {Object.entries(sinks).map(([name, sink]) => (
        <SinkEditor key={name} name={name} sink={sink} onChange={(s) => setSink(name, s)} onRemove={() => removeSink(name)} />
      ))}
      <Button
        variant="link"
        size="sm"
        onClick={() => setSink(`sink${Object.keys(sinks).length + 1}`, { type: "opensearch", url: "http://127.0.0.1:9200" })}
      >
        + {t("config.sink")}
      </Button>

      <SectionTitle>{t("sidebar.indexes")}</SectionTitle>
      {index.map((e, i) => (
        <div className="index-entry my-1 flex items-center gap-1.5" key={i}>
          <Text value={e.name} onChange={(name) => setEntry(i, { ...e, name })} placeholder={t("config.name")} />
          <Text value={e.schema} onChange={(schema) => setEntry(i, { ...e, schema })} placeholder="x.schema.yml" />
          <Check value={e.enabled} label={t("config.enabled")} onChange={(enabled) => setEntry(i, { ...e, enabled })} />
          <Select
            value={((e.on_error as string) ?? "default") as "default" | "stop" | "skip"}
            options={["default", "stop", "skip"]}
            onChange={(v) => setEntry(i, { ...e, on_error: v === "default" ? undefined : v })}
          />
          <Button variant="link" size="sm" title={t("config.duplicate")} onClick={() => onDuplicate(i)}>
            {t("config.dup")}
          </Button>
          <Button
            variant="link"
            size="sm"
            className="text-destructive"
            onClick={() => {
              if (confirm(t("config.removeIndex", { name: e.name })))
                onChange({ ...config, index: index.filter((_, j) => j !== i) });
            }}
          >
            ✕
          </Button>
        </div>
      ))}
      <Button
        variant="link"
        size="sm"
        onClick={() =>
          onChange({ ...config, index: [...index, { name: "new_index", schema: "new_index.schema.yml", enabled: true }] })
        }
      >
        + {t("config.index")}
      </Button>
    </div>
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
          <Text value={typeof cu === "string" ? cu : ""} onChange={setCu} placeholder="postgres://user:pass@host:5432/db" />
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
                value={typeof obj.port === "number" ? (obj.port) : undefined}
                onChange={(v) => setCu({ ...obj, port: v })}
              />
            </Field>
          </div>
          <Field label={t("config.user")}>
            <Text value={(obj.user as string) ?? ""} onChange={(v) => setCu({ ...obj, user: v })} />
          </Field>
          <Field label={t("config.password")}>
            <Text
              value={typeof obj.password === "string" ? (obj.password) : ""}
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

/// Edits one sink. Common fields are typed inputs; everything else round-trips
/// untouched (the sink object is preserved and only the edited keys change).
function SinkEditor({
  name,
  sink,
  onChange,
  onRemove,
}: {
  name: string;
  sink: Sink;
  onChange: (s: Sink) => void;
  onRemove: () => void;
}) {
  const { t } = useT();
  const type = (sink.type as string) ?? "opensearch";
  const set = (key: string, value: unknown) => onChange({ ...sink, [key]: value });
  const str = (key: string) => (typeof sink[key] === "string" ? (sink[key]) : "");
  const num = (key: string) => (typeof sink[key] === "number" ? (sink[key]) : undefined);
  const bool = (key: string) => sink[key] === true;

  return (
    <div className="sink-editor my-1.5 rounded-lg border border-border p-2.5">
      <div className="sink-head mb-2 flex items-center gap-2.5">
        <strong>{name}</strong>
        <Select value={type} options={["opensearch", "stdout"]} onChange={(ty) => set("type", ty)} />
        <Button variant="link" size="sm" className="text-destructive" onClick={onRemove}>
          {t("common.remove")}
        </Button>
      </div>
      {type === "opensearch" ? (
        <>
          <Field label="url">
            <Text value={str("url")} onChange={(v) => set("url", v)} placeholder="http://127.0.0.1:9200" />
          </Field>
          <Field label="username">
            <Text value={str("username")} onChange={(v) => set("username", v || undefined)} />
          </Field>
          <Check value={bool("tls_verify")} label="tls_verify" onChange={(v) => set("tls_verify", v)} />
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
            <Text value={str("refresh_interval")} onChange={(v) => set("refresh_interval", v || undefined)} placeholder="1s" />
          </Field>
        </>
      ) : (
        <Check value={bool("pretty")} label="pretty" onChange={(v) => set("pretty", v)} />
      )}
    </div>
  );
}
