import type { ConfigToml, IndexEntry } from "../api";
import { Check, Field, Text } from "./widgets";

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

  const setEntry = (i: number, e: IndexEntry) => {
    const next = index.slice();
    next[i] = e;
    onChange({ ...config, index: next });
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
