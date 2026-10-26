import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import type { NodeDto } from "./api";
import { pct, ms, rate } from "./format";
import { Glyph, KIND_LABEL } from "./Glyph";
import { CARD_W, COLLAPSED, formatMetric, type CardModel } from "./model";

export interface SvcData extends Record<string, unknown> {
  node: NodeDto;
  model: CardModel;
  expanded: boolean;
  dim: boolean;
}
export type SvcNode = Node<SvcData, "svc">;

// The fill says how bad it is (severity); the tier badge says how much it matters.
function fillFor(n: NodeDto): string {
  return n.impact === "down" ? "var(--bad)" : n.impact === "degraded" ? "var(--warn)" : "var(--node)";
}

const STATUS_TEXT = { none: "OK", degraded: "Degraded", down: "Down" } as const;

export function SvcNodeView({ data, selected }: NodeProps<SvcNode>) {
  const { node, model, expanded, dim } = data;
  const isCause = node.impact !== "none" && node.causes.includes(node.id);
  const cls = ["svc", node.impact, expanded ? "open" : "", selected ? "sel" : "", dim ? "dim" : "", isCause ? "cause" : ""]
    .filter(Boolean)
    .join(" ");
  const style = {
    "--fill": fillFor(node),
    "--stroke": isCause ? "var(--bad)" : node.declared ? "var(--node-border)" : "var(--undeclared)",
  } as React.CSSProperties;
  // Ports sit on the glyph (collapsed) or the card edge (expanded), level with the header.
  const portTop = expanded ? 30 : 28;
  const portInset = expanded ? 0 : (COLLAPSED.w - 56) / 2;

  return (
    <div className={cls} style={{ ...style, width: expanded ? CARD_W : COLLAPSED.w }}>
      <Handle type="target" position={Position.Left} style={{ top: portTop, left: portInset }} />
      <Handle type="source" position={Position.Right} style={{ top: portTop, right: portInset }} />

      {!expanded ? (
        <>
          <div className="glyph-wrap">
            <Glyph kind={node.kind} />
            {node.impact !== "none" && node.tier && (
              <span className={`tierbadge tier-${node.tier}`} title={`Worst affected feature: ${node.tier}`}>
                {node.tier}
              </span>
            )}
          </div>
          <div className="label" title={`${node.id} · ${KIND_LABEL[node.kind]}`}>
            {node.id}
          </div>
        </>
      ) : (
        <div className="card">
          <div className="card-head">
            <Glyph kind={node.kind} size={38} />
            <div className="card-id">
              <div className="card-title">{node.id}</div>
              <div className="card-sub">
                {node.owner ?? "no owner"} · {KIND_LABEL[node.kind]}
              </div>
            </div>
            <span className={`pill ${node.impact}`}>{STATUS_TEXT[node.impact]}</span>
          </div>
          <div className="card-body">
            {model.features.length === 0 && <div className="empty-row">No business features declared</div>}
            {model.features.map(({ feature: f, metrics }) => (
              <div className="feat" key={f.id}>
                <div className="feat-head">
                  <span className={`chip tier-${f.tier}`}>{f.tier}</span>
                  <span className="feat-name" title={f.name}>{f.name}</span>
                  {f.impact !== "none" && <span className={`state ${f.impact}`}>{STATUS_TEXT[f.impact]}</span>}
                </div>
                {metrics.length === 0 && <div className="mrow quiet">no business metrics declared</div>}
                {metrics.map((m) => {
                  const v = formatMetric(m);
                  return (
                    <div className="mrow" key={m.name}>
                      <span className="mname">{m.name}</span>
                      <span className="mdots" />
                      <b title={v.title}>{v.text}</b>
                    </div>
                  );
                })}
              </div>
            ))}
            {model.hasTraffic && (
              <div className="traffic">
                {rate(node.rps)} in · {pct(node.error_ratio)} err · p95 {ms(node.p95_ms)}
              </div>
            )}
            {model.causes.length > 0 && <div className="cause-row">caused by {model.causes.join(", ")}</div>}
          </div>
        </div>
      )}
    </div>
  );
}
