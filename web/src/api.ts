export type Impact = "none" | "degraded" | "down";
export type Tier = "P0" | "P1" | "P2" | "P3";
export type NodeKind = "service" | "gateway" | "database" | "cache" | "queue" | "external" | "config";
export type Confidence = "high" | "medium" | "low";

export interface Meta {
  version: number;
  as_of_unix: number;
  age_seconds: number;
  stale: boolean;
  source: string;
  last_error: string | null;
}

export interface NodeDto {
  id: string;
  owner: string | null;
  declared: boolean;
  kind: NodeKind;
  impact: Impact;
  causes: string[];
  tier: Tier | null;
  features: string[];
  rps: number | null;
  error_ratio: number | null;
  p95_ms: number | null;
}

export interface EdgeDto {
  client: string;
  server: string;
  kind: "hard" | "soft";
  origin: "declared" | "observed" | "both";
  confidence: Confidence;
  health: Impact;
  rps: number | null;
  error_ratio: number | null;
  p95_ms: number | null;
}

export interface FeatureDto {
  id: string;
  name: string;
  tier: Tier;
  owner: string;
  services: string[];
  impact: Impact;
  causes: string[];
  confidence: Confidence;
}

export interface MetricDto {
  feature: string;
  service: string;
  name: string;
  unit: string | null;
  value: number | null;
  error: string | null;
}

export interface Drift {
  undeclared: { client: string; server: string }[];
  unobserved: { client: string; server: string }[];
  undeclared_services: string[];
}

export interface ApiStat {
  name: string;
  rps: number;
  error_ratio: number;
  p95_ms: number | null;
}

export type GraphResp = Meta & { nodes: NodeDto[]; edges: EdgeDto[] };
export type FeaturesResp = Meta & { features: FeatureDto[] };
export type MetricsResp = Meta & { metrics: MetricDto[] };
export interface DriftResp {
  drift: Drift;
  registry_warnings: string[];
}

async function get<T>(url: string): Promise<T> {
  const r = await fetch(url);
  if (!r.ok) {
    const body = await r.json().catch(() => ({}));
    throw new Error(body.error ?? `${r.status} ${r.statusText}`);
  }
  return r.json();
}

export const fetchGraph = () => get<GraphResp>("/api/graph");
export const fetchFeatures = () => get<FeaturesResp>("/api/features");
export const fetchMetrics = () => get<MetricsResp>("/api/metrics");
export const fetchDrift = () => get<DriftResp>("/api/drift");
export const fetchApis = (client: string, server: string, q: string) =>
  get<{ apis: ApiStat[]; scope: string }>(
    `/api/edges/${encodeURIComponent(client)}/${encodeURIComponent(server)}/apis?q=${encodeURIComponent(q)}`,
  );
