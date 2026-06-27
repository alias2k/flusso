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
import { autoLayout, loadOverrides, saveOverride } from "../model/layout";
import { projectGraph } from "../model/tree";
import { useDesign } from "../state";
import { DocNodeView } from "./DocNodeView";

const nodeTypes = { doc: DocNodeView };

export function Canvas() {
  const { schema, indexName, select } = useDesign();
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // Re-project the tree to nodes/edges on every schema change. Persisted drag
  // positions (localStorage) win over the auto-layout, so manual arrangement
  // survives structural edits; new nodes fall back to the tidy layout.
  useEffect(() => {
    const graph = projectGraph(schema);
    const auto = autoLayout(graph.nodes);
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
  }, [schema, indexName, setNodes, setEdges]);

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
