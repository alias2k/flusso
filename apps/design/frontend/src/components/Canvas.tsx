import {
  Background,
  type Edge,
  MiniMap,
  type Node,
  Panel,
  ReactFlow,
  useEdgesState,
  useNodes,
  useNodesState,
  useReactFlow,
} from "@xyflow/react";
// React Flow's stylesheet is imported (layered) from index.css, not here — a JS
// import is unlayered and would beat our `@layer components` handle overrides.
import { useEffect, useRef, useState } from "react";
import { ChevronsDownUp, ChevronsUpDown, Lock, Maximize2, Unlock, ZoomIn, ZoomOut } from "lucide-react";
import { SCALAR_TYPES } from "../api";
import { autoLayout, clearOverrides, loadOverrides, loadViewport, saveOverride, saveViewport } from "../model/layout";
import { suggestRelations } from "../model/relations";
import { type DocNode, projectGraph } from "../model/tree";
import { useT } from "../i18n";
import { useDesign } from "../state";
import { useDesignStore } from "../store/design";
import { edgeColor } from "../theme";
import { Button } from "@/components/ui/button";
import { DocNodeView } from "./DocNodeView";
import { Hint } from "./Hint";
import { Icon } from "./Icon";

const nodeTypes = { doc: DocNodeView };

export function Canvas() {
  const { schema, indexName, select, catalog } = useDesign();
  const { t } = useT();
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [showMap, setShowMap] = useState(false);
  const [locked, setLocked] = useState(false);

  // Estimate a node's rendered height so the auto-layout reserves the right
  // vertical band (header + column rows, capped by the scroll area, + footer).
  // The footer is a fixed height: the add-menu row, plus one row for the
  // suggestion picker when the table has any FK suggestions (they collapse into
  // a single trigger, so its size no longer tracks the suggestion count).
  const estimateHeight = (n: DocNode): number => {
    const tableCols = catalog?.catalog.tables.find((t) => t.name === n.table)?.columns ?? [];
    const catalogNames = new Set(tableCols.map((c) => c.name));
    const special = n.leaves.filter(
      (l) => !((SCALAR_TYPES as string[]).includes(l.kind) && l.column && catalogNames.has(l.column)),
    ).length;
    const rows = (tableCols.length || n.leaves.length) + special;
    const colsH = Math.min(rows * 36 + 12, 280);
    const hasSuggestions = catalog ? suggestRelations(catalog, n.table).length > 0 : false;
    const footerH = 76 + (hasSuggestions ? 34 : 0);
    return 64 + colsH + footerH;
  };

  // Re-project the tree to nodes/edges on every schema change, but *keep each
  // existing node's current position* — only brand-new nodes get an estimated
  // slot. So a field edit never moves anything, and this never clobbers the
  // measured layout (below) or a manual drag. Deliberately NOT keyed on
  // `catalog`: it arrives async, and re-running here would reset positions.
  const pruneCollapsed = useDesignStore((s) => s.pruneCollapsed);
  useEffect(() => {
    const graph = projectGraph(schema);
    pruneCollapsed(graph.nodes.map((n) => n.id));
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
  // catalog loading (its columns fill in the rows), adding/removing a field,
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
    setNodes((current) => current.map((n) => ({ ...n, position: overrides[n.id] ?? auto[n.id] ?? n.position })));
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
      nodesDraggable={!locked}
    >
      <Background />
      <FlowControls locked={locked} onToggle={() => setLocked((l) => !l)} />
      {showMap && <MiniMap pannable zoomable style={{ marginBottom: "2.5rem" }} />}
      <RestoreViewport index={indexName} />
      <FocusOnRequest />
      <Panel position="bottom-right">
        <div className="flex gap-1.5">
          <Hint label={t("canvas.resetLayout")} side="left">
            <Button variant="secondary" size="icon-sm" aria-label={t("canvas.resetLayout")} onClick={resetLayout}>
              <Icon name="tidy" />
            </Button>
          </Hint>
          <Hint label={showMap ? t("canvas.hideMinimap") : t("canvas.showMinimap")} side="left">
            <Button
              variant="secondary"
              size="icon-sm"
              aria-label={showMap ? t("canvas.hideMinimap") : t("canvas.showMinimap")}
              onClick={() => setShowMap((m) => !m)}
            >
              <Icon name="map" />
            </Button>
          </Hint>
        </div>
      </Panel>
    </ReactFlow>
  );
}

/// Canvas controls (zoom / fit / lock) as our shadcn icon buttons, replacing
/// React Flow's built-in `<Controls>`. Lives inside `<ReactFlow>` for the zoom
/// hooks; `locked` (lifted to Canvas) freezes node dragging.
function FlowControls({ locked, onToggle }: { locked: boolean; onToggle: () => void }) {
  const { t } = useT();
  const { zoomIn, zoomOut, fitView } = useReactFlow();
  const collapseAll = useDesignStore((s) => s.collapseAll);
  const expandAll = useDesignStore((s) => s.expandAll);
  return (
    <Panel position="bottom-left">
      <div className="flex flex-col gap-1.5">
        <Hint label={t("canvas.collapseAll")} side="right">
          <Button variant="secondary" size="icon-sm" aria-label={t("canvas.collapseAll")} onClick={() => collapseAll()}>
            <ChevronsDownUp />
          </Button>
        </Hint>
        <Hint label={t("canvas.expandAll")} side="right">
          <Button variant="secondary" size="icon-sm" aria-label={t("canvas.expandAll")} onClick={() => expandAll()}>
            <ChevronsUpDown />
          </Button>
        </Hint>
        <Hint label={t("canvas.zoomIn")} side="right">
          <Button variant="secondary" size="icon-sm" aria-label={t("canvas.zoomIn")} onClick={() => void zoomIn()}>
            <ZoomIn />
          </Button>
        </Hint>
        <Hint label={t("canvas.zoomOut")} side="right">
          <Button variant="secondary" size="icon-sm" aria-label={t("canvas.zoomOut")} onClick={() => void zoomOut()}>
            <ZoomOut />
          </Button>
        </Hint>
        <Hint label={t("canvas.fitView")} side="right">
          <Button variant="secondary" size="icon-sm" aria-label={t("canvas.fitView")} onClick={() => void fitView()}>
            <Maximize2 />
          </Button>
        </Hint>
        <Hint label={locked ? t("canvas.unlock") : t("canvas.lock")} side="right">
          <Button
            variant="secondary"
            size="icon-sm"
            aria-label={locked ? t("canvas.unlock") : t("canvas.lock")}
            onClick={onToggle}
          >
            {locked ? <Lock /> : <Unlock />}
          </Button>
        </Hint>
      </div>
    </Panel>
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

/// Consumes a pending focus request from the store (set by the global command
/// palette, which lives outside React Flow): pans/zooms the canvas to the
/// requested node, expanding it if collapsed, then clears the request. Waits for
/// the node to be projected (e.g. after switching index) before acting. Lives
/// inside <ReactFlow> for the flow instance.
function FocusOnRequest() {
  const { getNode, fitView } = useReactFlow();
  const nodes = useNodes();
  const { indexName, collapsed, toggleCollapsed } = useDesign();
  const focus = useDesignStore((s) => s.focus);
  const clearFocus = useDesignStore((s) => s.clearFocus);
  useEffect(() => {
    if (!focus) return;
    if (focus.index !== indexName) return;
    if (!getNode(focus.nodeId)) return; // not projected yet — re-runs when `nodes` changes
    if (collapsed.has(focus.nodeId)) toggleCollapsed(focus.nodeId);
    void fitView({ nodes: [{ id: focus.nodeId }], duration: 300, maxZoom: 1 });
    clearFocus();
  }, [focus, indexName, nodes, getNode, fitView, collapsed, toggleCollapsed, clearFocus]);
  return null;
}
