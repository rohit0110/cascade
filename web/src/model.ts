import type { FeatureDto, MetricDto, NodeDto } from "./api";

// Layout constants. The card CSS uses the same numbers so that the layout can
// reserve the right amount of room before the DOM is measured.
export const COLLAPSED = { w: 120, h: 96 };
export const CARD_W = 300;
const HEAD = 60;
const BODY_PAD = 16;
const FEAT_HEAD = 28;
const METRIC_ROW = 22;
const FEAT_GAP = 6;
const EMPTY_ROW = 26;
const TRAFFIC_ROW = 30;
const CAUSE_ROW = 26;

export interface CardFeature {
  feature: FeatureDto;
  metrics: MetricDto[];
}

export interface CardModel {
  features: CardFeature[];
  hasTraffic: boolean;
  causes: string[];
}

export function cardModel(node: NodeDto, features: FeatureDto[], metrics: MetricDto[]): CardModel {
  return {
    features: features
      .filter((f) => f.services.includes(node.id))
      .map((feature) => ({ feature, metrics: metrics.filter((m) => m.feature === feature.id) })),
    hasTraffic: node.rps != null,
    causes: node.impact !== "none" ? node.causes.filter((c) => c !== node.id) : [],
  };
}

export function cardHeight(m: CardModel): number {
  const feats =
    m.features.length === 0
      ? EMPTY_ROW
      : m.features.reduce((h, f) => h + FEAT_HEAD + Math.max(1, f.metrics.length) * METRIC_ROW + FEAT_GAP, 0);
  return HEAD + BODY_PAD + feats + (m.hasTraffic ? TRAFFIC_ROW : 0) + (m.causes.length ? CAUSE_ROW : 0);
}

const nf = (v: number) => (Math.abs(v) >= 100 ? Math.round(v).toString() : Math.abs(v) >= 10 ? v.toFixed(1) : v.toFixed(2));

export function formatMetric(m: MetricDto): { text: string; title?: string } {
  if (m.value == null) return { text: "–", title: m.error ?? "no data" };
  return { text: `${nf(m.value)}${m.unit ? ` ${m.unit}` : ""}` };
}
