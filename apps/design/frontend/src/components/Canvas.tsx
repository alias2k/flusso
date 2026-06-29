import {
  Background,
  Controls,
  type Edge,
  MiniMap,
  type Node,
  Panel,
  ReactFlow,
  useEdgesState,
  useNodesState,
  useReactFlow,
} from "@xyflow/react";
// React Flow's stylesheet is imported (layered) from index.css, not here — a JS
// import is unlayered and would beat our `@layer components` handle overrides.
import { useEffect, useRef, useState } from "react";
import { SCALAR_TYPES } from "../api";
import {
  autoLayout,
  clearOverrides,
  loadOverrides,
  loadViewport,
  saveOverride,
  saveViewport,
} from "../model/layout";
import { suggestRelations } from "../model/relations";
import { type DocNode, projectGraph } from "../model/tree";
import { useT } from "../i18n";
import { useDesign } from "../state";
import { edgeColor } from "../theme";
import { DocNodeView } from "./DocNodeView";
import { Hint } from "./Hint";
import { Text } from "./widgets";
import { Icon } from "./Icon";

const nodeTypes = { doc: DocNodeView };

export function Canvas() {
  const { schema, indexName, select, catalog } = useDesign();
  const { t } = useT();
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [showMap, setShowMap] = useState(false);

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
    setEdges(
      graph.edges.map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        label: e.label,
        animated: true, // flusso = flow: the current runs source → document
        style: { stroke: edgeColor(e.label), strokeWidth: 1.5 },
      })),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [schema, indexName, setNodes, setEdges]);

  // Estimates can't know real heights, so re-run the tidy layout keyed on the
  // *measured* heights themselves: whenever a node's real height changes — the
  // catalog loading (FK suggestions grow the footer), adding/removing a field,
  // switching index — re-tidy with the true heights. Renames/type changes don't
  // change height, so they don't reshuffle. Position-only updates don't change
  // the signature, so this never loops. Manual drags (overrides) still win.
  const laidOut = useRef("");
  useEffect(() => {
    if (!nodes.length || nodes.some((n) => !n.measured?.height)) return; // wait for measurement
    const sig = `${indexName}|${nodes
      .map((n) => `${n.id}:${Math.round(n.measured?.height ?? 0)}`)
      .sort()
      .join(",")}`;
    if (sig === laidOut.current) return;
    laidOut.current = sig;
    const docNodes = nodes.map((n) => (n.data as { node: DocNode }).node);
    const heights = new Map(nodes.map((n) => [n.id, n.measured?.height ?? 0]));
    const auto = autoLayout(docNodes, (dn) => heights.get(dn.id) ?? 300);
    const overrides = loadOverrides(indexName);
    setNodes((current) =>
      current.map((n) => ({ ...n, position: overrides[n.id] ?? auto[n.id] ?? n.position })),
    );
  }, [nodes, indexName, setNodes]);

  // Drop manual positions and re-tidy from the measured heights.
  const resetLayout = () => {
    clearOverrides(indexName);
    const docNodes = nodes.map((n) => (n.data as { node: DocNode }).node);
    const heights = new Map(nodes.map((n) => [n.id, n.measured?.height ?? 0]));
    const auto = autoLayout(docNodes, (dn) => heights.get(dn.id) ?? 300);
    laidOut.current = "";
    setNodes((current) => current.map((n) => ({ ...n, position: auto[n.id] ?? n.position })));
  };

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      onNodeDragStop={(_, node) => saveOverride(indexName, node.id, node.position.x, node.position.y)}
      onMoveEnd={(_, vp) => saveViewport(indexName, vp)}
      onPaneClick={() => select(null)}
      onEdgeClick={(_, edge) => {
        const target = nodes.find((n) => n.id === edge.target);
        if (target) {
          const dn = (target.data as { node: DocNode }).node;
          select(dn.path.length ? { kind: "node", path: dn.path } : { kind: "root" });
        }
      }}
      fitView
      minZoom={0.2}
    >
      <Background />
      <Controls />
      {showMap && <MiniMap pannable zoomable style={{ marginBottom: "2.5rem" }} />}
      <RestoreViewport index={indexName} />
      <Panel position="top-left">
        <NodeSearch />
      </Panel>
      <Panel position="bottom-right">
        <Hint label={t("canvas.resetLayout")} side="left">
          <button className="icon panel-btn" aria-label={t("canvas.resetLayout")} onClick={resetLayout}>
            <Icon name="tidy" />
          </button>
        </Hint>
        <Hint label={showMap ? t("canvas.hideMinimap") : t("canvas.showMinimap")} side="left">
          <button
            className="icon panel-btn map-toggle"
            aria-label={showMap ? t("canvas.hideMinimap") : t("canvas.showMinimap")}
            onClick={() => setShowMap((m) => !m)}
          >
            <Icon name="map" />
          </button>
        </Hint>
      </Panel>
    </ReactFlow>
  );
}

/// Restore each index's pan/zoom on switch (the initial fit is handled by the
/// `fitView` prop, so this skips the first run); edits don't refit.
function RestoreViewport({ index }: { index: string }) {
  const { setViewport, fitView } = useReactFlow();
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const vp = loadViewport(index);
    if (vp) void setViewport(vp);
    else void fitView({ duration: 300 });
  }, [index, setViewport, fitView]);
  return null;
}

/// Type-ahead jump: filter nodes by name/table, click to centre on one and
/// select it. Lives inside <ReactFlow> so it can use the flow instance.
function NodeSearch() {
  const { getNodes, setCenter } = useReactFlow();
  const { select } = useDesign();
  const { t } = useT();
  const [q, setQ] = useState("");

  const label = (n: Node) => {
    const dn = (n.data as { node: DocNode }).node;
    return dn.name ?? dn.table;
  };
  const results = q
    ? getNodes()
        .filter((n) => `${label(n)} ${(n.data as { node: DocNode }).node.table}`.toLowerCase().includes(q.toLowerCase()))
        .slice(0, 8)
    : [];

  const jump = (n: Node) => {
    const w = n.measured?.width ?? 145;
    const h = n.measured?.height ?? 80;
    void setCenter(n.position.x + w / 2, n.position.y + h / 2, { zoom: 1, duration: 300 });
    const dn = (n.data as { node: DocNode }).node;
    select(dn.path.length ? { kind: "node", path: dn.path } : { kind: "root" });
    setQ("");
  };

  return (
    <div className="node-search">
      <Text value={q} onChange={setQ} placeholder={t("canvas.jumpToNode")} />
      {results.length > 0 && (
        <ul>
          {results.map((n) => (
            <li key={n.id} onClick={() => jump(n)}>
              {label(n)}
              <span className="muted"> · {(n.data as { node: DocNode }).node.table}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
