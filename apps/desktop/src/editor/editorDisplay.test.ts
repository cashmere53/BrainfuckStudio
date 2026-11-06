import { describe, expect, it } from "vitest";
import type { ParseErrorDto } from "../generated/ParseErrorDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { SourceRangeDto } from "../generated/SourceRangeDto";
import { type DebuggerState, initialDebuggerState } from "../state/debuggerStore";
import { editorDisplayOf } from "./editorDisplay";

function range(from: number, to: number): SourceRangeDto {
  const pos = (utf16Offset: number) => ({ byteOffset: utf16Offset, utf16Offset, line: 1, column: utf16Offset + 1 });
  return { start: pos(from), end: pos(to) };
}

/** セッションのある状態(表示に使うフィールドだけを埋める) */
function sessionState(currentRange: SourceRangeDto | null): SessionStateDto {
  return {
    sessionId: 1,
    snapshot: { pc: 0, dp: 0, currentCell: 0, stepsExecuted: 0, halted: false },
    tapeWindow: { start: 0, cells: [0], pointerOffset: 0 },
    output: { dataStart: 0, totalLen: 0, dataBase64: "" },
    inputConsumed: 0,
    inputTotal: 0,
    lastStop: null,
    currentRange,
  } as SessionStateDto;
}

function withState(patch: Partial<DebuggerState>): DebuggerState {
  return { ...initialDebuggerState(), ...patch };
}

const PARSE_ERROR: ParseErrorDto = {
  kind: "unmatchedOpeningBracket",
  instructionIndex: 0,
  position: { byteOffset: 4, utf16Offset: 4, line: 1, column: 5 },
};

describe("editorDisplayOf(debuggerStore → エディタの表示)", () => {
  it("未開始の画面は何も強調しない", () => {
    expect(editorDisplayOf(initialDebuggerState())).toEqual({
      execution: null,
      parseError: null,
      scrollRequest: false,
    });
  });

  it("停止中(待機・実行中・一時停止処理中)は現在命令を current で示す", () => {
    for (const phase of ["idle", "running", "pausing"] as const) {
      const display = editorDisplayOf(
        withState({ phase, sessionId: 1, state: sessionState(range(2, 3)) }),
      );
      expect(display.execution).toEqual({ range: { from: 2, to: 3 }, kind: "current" });
    }
  });

  it("現在命令が無い(停止・終端到達)なら強調しない", () => {
    const display = editorDisplayOf(withState({ phase: "halted", sessionId: 1, state: sessionState(null) }));
    expect(display.execution).toBeNull();
  });

  it("実行エラーでは、エラーを起こした命令を runtimeError で示す", () => {
    const display = editorDisplayOf(
      withState({
        phase: "failed",
        sessionId: 1,
        state: sessionState(range(5, 6)),
        error: {
          category: "runtime",
          error: { kind: "pointerOutOfBounds", pc: 5, dp: 0, direction: "left" },
        },
      }),
    );
    expect(display.execution).toEqual({ range: { from: 5, to: 6 }, kind: "runtimeError" });
    expect(display.parseError).toBeNull();
  });

  it("解析エラーでは括弧の 1 文字を parseError で示し、現在命令は強調しない", () => {
    const display = editorDisplayOf(
      withState({ phase: "noSession", error: { category: "parse", error: PARSE_ERROR } }),
    );
    expect(display.parseError).toEqual({ from: 4, to: 5 });
    expect(display.execution).toBeNull();
  });

  it("停止時のスクロール要求をそのまま伝える", () => {
    const display = editorDisplayOf(
      withState({ phase: "idle", sessionId: 1, state: sessionState(range(0, 1)), scrollRequest: true }),
    );
    expect(display.scrollRequest).toBe(true);
  });
});
