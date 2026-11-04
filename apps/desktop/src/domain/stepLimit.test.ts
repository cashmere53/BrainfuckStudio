import { describe, expect, it } from "vitest";
import { DEFAULT_MAX_STEPS, MAX_STEP_LIMIT } from "./limits";
import { DEFAULT_STEP_LIMIT_TEXT, parseStepLimit, resolveStepLimit } from "./stepLimit";

describe("parseStepLimit: accepted values", () => {
  it("accepts the lower bound 1", () => {
    expect(parseStepLimit("1")).toEqual({ ok: true, limit: { kind: "limited", maxSteps: 1 } });
  });

  it("accepts 2^53 - 1 (the upper bound)", () => {
    expect(parseStepLimit("9007199254740991")).toEqual({
      ok: true,
      limit: { kind: "limited", maxSteps: MAX_STEP_LIMIT },
    });
  });

  it("accepts the default 10,000,000 used as the initial value", () => {
    expect(parseStepLimit(DEFAULT_STEP_LIMIT_TEXT)).toEqual({
      ok: true,
      limit: { kind: "limited", maxSteps: DEFAULT_MAX_STEPS },
    });
    expect(DEFAULT_STEP_LIMIT_TEXT).toBe("10000000");
  });

  it("reads leading zeros as the same decimal value", () => {
    expect(parseStepLimit("0042")).toEqual({ ok: true, limit: { kind: "limited", maxSteps: 42 } });
  });
});

describe("parseStepLimit: rejected values", () => {
  it("rejects 0 as belowMin", () => {
    expect(parseStepLimit("0")).toEqual({ ok: false, reason: "belowMin" });
  });

  it("rejects 000 as belowMin", () => {
    expect(parseStepLimit("000")).toEqual({ ok: false, reason: "belowMin" });
  });

  it("rejects a negative number as belowMin", () => {
    expect(parseStepLimit("-1")).toEqual({ ok: false, reason: "belowMin" });
  });

  it("rejects 2^53 as aboveMax", () => {
    expect(parseStepLimit("9007199254740992")).toEqual({ ok: false, reason: "aboveMax" });
  });

  it("rejects a value far above 2^53 as aboveMax without losing precision", () => {
    expect(parseStepLimit("99999999999999999999999")).toEqual({ ok: false, reason: "aboveMax" });
  });

  it("rejects a decimal as notInteger", () => {
    expect(parseStepLimit("1.5")).toEqual({ ok: false, reason: "notInteger" });
  });

  it("rejects an empty string as empty", () => {
    expect(parseStepLimit("")).toEqual({ ok: false, reason: "empty" });
  });

  it("rejects non-digit text as notInteger", () => {
    expect(parseStepLimit("abc")).toEqual({ ok: false, reason: "notInteger" });
    expect(parseStepLimit("1e3")).toEqual({ ok: false, reason: "notInteger" });
    expect(parseStepLimit("0x10")).toEqual({ ok: false, reason: "notInteger" });
    expect(parseStepLimit("１０")).toEqual({ ok: false, reason: "notInteger" });
  });

  it("rejects surrounding whitespace as notInteger", () => {
    expect(parseStepLimit(" 10")).toEqual({ ok: false, reason: "notInteger" });
    expect(parseStepLimit("   ")).toEqual({ ok: false, reason: "notInteger" });
  });
});

describe("resolveStepLimit: 'unlimited' is a separate choice", () => {
  it("returns unlimited without looking at the text when the option is selected", () => {
    expect(resolveStepLimit(true, "abc")).toEqual({ ok: true, limit: { kind: "unlimited" } });
  });

  it("parses the text when the option is not selected", () => {
    expect(resolveStepLimit(false, "5")).toEqual({ ok: true, limit: { kind: "limited", maxSteps: 5 } });
    expect(resolveStepLimit(false, "0")).toEqual({ ok: false, reason: "belowMin" });
  });
});
