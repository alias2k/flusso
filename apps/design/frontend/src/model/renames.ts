// Renames by identity. A name box applies what it holds through one of these,
// given the identity of what it renames (an index row's id, a sink's id, a
// field's uid) and the typed text: the thing is looked up in the state as it is
// now, the name is checked against its siblings as they are now, and nothing
// happens when the thing is gone or the name is refused. So a rename applied
// late (a box unmounted while it held a name) can never land on whatever took
// the thing's place.
//
// ```ts
// renameField(schema, uid, " born ").problem === "taken"; // another field is `born`
// renameSinkIn(config, sinkId, "Audit").config.sinks.audit; // read as flusso reads it
// ```

import type { ConfigToml, IndexSchema } from "../api";

type Sink = Record<string, unknown>;
import { updateField } from "./edit";
import { fieldNameProblem, identifier, identifierProblem, trimmed } from "./names";
import { locateField } from "./selection";
import { nodeFields } from "./tree";

/// Why a rename did not apply: the name is refused, or the thing is gone.
export type RenameProblem = "taken" | "pattern" | "length" | "gone";

/// The field `uid` of `schema` renamed to `text` as flusso reads a field name.
export function renameField(
  schema: IndexSchema,
  uid: string,
  text: string,
): { schema: IndexSchema; problem: RenameProblem | null } {
  const at = locateField(schema, uid);
  const fields = at ? nodeFields(schema, at.path) : [];
  const field = at ? fields[at.index] : undefined;
  if (!at || !field) return { schema, problem: "gone" };
  const name = trimmed(text);
  if (name === field.field) return { schema, problem: null };
  const siblings = fields.filter((_, i) => i !== at.index).map((f) => f.field);
  const problem = fieldNameProblem(name, siblings);
  if (problem) return { schema, problem };
  return { schema: updateField(schema, uid, (now) => ({ ...now, field: name })), problem: null };
}

/// The `[[index]]` entry with row id `id` renamed to `text` as flusso reads an
/// index name.
export function renameIndexIn(
  config: ConfigToml,
  id: string,
  text: string,
): { config: ConfigToml; problem: RenameProblem | null } {
  const entries = config.index ?? [];
  const entry = entries.find((e) => e.id === id);
  if (!entry) return { config, problem: "gone" };
  const name = identifier(text);
  if (!name || name === entry.name) return { config, problem: null };
  const problem = identifierProblem(
    name,
    entries.filter((e) => e.id !== id).map((e) => e.name),
  );
  if (problem) return { config, problem };
  return { config: { ...config, index: entries.map((e) => (e.id === id ? { ...e, name } : e)) }, problem: null };
}

/// The sink with id `id` (its name, for a sink without one) renamed to `text`
/// as flusso reads a sink name: in place, keeping declaration order, its id,
/// and its per-index options.
export function renameSinkIn(
  config: ConfigToml,
  id: string,
  text: string,
): { config: ConfigToml; problem: RenameProblem | null } {
  const sinks = (config.sinks ?? {}) as Record<string, Sink>;
  const from = sinkNamed(config, id);
  if (from === undefined) return { config, problem: "gone" };
  const name = identifier(text);
  if (!name || name === from) return { config, problem: null };
  const problem = identifierProblem(
    name,
    Object.keys(sinks).filter((n) => n !== from),
  );
  if (problem) return { config, problem };
  const next: Record<string, Sink> = {};
  for (const [k, v] of Object.entries(sinks)) next[k === from ? name : k] = v;
  const sinkIds: Record<string, string> = {};
  for (const [k, v] of Object.entries(config.sinkIds ?? {})) sinkIds[k === from ? name : k] = v;
  return { config: { ...config, sinks: next, sinkIds, index: reKeyIndexSinks(config, from, name) }, problem: null };
}

/// The name of the sink with id `id` now (or the sink named `id`, for one that
/// has no id).
export function sinkNamed(config: ConfigToml, id: string): string | undefined {
  const byId = Object.entries(config.sinkIds ?? {}).find(([, v]) => v === id)?.[0];
  if (byId !== undefined && byId in (config.sinks ?? {})) return byId;
  return id in (config.sinks ?? {}) ? id : undefined;
}

/// The `[[index]]` entries with their per-index options for sink `from` moved
/// to `to`, or dropped when `to` is `undefined`: they are keyed by sink name,
/// so they follow a rename and go with a removal (a dangling key would fail
/// validation).
export function reKeyIndexSinks(config: ConfigToml, from: string, to: string | undefined) {
  return (config.index ?? []).map((e) => {
    if (!e.sinks || !(from in e.sinks)) return e;
    const next: Record<string, Record<string, unknown>> = {};
    for (const [k, v] of Object.entries(e.sinks)) {
      if (k !== from) next[k] = v;
      else if (to) next[to] = v;
    }
    return { ...e, sinks: Object.keys(next).length ? next : undefined };
  });
}
