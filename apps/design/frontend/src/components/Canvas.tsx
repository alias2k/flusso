import {
  Background,
  Controls,
  type Edge,
  MiniMap,
  type Node,
  ReactFlow,
  useEdgesState,
  useNodesState,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useEffect, useRef } from "react";
import { SCALAR_TYPES } from "../api";
import { autoLayout, loadOverrides, saveOverride } from "../model/layout";
import { suggestRelations } from "../model/relations";
import { type DocNode, projectGraph } from "../model/tree";
import { useDesign } from "../state";
import { DocNodeView } from "./DocNodeView";

const nodeTypes = { doc: DocNodeView };

export function Canvas() {
  const { schema, indexName, select, catalog } = useDesign();
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // Estimate a node's rendered height so the auto-layout reserves the right
  // vertical band (header + column rows, capped by the scroll area, + footer
  // whose size tracks the FK suggestion count).
  const estimateHeight = (n: DocNode): number => {
    const tableCols = catalog?.catalog.tables.find((t) => t.name === n.table)?.columns ?? [];
    const catalogNames = new Set(tableCols.map((c) => c.name));
    const special = n.leaves.filter(
      (l) => !((SCALAR_TYPES as string[]).includes(l.kind) && l.column && catalogNames.has(l.column)),
    ).length;
    const rows = (tableCols.length || n.leaves.length) + special;
    const colsH = Math.min(rows * 36 + 12, 280);
    const suggestions = catalog ? suggestRelations(catalog, n.table).length : 0;
    const footerH = 76 + suggestions * 34;
    return 64 + colsH + footerH;
  };

  // Re-project the tree to nodes/edges on every schema change, but *keep each
  // existing node's current position* — only brand-new nodes get an estimated
  // slot. So a field edit never moves anything, and this never clobbers the
  // measured layout (below) or a manual drag. Deliberately NOT keyed on
  // `catalog`: it arrives async, and re-running here would reset positions.
  useEffect(() => {
    const graph = projectGraph(schema);
    const auto = autoLayout(graph.nodes, estimateHeight);
    const overrides = loadOverrides(indexName);
    setNodes((prev) => {
      const prevPos = new Map(prev.map((n) => [n.id, n.position]));
      return graph.nodes.map((n) => ({
        id: n.id,
        type: "doc",
        position: overrides[n.id] ?? prevPos.get(n.id) ?? auto[n.id] ?? { x: 0, y: 0 },
        data: { node: n },
      }));
    });
    setEdges(graph.edges.map((e) => ({ id: e.id, source: e.source, target: e.target, label: e.label })));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schema, indexName, setNodes, setEdges]);

  // Estimates can't know real heights, so once React Flow has *measured* every
  // node, re-run the tidy layout with those true heights. Re-tidies when the
  // node set changes (add/remove/switch index) or when the catalog first loads
  // (FK suggestions change node heights) — not on every field edit, so editing
  // never reshuffles the canvas. Manual drags (overrides) still win.
  const laidOut = useRef("");
  useEffect(() => {
    const sig = `${indexName}|${catalog ? "c" : ""}|${nodes.map((n) => n.id).sort().join(",")}`;
    if (sig === laidOut.current) return;
    if (!nodes.length || nodes.some((n) => !n.measured?.height)) return; // wait for measurement
    laidOut.current = sig;
    const docNodes = nodes.map((n) => (n.data as { node: DocNode }).node);
    const heights = new Map(nodes.map((n) => [n.id, n.measured?.height ?? 0]));
    const auto = autoLayout(docNodes, (dn) => heights.get(dn.id) ?? 300);
    const overrides = loadOverrides(indexName);
    setNodes((current) =>
      current.map((n) => ({ ...n, position: overrides[n.id] ?? auto[n.id] ?? n.position })),
    );
  }, [nodes, indexName, catalog, setNodes]);

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      onNodeDragStop={(_, node) => saveOverride(indexName, node.id, node.position.x, node.position.y)}
      onPaneClick={() => select(null)}
      fitView
      minZoom={0.2}
    >
      <Background />
      <Controls />
      <MiniMap pannable zoomable />
    </ReactFlow>
  );
}
