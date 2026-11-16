import { describe, expect, it } from "vitest";
import type { AppConfig } from "./contracts";
import { estimateReading, readerTokens } from "./estimate";

const cfg = (pricing: AppConfig["pricing"]) =>
  ({
    roles: [
      { id: "a", model: "m1" },
      { id: "chief", model: "m1" },
    ],
    pricing,
  }) as unknown as AppConfig;

describe("estimateReading", () => {
  it("khớp mốc của plan cho trải 3 lá", () => {
    const t = readerTokens(3);
    expect(t.input).toBeGreaterThan(1400);
    expect(t.input).toBeLessThan(1800);
    expect(t.output).toBeGreaterThan(500);
    expect(t.output).toBeLessThan(700);
  });

  it("chi phí theo bảng giá; thiếu giá → null", () => {
    const e = estimateReading(cfg({ m1: { input_per_m: 1, cached_per_m: 0, output_per_m: 2 } }), 3, ["a"], "chief");
    expect(e.tokens).toBeGreaterThan(3000);
    expect(e.costUsd).toBeGreaterThan(0);
    expect(estimateReading(cfg({}), 3, ["a"], "chief").costUsd).toBeNull();
  });
});
