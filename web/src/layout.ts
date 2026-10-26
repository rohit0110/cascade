import dagre from "@dagrejs/dagre";

export interface Box {
  id: string;
  w: number;
  h: number;
}

/**
 * Left-to-right layered layout. Boxes keep their real sizes, so expanding a node
 * pushes its neighbours aside instead of overlapping them. Returns top-left corners.
 */
export function layout(
  boxes: Box[],
  edges: { source: string; target: string }[],
): Map<string, { x: number; y: number }> {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: "LR", nodesep: 40, ranksep: 140, marginx: 24, marginy: 24, acyclicer: "greedy" });
  g.setDefaultEdgeLabel(() => ({}));
  for (const b of boxes) g.setNode(b.id, { width: b.w, height: b.h });
  for (const e of edges) {
    if (e.source !== e.target) g.setEdge(e.source, e.target);
  }
  dagre.layout(g);
  const out = new Map<string, { x: number; y: number }>();
  for (const b of boxes) {
    const n = g.node(b.id);
    out.set(b.id, { x: n.x - b.w / 2, y: n.y - b.h / 2 });
  }
  return out;
}
