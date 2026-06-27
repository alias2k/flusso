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
import { useEffect } from "react";
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
    const colsH = Math.min(rows * 24 + 12, 272);
    const suggestions = catalog ? suggestRelations(catalog, n.table).length : 0;
    const footerH = 52 + suggestions * 28;
    return 56 + colsH + footerH;
  };

  // Re-project the tree to nodes/edges on every schema change. Persisted drag
  // positions (localStorage) win over the auto-layout, so manual arrangement
  // survives structural edits; new nodes fall back to the tidy layout.
  useEffect(() => {
    const graph = projectGraph(schema);
    const auto = autoLayout(graph.nodes, estimateHeight);
    const overrides = loadOverrides(indexName);
    setNodes(
      graph.nodes.map((n) => ({
        id: n.id,
        type: "doc",
        position: overrides[n.id] ?? auto[n.id] ?? { x: 0, y: 0 },
        data: { node: n },
      })),
    );
    setEdges(
      graph.edges.map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        label: e.label,
      })),
    );
    // `catalog` participates because the height estimate (and so the layout)
    // depends on it; it arrives asynchronously after the first render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schema, indexName, catalog, setNodes, setEdges]);

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
