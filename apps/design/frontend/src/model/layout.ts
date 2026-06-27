// A tidy left-to-right tree layout: depth → column; each subtree is packed into
// its own vertical band sized to that subtree's *estimated* height, so tall
// nodes (lots of columns + a footer) never overlap their siblings. Positions
// aren't schema data, so a manual drag is remembered per-index in localStorage
// and overlaid on the auto-layout.

import type { DocNode } from "./tree";

export type Positions = Record<string, { x: number; y: number }>;

const COL_W = 360;
const GAP = 44;

/// `height(node)` is the caller's pixel estimate of a node's rendered height
/// (the canvas knows the catalog, so it can estimate column/footer rows).
export function autoLayout(nodes: DocNode[], height: (n: DocNode) => number): Positions {
  const byParent = new Map<string, DocNode[]>();
  for (const n of nodes) {
    const key = n.parentId ?? "";
    (byParent.get(key) ?? byParent.set(key, []).get(key)!).push(n);
  }
  const pos: Positions = {};

  // Place `node`'s subtree starting at vertical offset `top`; return the band
  // height it consumed so the caller can advance past it.
  const place = (node: DocNode, top: number): number => {
    const x = node.depth * COL_W;
    const ownH = height(node);
    const kids = byParent.get(node.id) ?? [];
    if (kids.length === 0) {
      pos[node.id] = { x, y: top };
      return ownH;
    }
    let cursor = top;
    for (const k of kids) cursor += place(k, cursor) + GAP;
    const childrenSpan = cursor - GAP - top;
    const band = Math.max(childrenSpan, ownH);
    pos[node.id] = { x, y: top + (band - ownH) / 2 };
    return band;
  };

  const root = nodes.find((n) => n.parentId === null);
  if (root) place(root, 0);
  return pos;
}

const key = (index: string) => `flusso-design.layout.${index}`;

export function loadOverrides(index: string): Positions {
  try {
    return JSON.parse(localStorage.getItem(key(index)) ?? "{}") as Positions;
  } catch {
    return {};
  }
}

export function saveOverride(index: string, id: string, x: number, y: number) {
  const all = loadOverrides(index);
  all[id] = { x, y };
  try {
    localStorage.setItem(key(index), JSON.stringify(all));
  } catch {
    /* storage full / disabled — layout just won't persist */
  }
}
