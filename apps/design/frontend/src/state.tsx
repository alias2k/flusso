// Shared canvas state, exposed via context so React Flow custom nodes (which
// only receive `data`) can read the catalog and dispatch path-addressed edits.

import { createContext, useContext } from "react";
import type { CatalogResponse, ColumnShape, DiagnosticDto, IndexSchema } from "./api";

export type Selection =
  | { kind: "root" }
  | { kind: "node"; path: number[] }
  | { kind: "field"; path: number[]; index: number }
  /// Multiple catalog columns of the node at `path`, selected by name — drives
  /// the Inspector's bulk include/exclude panel.
  | { kind: "columns"; path: number[]; names: string[] }
  | null;

export interface DesignCtx {
  catalog: CatalogResponse | null;
  schema: IndexSchema;
  indexName: string;
  apply: (fn: (s: IndexSchema) => IndexSchema) => void;
  selection: Selection;
  select: (s: Selection) => void;
  columnsFor: (table: string) => ColumnShape[];
  /// Live DB-validation diagnostics for the active index, keyed by field name.
  diagnostics: DiagnosticDto[];
  /// Node ids (path ids) the user has collapsed to just their header.
  collapsed: Set<string>;
  toggleCollapsed: (id: string) => void;
}

const Ctx = createContext<DesignCtx | null>(null);

export const DesignProvider = Ctx.Provider;

export function useDesign(): DesignCtx {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useDesign outside a DesignProvider");
  return ctx;
}
