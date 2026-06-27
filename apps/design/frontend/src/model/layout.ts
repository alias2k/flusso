// A tidy left-to-right tree layout: depth → column, children stacked and their
// parent centered over them. Positions aren't schema data, so a manual drag is
// remembered per-index in localStorage and overlaid on the auto-layout.

import type { DocNode } from "./tree";

export type Positions = Record<string, { x: number; y: number }>;

const COL_W = 340;
const ROW_H = 150;

export function autoLayout(nodes: DocNode[]): Positions {
  const byParent = new Map<string, DocNode[]>();
  for (const n of nodes) {
    const key = n.parentId ?? "";
    (byParent.get(key) ?? byParent.set(key, []).get(key)!).push(n);
  }
  const pos: Positions = {};
  let row = 0;
  const place = (node: DocNode) => {
    const kids = byParent.get(node.id) ?? [];
    if (kids.length === 0) {
      pos[node.id] = { x: node.depth * COL_W, y: row * ROW_H };
      row += 1;
      return;
    }
    kids.forEach(place);
    const ys = kids.map((k) => pos[k.id].y);
    pos[node.id] = { x: node.depth * COL_W, y: (Math.min(...ys) + Math.max(...ys)) / 2 };
  };
  const root = nodes.find((n) => n.parentId === null);
  if (root) place(root);
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
