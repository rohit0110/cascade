import { useEffect, useState } from "react";
import { fetchApis } from "./api";
import type { ApiStat, DriftResp, EdgeDto, FeatureDto, NodeDto } from "./api";
import { ms, pct, rate } from "./format";

export function TierChip({ tier }: { tier: string }) {
  return <span className={`chip tier-${tier}`}>{tier}</span>;
}

export function ImpactList({
  features,
  onPick,
  picked,
}: {
  features: FeatureDto[];
  onPick: (f: FeatureDto) => void;
  picked: string | null;
}) {
  const hit = features.filter((f) => f.impact !== "none");
  if (hit.length === 0) {
    return <p className="quiet">No business feature is affected right now.</p>;
  }
  return (
    <ul className="impact-list">
      {hit.map((f) => (
        <li key={f.id}>
          <button className={`impact ${f.impact} ${picked === f.id ? "picked" : ""}`} onClick={() => onPick(f)}>
            <span className="row">
              <TierChip tier={f.tier} />
              <strong>{f.name}</strong>
              <span className={`state ${f.impact}`}>{f.impact === "down" ? "Down" : "Degraded"}</span>
            </span>
            <span className="meta">
              owner <b>{f.owner}</b> · caused by <b>{f.causes.join(", ") || "unknown"}</b> · confidence {f.confidence}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

export function NodeDetail({ node, features, edges }: { node: NodeDto; features: FeatureDto[]; edges: EdgeDto[] }) {
  const hosted = features.filter((f) => f.services.includes(node.id));
  const calls = edges.filter((e) => e.client === node.id);
  const calledBy = edges.filter((e) => e.server === node.id);
  return (
    <div className="detail">
      <h3>{node.id}</h3>
      <dl>
        <dt>Owner</dt>
        <dd>{node.owner ?? <span className="warn-text">none declared (no cascade.yaml)</span>}</dd>
        <dt>Status</dt>
        <dd>{node.impact === "none" ? "OK" : node.impact}</dd>
        {node.causes.length > 0 && (
          <>
            <dt>Caused by</dt>
            <dd>{node.causes.join(", ")}</dd>
          </>
        )}
        <dt>Inbound</dt>
        <dd>
          {rate(node.rps)} · {pct(node.error_ratio)} errors
        </dd>
      </dl>
      {hosted.length > 0 && (
        <>
          <h4>Features served</h4>
          <ul className="plain">
            {hosted.map((f) => (
              <li key={f.id}>
                <TierChip tier={f.tier} /> {f.name} <span className="quiet">({f.impact === "none" ? "ok" : f.impact})</span>
              </li>
            ))}
          </ul>
        </>
      )}
      <h4>Calls ({calls.length})</h4>
      <p className="quiet">{calls.map((e) => `${e.server}${e.kind === "soft" ? " (soft)" : ""}`).join(", ") || "nothing"}</p>
      <h4>Called by ({calledBy.length})</h4>
      <p className="quiet">{calledBy.map((e) => e.client).join(", ") || "nothing"}</p>
    </div>
  );
}

export function EdgeDetail({ edge }: { edge: EdgeDto }) {
  const [q, setQ] = useState("");
  const [apis, setApis] = useState<ApiStat[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    setApis(null);
    setErr(null);
    const t = setTimeout(() => {
      fetchApis(edge.client, edge.server, q)
        .then((r) => setApis(r.apis))
        .catch((e: Error) => setErr(e.message));
    }, 200);
    return () => clearTimeout(t);
  }, [edge.client, edge.server, q]);

  return (
    <div className="detail">
      <h3>
        {edge.client} → {edge.server}
      </h3>
      <dl>
        <dt>Health</dt>
        <dd className={`state ${edge.health}`}>{edge.health === "none" ? "OK" : edge.health}</dd>
        <dt>Dependency</dt>
        <dd>
          {edge.kind} · {edge.origin === "both" ? "declared and observed" : edge.origin} · confidence {edge.confidence}
        </dd>
        <dt>Traffic</dt>
        <dd>
          {rate(edge.rps)} · {pct(edge.error_ratio)} errors · p95 {ms(edge.p95_ms)}
        </dd>
      </dl>
      <h4>APIs on {edge.server}</h4>
      <input
        type="search"
        placeholder="Search APIs…"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        aria-label="Search APIs"
      />
      <p className="quiet small">Inbound APIs of {edge.server} across all callers; the service graph has no per-route data.</p>
      {err && <p className="warn-text">Could not load APIs: {err}</p>}
      {apis && apis.length === 0 && <p className="quiet">No matching APIs seen in the window.</p>}
      {apis && apis.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>API</th>
              <th>Rate</th>
              <th>Err</th>
              <th>p95</th>
            </tr>
          </thead>
          <tbody>
            {apis.map((a) => (
              <tr key={a.name} className={a.error_ratio >= 0.05 ? "bad-row" : ""}>
                <td className="api-name">{a.name}</td>
                <td>{rate(a.rps)}</td>
                <td>{pct(a.error_ratio)}</td>
                <td>{ms(a.p95_ms)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

export function DriftPanel({ data }: { data: DriftResp | null }) {
  if (!data) return null;
  const { drift, registry_warnings } = data;
  const n = drift.undeclared.length + drift.unobserved.length + drift.undeclared_services.length + registry_warnings.length;
  return (
    <details className="drift">
      <summary>
        Registry health {n === 0 ? "· clean" : <span className="badge">{n}</span>}
      </summary>
      {drift.undeclared.length > 0 && (
        <>
          <h4>Seen in traffic, not declared</h4>
          <ul className="plain">
            {drift.undeclared.map((e) => (
              <li key={e.client + e.server}>
                {e.client} → {e.server}
              </li>
            ))}
          </ul>
        </>
      )}
      {drift.unobserved.length > 0 && (
        <>
          <h4>Declared, not seen recently</h4>
          <ul className="plain">
            {drift.unobserved.map((e) => (
              <li key={e.client + e.server}>
                {e.client} → {e.server}
              </li>
            ))}
          </ul>
        </>
      )}
      {drift.undeclared_services.length > 0 && (
        <>
          <h4>Calling services with no cascade.yaml</h4>
          <p className="quiet">{drift.undeclared_services.join(", ")}</p>
        </>
      )}
      {registry_warnings.length > 0 && (
        <>
          <h4>Registry warnings</h4>
          <ul className="plain">
            {registry_warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </>
      )}
    </details>
  );
}
