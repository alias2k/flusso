// AST-based anchors for the Code editor: precise source ranges for things the
// backend can only *name* (a field, its type tag, an offending value). Walks
// the `yaml` package's parsed document, whose nodes carry byte ranges — no
// string searching, so a duplicate name in another block can't mislead the
// squiggle.

import { isMap, isScalar, visit, type Document, type YAMLMap } from "yaml";
import { SCALAR_TYPES } from "../api";

export interface Span {
  from: number;
  to: number;
}

/// A structured parse error from `/api/parse`: the clean message plus either a
/// trustworthy source position or the field (and its type tag) it names.
export interface ParseErrorInfo {
  message: string;
  location?: { line: number; column: number };
  field?: string;
  typeTag?: string;
}

// Every key that introduces a field in the type-first syntax — the scalar
// types plus the structured/relation/aggregate kinds.
const FIELD_TAGS = new Set<string>([
  ...(SCALAR_TYPES as string[]),
  "geo",
  "map",
  "custom",
  "constant",
  "object",
  "belongs_to",
  "has_one",
  "has_many",
  "many_to_many",
  "count",
  "sum",
  "avg",
  "min",
  "max",
  "ids",
]);

const spanOf = (node: unknown): Span | null => {
  const range = isScalar(node) ? node.range : null;
  return range ? { from: range[0], to: range[1] } : null;
};

/// The `<tag>: <name>` pair introducing the document field `name`: its value
/// node's span, plus the field's own mapping (to search siblings). With `tag`
/// the match is exact; without, any field-tag key with that value matches.
export function anchorField(doc: Document, name: string, tag?: string): { span: Span; map: YAMLMap | null } | null {
  let found: { span: Span; map: YAMLMap | null } | null = null;
  visit(doc, {
    Pair(_, pair, path) {
      if (!isScalar(pair.key) || !isScalar(pair.value)) return undefined;
      if (String(pair.value.value) !== name) return undefined;
      const key = String(pair.key.value);
      if (tag ? key !== tag : !FIELD_TAGS.has(key)) return undefined;
      const span = spanOf(pair.value);
      if (!span) return undefined;
      const parent = path[path.length - 1];
      found = { span, map: isMap(parent) ? parent : null };
      return visit.BREAK;
    },
  });
  return found;
}

/// The first of `map`'s direct values whose scalar text equals `text` — the
/// offending value inside an already-located field's block.
export function anchorValueIn(map: YAMLMap, text: string): Span | null {
  for (const item of map.items) {
    if (isScalar(item.value) && String(item.value.value) === text) {
      const span = spanOf(item.value);
      if (span) return span;
    }
  }
  return null;
}

/// The first of `map`'s direct keys whose scalar text equals `text` — the
/// offending *key* (an unknown or misplaced sibling) inside an already-located
/// field's block.
export function anchorKeyIn(map: YAMLMap, text: string): Span | null {
  for (const item of map.items) {
    if (isScalar(item.key) && String(item.key.value) === text) {
      const span = spanOf(item.key);
      if (span) return span;
    }
  }
  return null;
}

/// Any pair whose key *or* value scalar equals `token` — the loose anchor for
/// conversion errors, which name sibling keys/ops rather than fields.
export function anchorToken(doc: Document, token: string): Span | null {
  let found: Span | null = null;
  visit(doc, {
    Pair(_, pair) {
      for (const node of [pair.key, pair.value]) {
        if (isScalar(node) && String(node.value) === token) {
          const span = spanOf(node);
          if (span) {
            found = span;
            return visit.BREAK;
          }
        }
      }
      return undefined;
    },
  });
  return found;
}

/// Byte offset of a 1-based line/column position.
export function offsetOf(text: string, line: number, column: number): number {
  let offset = 0;
  for (let l = 1; l < line; l += 1) {
    const nl = text.indexOf("\n", offset);
    if (nl === -1) break;
    offset = nl + 1;
  }
  return Math.min(offset + column - 1, text.length);
}

/// Where a parse error's squiggle belongs, best anchor first:
/// 1. the reported source position (top-level syntax errors), extended over
///    the token it points at;
/// 2. the named field — and within its block, the quoted offending value or
///    the back-ticked offending key (an unknown/misplaced sibling) when the
///    message carries one;
/// 3. any back-ticked token from the message (conversion errors name sibling
///    keys/ops);
/// 4. the buffer start.
export function anchorParseError(doc: Document, text: string, error: ParseErrorInfo): Span {
  if (error.location) {
    const from = offsetOf(text, error.location.line, error.location.column);
    const token = /^[^\s:,#[\]{}]+/.exec(text.slice(from));
    return { from, to: from + Math.max(token?.[0].length ?? 1, 1) };
  }
  if (error.field) {
    const hit = anchorField(doc, error.field, error.typeTag);
    if (hit) {
      if (hit.map) {
        for (const m of error.message.matchAll(/"([^"]+)"/g)) {
          const span = m[1] && anchorValueIn(hit.map, m[1]);
          if (span) return span;
        }
        for (const m of error.message.matchAll(/`([^`]+)`/g)) {
          const token = m[1];
          if (!token || token === error.field || token === error.typeTag) continue;
          const span = anchorKeyIn(hit.map, token);
          if (span) return span;
        }
      }
      return hit.span;
    }
  }
  for (const m of [...error.message.matchAll(/`([^`]+)`/g)].reverse()) {
    const span = m[1] && anchorToken(doc, m[1]);
    if (span) return span;
  }
  return { from: 0, to: 0 };
}
