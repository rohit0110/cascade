import type { DataQuery, DataSourceJsonData } from "@grafana/data";

export type QueryKind = "nodegraph" | "impact";

export interface CascadeQuery extends DataQuery {
  kind: QueryKind;
}

// URL and access mode live in the standard HTTP settings; nothing extra is stored.
export type CascadeOptions = DataSourceJsonData;
