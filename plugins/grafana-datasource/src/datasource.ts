import {
  DataQueryRequest,
  DataQueryResponse,
  DataSourceApi,
  DataSourceInstanceSettings,
  FieldType,
  MutableDataFrame,
} from "@grafana/data";
import { getBackendSrv } from "@grafana/runtime";
import { lastValueFrom } from "rxjs";
import type { CascadeOptions, CascadeQuery } from "./types";

interface NodeGraphResp {
  nodes: Record<string, string | number>[];
  edges: Record<string, string | number>[];
}
interface FeaturesResp {
  features: { id: string; name: string; tier: string; owner: string; impact: string; causes: string[]; confidence: string }[];
}

export class CascadeDataSource extends DataSourceApi<CascadeQuery, CascadeOptions> {
  constructor(private settings: DataSourceInstanceSettings<CascadeOptions>) {
    super(settings);
  }

  // Requests go through Grafana's datasource proxy, so the Cascade URL stays server-side.
  private async get<T>(path: string): Promise<T> {
    const res = await lastValueFrom(
      getBackendSrv().fetch<T>({ url: `/api/datasources/proxy/uid/${this.settings.uid}${path}` }),
    );
    return res.data;
  }

  async query(req: DataQueryRequest<CascadeQuery>): Promise<DataQueryResponse> {
    const data: MutableDataFrame[] = [];
    for (const t of req.targets.filter((t) => !t.hide)) {
      if ((t.kind ?? "nodegraph") === "nodegraph") {
        const g = await this.get<NodeGraphResp>("/api/grafana/nodegraph");
        const nodes = new MutableDataFrame({
          refId: t.refId,
          name: "nodes",
          meta: { preferredVisualisationType: "nodeGraph" },
          fields: [
            { name: "id", type: FieldType.string, values: g.nodes.map((n) => n.id) },
            { name: "title", type: FieldType.string, values: g.nodes.map((n) => n.title) },
            { name: "subTitle", type: FieldType.string, values: g.nodes.map((n) => n.subTitle) },
            { name: "mainStat", type: FieldType.string, values: g.nodes.map((n) => n.mainStat) },
            { name: "secondaryStat", type: FieldType.string, values: g.nodes.map((n) => n.secondaryStat) },
            { name: "arc__failed", type: FieldType.number, config: { color: { mode: "fixed", fixedColor: "red" } }, values: g.nodes.map((n) => n.arc__failed) },
            { name: "arc__ok", type: FieldType.number, config: { color: { mode: "fixed", fixedColor: "green" } }, values: g.nodes.map((n) => n.arc__ok) },
          ],
        });
        const edges = new MutableDataFrame({
          refId: t.refId,
          name: "edges",
          meta: { preferredVisualisationType: "nodeGraph" },
          fields: [
            { name: "id", type: FieldType.string, values: g.edges.map((e) => e.id) },
            { name: "source", type: FieldType.string, values: g.edges.map((e) => e.source) },
            { name: "target", type: FieldType.string, values: g.edges.map((e) => e.target) },
            { name: "mainStat", type: FieldType.string, values: g.edges.map((e) => e.mainStat) },
            { name: "detail__health", type: FieldType.string, values: g.edges.map((e) => e.detail__health) },
            { name: "detail__kind", type: FieldType.string, values: g.edges.map((e) => e.detail__kind) },
          ],
        });
        data.push(nodes, edges);
      } else {
        const r = await this.get<FeaturesResp>("/api/features");
        data.push(
          new MutableDataFrame({
            refId: t.refId,
            name: "impact",
            fields: [
              { name: "feature", type: FieldType.string, values: r.features.map((f) => f.name) },
              { name: "tier", type: FieldType.string, values: r.features.map((f) => f.tier) },
              { name: "owner", type: FieldType.string, values: r.features.map((f) => f.owner) },
              { name: "impact", type: FieldType.string, values: r.features.map((f) => f.impact) },
              { name: "caused by", type: FieldType.string, values: r.features.map((f) => f.causes.join(", ")) },
              { name: "confidence", type: FieldType.string, values: r.features.map((f) => f.confidence) },
            ],
          }),
        );
      }
    }
    return { data };
  }

  async testDatasource() {
    try {
      await this.get("/api/graph");
      return { status: "success", message: "Connected to Cascade" };
    } catch (e) {
      return { status: "error", message: `Cannot reach Cascade: ${(e as Error).message}` };
    }
  }
}
