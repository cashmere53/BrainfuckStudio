// @vitest-environment node
import { describe, expect, it } from "vitest";
import { shouldCollectFailureMaterials } from "./fixtureFailure.ts";

describe("shouldCollectFailureMaterials(失敗の材料を残すかの判定)", () => {
  it("準備(接続・ページの特定・エディタの表示待ち・observeIpc)で失敗したら、結果の状態によらず残す", () => {
    expect(shouldCollectFailureMaterials(true, "passed", "passed")).toBe(true);
    expect(shouldCollectFailureMaterials(true, "failed", "passed")).toBe(true);
    expect(shouldCollectFailureMaterials(true, undefined, "passed")).toBe(true);
  });

  it("本体(use の後)で失敗したら残す", () => {
    expect(shouldCollectFailureMaterials(false, "failed", "passed")).toBe(true);
  });

  it("期待どおりに成功したら残さない", () => {
    expect(shouldCollectFailureMaterials(false, "passed", "passed")).toBe(false);
  });

  it("test.fail() で期待どおりに失敗したら残さない(期待の状態と結果の状態が一致)", () => {
    expect(shouldCollectFailureMaterials(false, "failed", "failed")).toBe(false);
  });

  it("test.fail() なのに成功してしまったら残す(期待の失敗と結果が食い違う)", () => {
    expect(shouldCollectFailureMaterials(false, "passed", "failed")).toBe(true);
  });

  it("タイムアウト・中断・結果の状態が未定のときは期待と一致しない限り残す", () => {
    expect(shouldCollectFailureMaterials(false, "timedOut", "passed")).toBe(true);
    expect(shouldCollectFailureMaterials(false, "interrupted", "passed")).toBe(true);
    expect(shouldCollectFailureMaterials(false, undefined, "passed")).toBe(true);
    expect(shouldCollectFailureMaterials(false, "timedOut", "failed")).toBe(true);
  });
});
