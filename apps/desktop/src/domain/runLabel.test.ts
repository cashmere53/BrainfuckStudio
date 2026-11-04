import { describe, expect, it } from "vitest";
import type { StopReasonDto } from "../generated/StopReasonDto";
import { deriveRunLabel, type RunSession, type UiState } from "./runLabel";

const idle: UiState = { phase: "idle", inputValid: true };
const halted: UiState = { phase: "halted", inputValid: true };

function session(stepsExecuted: number, lastStop: StopReasonDto | null): RunSession {
  return { snapshot: { stepsExecuted }, lastStop };
}

describe("deriveRunLabel: セッションが無い・実行中・エラー", () => {
  it("セッションが無ければ「未開始」", () => {
    expect(deriveRunLabel({ phase: "noSession", inputValid: true }, null)).toBe("未開始");
  });

  it("Running は「実行中」(セッションの内容によらない)", () => {
    expect(deriveRunLabel({ phase: "running", inputValid: true }, session(5, null))).toBe("実行中");
    expect(deriveRunLabel({ phase: "running", inputValid: true }, null)).toBe("実行中");
  });

  it("Pausing は一時停止の完了前なので「実行中」", () => {
    expect(deriveRunLabel({ phase: "pausing", inputValid: true }, session(5, null))).toBe("実行中");
  });

  it("Failed は「実行エラー」", () => {
    expect(deriveRunLabel({ phase: "failed", inputValid: true }, session(3, null))).toBe("実行エラー");
  });
});

describe("deriveRunLabel: 停止状態(Halted/Idle)の lastStop による分類", () => {
  it("lastStop = halted は「正常終了」", () => {
    expect(deriveRunLabel(halted, session(42, "halted"))).toBe("正常終了");
  });

  it("lastStop = breakpoint は「ブレークポイントで停止」", () => {
    expect(deriveRunLabel(halted, session(42, "breakpoint"))).toBe("ブレークポイントで停止");
  });

  it("lastStop = cancelled は「一時停止」", () => {
    expect(deriveRunLabel(halted, session(42, "cancelled"))).toBe("一時停止");
  });

  it("lastStop = batchReached(局所 Pause の後に残る値)は「一時停止」", () => {
    expect(deriveRunLabel(halted, session(42, "batchReached"))).toBe("一時停止");
  });
});

describe("deriveRunLabel: lastStop が null の場合(未開始・ステップ実行後・局所 Pause)", () => {
  it("steps == 0 かつ lastStop == null は「未開始」(開始直後・リセット直後)", () => {
    expect(deriveRunLabel(idle, session(0, null))).toBe("未開始");
  });

  it("lastStop == null かつ steps > 0 は「ステップ実行後」", () => {
    expect(deriveRunLabel(idle, session(1, null))).toBe("ステップ実行後");
    expect(deriveRunLabel(halted, session(7, null))).toBe("ステップ実行後");
  });

  it("局所 Pause で止まった場合は lastStop が null でも「一時停止」", () => {
    expect(deriveRunLabel({ ...halted, stoppedByPause: true }, session(7, null))).toBe("一時停止");
  });

  it("局所 Pause フラグは停止位置の lastStop より優先される", () => {
    expect(deriveRunLabel({ ...halted, stoppedByPause: true }, session(7, "batchReached"))).toBe("一時停止");
  });
});
