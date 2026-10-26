import { describe, expect, it } from "vitest";
import { ago, ms, pct, rate } from "./format";

describe("format", () => {
  it("formats ages", () => {
    expect(ago(5)).toBe("5s ago");
    expect(ago(300)).toBe("5m ago");
    expect(ago(7200)).toBe("2h ago");
  });
  it("formats ratios and rates", () => {
    expect(pct(null)).toBe("–");
    expect(pct(0)).toBe("0%");
    expect(pct(0.0004)).toBe("<0.1%");
    expect(pct(0.05)).toBe("5.0%");
    expect(pct(1)).toBe("100%");
    expect(rate(0.5)).toBe("0.50/s");
    expect(rate(120.4)).toBe("120/s");
    expect(ms(250)).toBe("250 ms");
    expect(ms(2500)).toBe("2.5 s");
  });
});
