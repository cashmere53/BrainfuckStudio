import { describe, expect, it } from "vitest";
import * as limits from "./limits";

describe("shared limits", () => {
  it("matches the documented values (1 MiB, 64 KiB, 2^53-1, radius 64, batch 1,000..1,000,000)", () => {
    expect(limits.MAX_SOURCE_BYTES).toBe(1_048_576);
    expect(limits.MAX_INPUT_BYTES).toBe(1_048_576);
    expect(limits.OUTPUT_TAIL_LIMIT).toBe(65_536);
    expect(limits.MAX_STEP_LIMIT).toBe(Number.MAX_SAFE_INTEGER);
    expect(limits.MAX_TAPE_RADIUS).toBe(64);
    expect(limits.DEFAULT_TAPE_RADIUS).toBe(16);
    expect(limits.MIN_BATCH_SIZE).toBe(1_000);
    expect(limits.DEFAULT_BATCH_SIZE).toBe(1_000);
    expect(limits.MAX_BATCH_SIZE).toBe(1_000_000);
    expect(limits.DEFAULT_MAX_STEPS).toBe(10_000_000);
  });
});
