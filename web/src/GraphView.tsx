import {
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useEffect, useMemo, useRef } from "react";
import type { EdgeDto, FeatureDto, MetricDto, NodeDto } from "./api";
import { pct } from "./format";
import { layout } from "./layout";
import { CARD_W, cardHeight, cardModel, COLLAPSED } from "./model";
import { SvcNodeView, type SvcNode } from "./SvcNode";

export type Selection = { kind: "node"; id: string } | { kind: "edge"; client: string; server: string } | null;

const nodeTypes = { svc: SvcNodeView };

function edgeStyle(e: EdgeDto, selected: boolean, dim: boolean) {
  const failing = e.health !== "none";
  const stroke = selected
    ? "var(--accent)"
    : e.health === "down"
      ? "var(--bad)"
      : e.health === "degraded"
        ? "var(--warn)"
        : e.origin === "observed"
          ? "var(--undeclared)"
          : "var(--edge)";
  return {
    stroke,
    strokeWidth: selected ? 3.5 : failing ? 3 : 1.6,
    strokeDasharray: e.kind === "soft" ? "7 5" : e.origin === "declared" ? "2 5" : undefined,
    // Healthy edges stay quiet; declared-only (unconfirmed) edges fade further.
    opacity: dim ? 0.12 : failing || selected ? 1 : e.origin === "declared" ? 0.4 : 0.7,
  };
}

interface Props {
  nodes: NodeDto[];
  edges: EdgeDto[];
  features: FeatureDto[];
  metrics: MetricDto[];
  expanded: Set<string>;
  selection: Selection;
  highlight: Set<string>;
  onNodeClick: (id: string) => void;
  onEdgeClick: (e: { client: string; server: string }) => void;
  onBackgroundClick: () => void;
}

function Inner(p: Props) {
  const rf = useReactFlow();
  const structure = useRef("");
  const lastExpanded = useRef<Set<string>>(new Set());
  const fitTimers = useRef<ReturnType<typeof setTimeout>[]>([]);

  const flowNodes: SvcNode[] = useMemo(() => {
    const models = new Map(p.nodes.map((n) => [n.id, cardModel(n, p.features, p.metrics)]));
    const boxes = p.nodes.map((n) => {
      const open = p.expanded.has(n.id);
      return {
        id: n.id,
        w: open ? CARD_W : COLLAPSED.w,
        h: open ? cardHeight(models.get(n.id)!) : COLLAPSED.h,
      };
    });
    const pos = layout(boxes, p.edges.map((e) => ({ source: e.client, target: e.server })));
    return p.nodes.map((n) => ({
      id: n.id,
      type: "svc" as const,
      position: pos.get(n.id) ?? { x: 0, y: 0 },
      draggable: false,
      selected: p.selection?.kind === "node" && p.selection.id === n.id,
      zIndex: p.expanded.has(n.id) ? 10 : 0,
      data: {
        node: n,
        model: models.get(n.id)!,
        expanded: p.expanded.has(n.id),
        dim: p.highlight.size > 0 && !p.highlight.has(n.id),
      },
    }));
  }, [p.nodes, p.edges, p.features, p.metrics, p.expanded, p.selection, p.highlight]);

  const flowEdges: Edge[] = useMemo(
    () =>
      p.edges.map((e) => {
        const selected = p.selection?.kind === "edge" && p.selection.client === e.client && p.selection.server === e.server;
        const dim = p.highlight.size > 0 && !(p.highlight.has(e.client) && p.highlight.has(e.server));
        const style = edgeStyle(e, selected, dim);
        return {
          id: `${e.client}>${e.server}`,
          source: e.client,
          target: e.server,
          type: "default", // cubic bezier: the S-curves
          animated: e.health !== "none",
          style,
          interactionWidth: 22,
          markerEnd: { type: MarkerType.ArrowClosed, color: style.stroke, width: 16, height: 16 },
          label: e.health !== "none" ? `${e.kind === "soft" ? "soft · " : ""}${pct(e.error_ratio)} err` : undefined,
          labelStyle: { fill: "var(--text)", fontSize: 11, fontWeight: 600 },
          labelBgStyle: { fill: "var(--bg)", fillOpacity: 0.9 },
          labelBgPadding: [4, 2] as [number, number],
          data: { client: e.client, server: e.server },
        };
      }),
    [p.edges, p.selection, p.highlight],
  );

  // Refit when the set of services/edges changes. React Flow measures node sizes a frame or
  // two after they appear, and `useNodesInitialized` can still report true from the previous
  // (empty) graph, so fit after a short delay, then once more when measuring has settled.
  useEffect(() => {
    const key = p.nodes.map((n) => n.id).join(",") + "|" + p.edges.map((e) => `${e.client}>${e.server}`).join(",");
    if (key === structure.current || p.nodes.length === 0) return;
    structure.current = key;
    const fit = () => rf.fitView({ padding: 0.15, duration: 250 });
    // Not cleared when this effect re-runs on a data refresh, or the fit would never fire.
    fitTimers.current.forEach(clearTimeout);
    fitTimers.current = [setTimeout(fit, 150), setTimeout(fit, 700)];
  }, [p.nodes, p.edges, rf]);
  useEffect(() => () => fitTimers.current.forEach(clearTimeout), []);

  useEffect(() => {
    const added = [...p.expanded].filter((id) => !lastExpanded.current.has(id));
    lastExpanded.current = new Set(p.expanded);
    if (added.length > 0) {
      // Let the relayout land first, then bring the new card into view.
      setTimeout(() => rf.fitView({ nodes: [{ id: added[0] }], padding: 0.5, maxZoom: 1.1, duration: 400 }), 60);
    }
  }, [p.expanded, rf]);

  return (
    <ReactFlow
      nodes={flowNodes}
      edges={flowEdges}
      nodeTypes={nodeTypes}
      colorMode="system"
      minZoom={0.15}
      maxZoom={2}
      nodesDraggable={false}
      nodesConnectable={false}
      elementsSelectable
      proOptions={{ hideAttribution: true }}
      onNodeClick={(_, n) => p.onNodeClick(n.id)}
      onEdgeClick={(_, e) => p.onEdgeClick({ client: String(e.source), server: String(e.target) })}
      onPaneClick={p.onBackgroundClick}
    >
      <Background variant={BackgroundVariant.Dots} gap={22} size={1.4} color="var(--dots)" />
      <Controls showInteractive={false} />
    </ReactFlow>
  );
}

export function GraphView(p: Props) {
  return (
    <div className="graph" role="application" aria-label="Service dependency graph">
      <ReactFlowProvider>
        <Inner {...p} />
      </ReactFlowProvider>
    </div>
  );
}
