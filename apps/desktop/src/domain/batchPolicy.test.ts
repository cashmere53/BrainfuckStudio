import { describe, expect, it } from "vitest";
import { DEFAULT_BATCH_SIZE, MAX_BATCH_SIZE, MIN_BATCH_SIZE } from "./limits";
import { nextBatchSize } from "./batchPolicy";

describe("batchPolicy constants", () => {
  it("starts at 1,000 and is bounded to 1,000..1,000,000", () => {
    expect(DEFAULT_BATCH_SIZE).toBe(1_000);
    expect(MIN_BATCH_SIZE).toBe(1_000);
    expect(MAX_BATCH_SIZE).toBe(1_000_000);
  });
});

describe("nextBatchSize: doubling when the batch is fast", () => {
  it("doubles when the round trip is below 15 ms", () => {
    expect(nextBatchSize(1_000, 14)).toBe(2_000);
    expect(nextBatchSize(2_000, 0)).toBe(4_000);
  });

  it("caps the doubled size at 1,000,000", () => {
    expect(nextBatchSize(600_000, 1)).toBe(1_000_000);
    expect(nextBatchSize(1_000_000, 1)).toBe(1_000_000);
  });
});

describe("nextBatchSize: halving when the batch is slow", () => {
  it("halves when the round trip is above 60 ms", () => {
    expect(nextBatchSize(8_000, 61)).toBe(4_000);
    expect(nextBatchSize(4_000, 500)).toBe(2_000);
  });

  it("floors the halved size at 1,000", () => {
    expect(nextBatchSize(1_500, 100)).toBe(1_000);
    expect(nextBatchSize(1_000, 100)).toBe(1_000);
  });
});

describe("nextBatchSize: no change in the middle band", () => {
  it("keeps the size when the round trip is exactly 15 ms or exactly 60 ms", () => {
    expect(nextBatchSize(4_000, 15)).toBe(4_000);
    expect(nextBatchSize(4_000, 60)).toBe(4_000);
  });

  it("keeps the size between 15 ms and 60 ms", () => {
    expect(nextBatchSize(4_000, 30)).toBe(4_000);
  });
});
