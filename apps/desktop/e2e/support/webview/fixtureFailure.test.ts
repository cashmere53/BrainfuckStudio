// @vitest-environment node
import { describe, expect, it } from "vitest";
import { hostCallRefusal, shouldCollectFailureMaterials } from "./fixtureFailure.ts";

describe("hostCallRefusal(アプリが終了した後に OS の窓口を呼ばない判定)", () => {
  it("アプリが動いていれば(exit が null)呼んでよい(null を返す)", () => {
    expect(hostCallRefusal(null, "sendKeys", 1234)).toBeNull();
  });

  it("終了していれば(exit が null でない)呼ばずに、操作名・pid・終了の状態を含む理由を返す", () => {
    const refusal = hostCallRefusal({ code: 0, signal: null }, "requestClose", 1234);
    expect(refusal).not.toBeNull();
    expect(refusal).toContain("requestClose");
    expect(refusal).toContain("1234");
    expect(refusal).toContain("終了コード 0");
    expect(refusal).toContain("シグナル なし");
  });

  it("終了コードが無く、シグナルで終わった場合も理由を返す", () => {
    const refusal = hostCallRefusal({ code: null, signal: "SIGKILL" }, "sendKeys", 99);
    expect(refusal).toContain("終了コード なし");
    expect(refusal).toContain("SIGKILL");
  });

  it("異常終了(終了コードが 1)でも呼ばない", () => {
    expect(hostCallRefusal({ code: 1, signal: null }, "windowTitle", 5)).not.toBeNull();
  });
});

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
