import { describe, expect, it } from "vitest";
import type { FeatureDto, MetricDto, NodeDto } from "./api";
import { cardHeight, cardModel, COLLAPSED, formatMetric } from "./model";
import { layout } from "./layout";

const node = (o: Partial<NodeDto> = {}): NodeDto => ({
  id: "payment", owner: "pay", declared: true, kind: "service", impact: "none", causes: [], tier: null,
  features: [], rps: null, error_ratio: null, p95_ms: null, ...o,
});
const feat = (id: string, services: string[]): FeatureDto => ({
  id, name: id, tier: "P0", owner: "pay", services, impact: "none", causes: [], confidence: "high",
});
const metric = (feature: string, o: Partial<MetricDto> = {}): MetricDto => ({
  feature, service: "payment", name: "tx", unit: "/min", value: 12.345, error: null, ...o,
});

describe("cardModel", () => {
  it("groups metrics under the node's own features only", () => {
    const m = cardModel(node(), [feat("a", ["payment"]), feat("b", ["other"])], [metric("a"), metric("b")]);
    expect(m.features.map((f) => f.feature.id)).toEqual(["a"]);
    expect(m.features[0].metrics).toHaveLength(1);
  });
  it("excludes the node itself from its causes", () => {
    const m = cardModel(node({ impact: "down", causes: ["payment", "db"] }), [], []);
    expect(m.causes).toEqual(["db"]);
    expect(cardModel(node({ impact: "none", causes: ["db"] }), [], []).causes).toEqual([]);
  });
  it("grows with features, metrics, traffic and causes", () => {
    const empty = cardHeight(cardModel(node(), [], []));
    const one = cardHeight(cardModel(node(), [feat("a", ["payment"])], [metric("a")]));
    const two = cardHeight(cardModel(node(), [feat("a", ["payment"])], [metric("a"), metric("a")]));
    const traffic = cardHeight(cardModel(node({ rps: 1 }), [feat("a", ["payment"])], [metric("a")]));
    expect(one).toBeGreaterThan(empty);
    expect(two).toBeGreaterThan(one);
    expect(traffic).toBeGreaterThan(one);
    expect(one).toBeGreaterThan(COLLAPSED.h);
  });
});

describe("formatMetric", () => {
  it("formats values and explains missing ones", () => {
    expect(formatMetric(metric("a")).text).toBe("12.3 /min");
    expect(formatMetric(metric("a", { value: 1234.5 })).text).toBe("1235 /min");
    expect(formatMetric(metric("a", { value: 0.5, unit: null })).text).toBe("0.50");
    expect(formatMetric(metric("a", { value: null, error: "no query declared" }))).toEqual({
      text: "–",
      title: "no query declared",
    });
  });
});

describe("layout", () => {
  const boxes = [
    { id: "a", w: 120, h: 96 },
    { id: "b", w: 120, h: 96 },
    { id: "c", w: 300, h: 260 },
  ];
  it("flows left to right and never overlaps boxes", () => {
    const pos = layout(boxes, [{ source: "a", target: "b" }, { source: "a", target: "c" }]);
    expect(pos.get("a")!.x).toBeLessThan(pos.get("b")!.x);
    for (const p of boxes) {
      for (const q of boxes) {
        if (p.id >= q.id) continue;
        const a = pos.get(p.id)!, b = pos.get(q.id)!;
        const apart = a.x + p.w <= b.x || b.x + q.w <= a.x || a.y + p.h <= b.y || b.y + q.h <= a.y;
        expect(apart, `${p.id} overlaps ${q.id}`).toBe(true);
      }
    }
  });
  it("survives cycles and self-loops", () => {
    const pos = layout(boxes, [
      { source: "a", target: "b" }, { source: "b", target: "c" }, { source: "c", target: "a" }, { source: "a", target: "a" },
    ]);
    expect(pos.size).toBe(3);
  });
});
