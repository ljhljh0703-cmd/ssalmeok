import { describe, expect, it } from "vitest";
import {
  formatCountdown,
  formatKrw,
  formatPercent,
  formatTokens,
  riskTone,
  sourceLabel,
} from "./format";

describe("usage formatting", () => {
  it("clamps percentages", () => {
    expect(formatPercent(101)).toBe("100%");
    expect(formatPercent(-1)).toBe("0.0%");
    expect(formatPercent(69.3)).toBe("69%");
  });

  it("formats a reset countdown", () => {
    const now = new Date("2026-09-12T04:00:00Z").getTime();
    expect(formatCountdown("2026-09-12T05:31:00Z", now)).toBe("1시간 31분 후");
  });

  it("keeps local source labels understandable", () => {
    expect(sourceLabel("oauth")).toBe("기존 로그인");
    expect(sourceLabel("claude")).toBe("Claude 로컬");
  });

  it("formats large token counts and risk levels", () => {
    expect(formatTokens(1_500_000)).not.toBe("1500000");
    expect(riskTone(4)).toBe("critical");
    expect(riskTone(15)).toBe("watch");
    expect(riskTone(80)).toBe("safe");
    expect(formatKrw(12_345.4)).toBe("₩12,345");
  });
});
