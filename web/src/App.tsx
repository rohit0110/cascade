import { useCallback, useEffect, useMemo, useState } from "react";
import { fetchDrift, fetchFeatures, fetchGraph, fetchMetrics } from "./api";
import type { DriftResp, FeatureDto, GraphResp, MetricDto } from "./api";
import { ago } from "./format";
import { GraphView, type Selection } from "./GraphView";
import { Glyph, KIND_LABEL } from "./Glyph";
import { DriftPanel, EdgeDetail, ImpactList, NodeDetail } from "./Panels";

export function App() {
  const [graph, setGraph] = useState<GraphResp | null>(null);
  const [features, setFeatures] = useState<FeatureDto[]>([]);
  const [metrics, setMetrics] = useState<MetricDto[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [drift, setDrift] = useState<DriftResp | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selection, setSelection] = useState<Selection>(null);
  const [pickedFeature, setPickedFeature] = useState<FeatureDto | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  const refresh = useCallback(async () => {
    try {
      const [g, f, d, m] = await Promise.all([fetchGraph(), fetchFeatures(), fetchDrift(), fetchMetrics()]);
      setGraph(g);
      setFeatures(f.features);
      setMetrics(m.metrics);
      setDrift(d);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);

  useEffect(() => {
    refresh();
    // The server pushes a nudge on every new snapshot; polling is only a fallback.
    const es = new EventSource("/api/events");
    es.addEventListener("snapshot", () => refresh());
    const poll = setInterval(refresh, 30_000);
    const tick = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => {
      es.close();
      clearInterval(poll);
      clearInterval(tick);
    };
  }, [refresh]);

  const highlight = useMemo(() => {
    if (!pickedFeature) return new Set<string>();
    return new Set([...pickedFeature.services, ...pickedFeature.causes]);
  }, [pickedFeature]);

  const age = graph && graph.as_of_unix > 0 ? Math.max(0, now - graph.as_of_unix) : null;
  const stale = graph ? graph.stale && (age === null || age > 45) : false;

  const selectedNode = selection?.kind === "node" ? graph?.nodes.find((n) => n.id === selection.id) : undefined;
  const selectedEdge =
    selection?.kind === "edge"
      ? graph?.edges.find((e) => e.client === selection.client && e.server === selection.server)
      : undefined;

  const impacted = features.filter((f) => f.impact !== "none").length;

  return (
    <div className="app">
      <header>
        <h1>Cascade</h1>
        <span className={`summary ${impacted ? "bad" : "ok"}`}>
          {impacted ? `${impacted} feature${impacted === 1 ? "" : "s"} affected` : "All features healthy"}
        </span>
        <span className="spacer" />
        {graph && (
          <span className={`fresh ${stale ? "stale" : ""}`} title={`source: ${graph.source || "none yet"}`}>
            {age === null ? "waiting for first data" : `data as of ${ago(age)}`}
            {stale && " · STALE"}
          </span>
        )}
      </header>

      {(error || graph?.last_error) && (
        <div className="banner" role="alert">
          {error ? `Cannot reach Cascade: ${error}` : `Last refresh failed: ${graph?.last_error}. Showing the last good data.`}
        </div>
      )}

      <main>
        <section className="canvas" aria-label="Service graph">
          {graph && graph.nodes.length === 0 && (
            <p className="empty">No services yet. Check that the registry has cascade.yaml files and that the topology source has data.</p>
          )}
          <GraphView
            nodes={graph?.nodes ?? []}
            edges={graph?.edges ?? []}
            features={features}
            metrics={metrics}
            expanded={expanded}
            selection={selection}
            highlight={highlight}
            onNodeClick={(id) => {
              // Click expands the node into its card in place; click again collapses it.
              setExpanded((s) => {
                const n = new Set(s);
                if (!n.delete(id)) n.add(id);
                return n;
              });
              setSelection({ kind: "node", id });
              setPickedFeature(null);
            }}
            onEdgeClick={(e) => {
              setSelection({ kind: "edge", ...e });
              setPickedFeature(null);
            }}
            onBackgroundClick={() => {
              setSelection(null);
              setPickedFeature(null);
            }}
          />
          <ul className="legend" aria-label="Legend">
            <li><i className="sw p0" /> P0</li>
            <li><i className="sw p1" /> P1</li>
            <li><i className="sw p2" /> P2</li>
            <li><i className="sw p3" /> P3</li>
            {(["service", "gateway", "database", "cache", "queue", "config"] as const).map((k) => (
              <li key={k}><Glyph kind={k} size={16} /> {KIND_LABEL[k]}</li>
            ))}
            <li><i className="ln bad" /> failing call</li>
            <li><i className="ln soft" /> soft dependency</li>
            <li><i className="ln und" /> undeclared</li>
          </ul>
        </section>

        <aside>
          <h2>Impact</h2>
          <ImpactList
            features={features}
            picked={pickedFeature?.id ?? null}
            onPick={(f) => {
              setPickedFeature((p) => (p?.id === f.id ? null : f));
              setSelection(null);
            }}
          />
          {selectedNode && graph && <NodeDetail node={selectedNode} features={features} edges={graph.edges} />}
          {selectedEdge && <EdgeDetail edge={selectedEdge} />}
          {!selectedNode && !selectedEdge && <p className="quiet hint">Click a service to expand it in the graph, or a call to see its APIs.</p>}
          <DriftPanel data={drift} />
        </aside>
      </main>
    </div>
  );
}
