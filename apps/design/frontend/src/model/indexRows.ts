// Index rows of the Deployment panel, compared the way the server compares
// them: names as flusso's `IndexName` normalizes them, paths lexically
// normalized, and rows paired with their earlier state by their client id,
// never by position (a removal must not read as a neighbour's rename).
//
// ```ts
// indexName(" Orders") === "orders";
// nameProblem("my users", []) === "pattern";
// schemaPath("x/../orders.schema.yml") === "orders.schema.yml";
// const pairs = pairRows(before, after);
// const schemas = rekeyByRow(doc.schemas, pairs); // keyed by each row's new name
// ```

import type { IndexEntry } from "../api";
import { identifier, identifierProblem } from "./names";

/// An index name as flusso reads it: trimmed and lowercased.
export const indexName = identifier;

/// Why flusso would refuse `name` for an index, or `null` (see
/// `identifierProblem`).
export const nameProblem = identifierProblem;

/// `path` as the server saves it: `.` segments, doubled and trailing slashes
/// dropped, `..` kept.
export function cleanSchemaPath(path: string): string {
  const parts = path.split("/").filter((p) => p !== "" && p !== ".");
  return (path.startsWith("/") ? "/" : "") + parts.join("/");
}

/// The schema path an index named `name` gets by default.
export const defaultSchemaPath = (name: string): string => `${indexName(name)}.schema.yml`;

/// `path` with `.` segments, empty segments and `x/..` pairs removed, as the
/// server resolves it. A leading `..` that has nothing to cancel is kept.
export function schemaPath(path: string): string {
  const absolute = path.startsWith("/");
  const out: string[] = [];
  for (const part of path.trim().split("/")) {
    if (part === "" || part === ".") continue;
    if (part === ".." && out.length > 0 && out[out.length - 1] !== "..") out.pop();
    else if (!(part === ".." && absolute)) out.push(part);
  }
  return (absolute ? "/" : "") + out.join("/");
}

/// Whether `path`, relative to the config's directory, is the config file at
/// `configPath` (absolute, or relative to the same directory).
export function isConfigFile(path: string, configPath: string): boolean {
  const norm = schemaPath(path);
  const config = schemaPath(configPath);
  return norm === config || norm === config.slice(config.lastIndexOf("/") + 1);
}

/// One row now, and the same row (by id) in the earlier list, if it was there.
export interface RowPair {
  entry: IndexEntry;
  old?: IndexEntry;
}

/// Pair each row of `next` with its earlier state in `prev`, by id.
export function pairRows(prev: IndexEntry[] | undefined, next: IndexEntry[] | undefined): RowPair[] {
  const byId = new Map((prev ?? []).filter((e) => e.id).map((e) => [e.id, e]));
  return (next ?? []).map((entry) => ({ entry, old: entry.id ? byId.get(entry.id) : undefined }));
}

/// A per-index map (keyed by index name) re-keyed to each row's current name.
/// Removed and new rows get nothing. A row that kept its name claims it first,
/// so a transient clash never hands its value to another row.
export function rekeyByRow<T>(map: Record<string, T>, pairs: RowPair[]): Record<string, T> {
  const out: Record<string, T> = {};
  const renamed = (p: RowPair) => Number(p.old?.name !== p.entry.name);
  for (const { entry, old } of [...pairs].sort((a, b) => renamed(a) - renamed(b))) {
    if (old && old.name in map && !(entry.name in out)) out[entry.name] = map[old.name];
  }
  return out;
}
