import type { ConfigToml, IndexEntry } from "../api";
import { Check, Field, Num, Select, Text } from "./widgets";

type Sink = Record<string, unknown>;

/// Read the connection URL out of the loose `[source]` table, if present as a
/// plain string. Parts-form / env-ref connections are shown read-only.
function connectionUrl(config: ConfigToml): string | null {
  const url = (config.source as Record<string, unknown>)?.connection_url;
  return typeof url === "string" ? url : null;
}

export function ConfigPanel({
  config,
  onChange,
}: {
  config: ConfigToml;
  onChange: (c: ConfigToml) => void;
}) {
  const url = connectionUrl(config);
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
    <div className="config-panel">
      <h2>Deployment</h2>
      <Field label="index prefix">
        <Text value={config.prefix ?? ""} onChange={(prefix) => onChange({ ...config, prefix })} placeholder="(none)" />
      </Field>
      {url !== null ? (
        <Field label="connection_url">
          <Text
            value={url}
            onChange={(v) => onChange({ ...config, source: { ...config.source, connection_url: v } })}
            placeholder="postgres://user:pass@host:5432/db"
          />
        </Field>
      ) : (
        <p className="hint">Connection is set via parts or an env ref — edit it in flusso.toml.</p>
      )}

      <h3>Sinks</h3>
      {Object.entries(sinks).map(([name, sink]) => (
        <SinkEditor key={name} name={name} sink={sink} onChange={(s) => setSink(name, s)} onRemove={() => removeSink(name)} />
      ))}
      <button
        className="link"
        onClick={() => setSink(`sink${Object.keys(sinks).length + 1}`, { type: "opensearch", url: "http://127.0.0.1:9200" })}
      >
        + sink
      </button>

      <h3>Indexes</h3>
      {index.map((e, i) => (
        <div className="index-entry" key={i}>
          <Text value={e.name} onChange={(name) => setEntry(i, { ...e, name })} placeholder="name" />
          <Text value={e.schema} onChange={(schema) => setEntry(i, { ...e, schema })} placeholder="x.schema.yml" />
          <Check value={e.enabled} label="enabled" onChange={(enabled) => setEntry(i, { ...e, enabled })} />
          <button className="link danger" onClick={() => onChange({ ...config, index: index.filter((_, j) => j !== i) })}>
            ✕
          </button>
        </div>
      ))}
      <button
        className="link"
        onClick={() =>
          onChange({ ...config, index: [...index, { name: "new_index", schema: "new_index.schema.yml", enabled: true }] })
        }
      >
        + index
      </button>
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
  const type = (sink.type as string) ?? "opensearch";
  const set = (key: string, value: unknown) => onChange({ ...sink, [key]: value });
  const str = (key: string) => (typeof sink[key] === "string" ? (sink[key] as string) : "");
  const num = (key: string) => (typeof sink[key] === "number" ? (sink[key] as number) : undefined);
  const bool = (key: string) => sink[key] === true;

  return (
    <div className="sink-editor">
      <div className="sink-head">
        <strong>{name}</strong>
        <Select value={type} options={["opensearch", "stdout"]} onChange={(t) => set("type", t)} />
        <button className="link danger" onClick={onRemove}>
          remove
        </button>
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
          <div className="row">
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
