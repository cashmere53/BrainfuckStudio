import { describe, expect, it } from "vitest";
import type { ApiError } from "../generated/ApiError";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { OutputChunkDto } from "../generated/OutputChunkDto";
import type { ParseErrorDto } from "../generated/ParseErrorDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { StepResultDto } from "../generated/StepResultDto";
import type { TapeWindowDto } from "../generated/TapeWindowDto";
import type { UiPhase } from "../domain/controls";
import { encodeBase64 } from "../domain/base64";
import { deriveRunLabel } from "../domain/runLabel";
import {
  debuggerReducer,
  displayOf,
  initialDebuggerState,
  initialDisplay,
  type DebuggerAction,
  type DebuggerState,
} from "./debuggerStore";

/** 出力全体の `dataStart` からのバイト列を DTO の形で作る */
function chunk(dataStart: number, totalLen: number, bytes: number[]): OutputChunkDto {
  return { dataStart, totalLen, dataBase64: encodeBase64(new Uint8Array(bytes)) };
}

/** 状態 DTO を作る(既定値は未開始に近い値) */
function dto(
  sessionId: number,
  overrides: Partial<Omit<SessionStateDto, "sessionId">> & { stepsExecuted?: number } = {},
): SessionStateDto {
  const { stepsExecuted = 0, ...rest } = overrides;
  return {
    sessionId,
    snapshot: { pc: 0, dp: 0, currentCell: 0, stepsExecuted, halted: false },
    tapeWindow: { start: 0, cells: [0], pointerOffset: 0 },
    output: chunk(0, 0, []),
    inputConsumed: 0,
    inputTotal: 0,
    lastStop: null,
    currentRange: null,
    ...rest,
  };
}

function batch(state: SessionStateDto, stop: BatchResultDto["stop"], executed = 0): BatchResultDto {
  return { stop, executed, state };
}

function apply(state: DebuggerState, ...actions: DebuggerAction[]): DebuggerState {
  return actions.reduce(debuggerReducer, state);
}

const RUNTIME_ERR = { kind: "pointerOutOfBounds", pc: 3, dp: 0, direction: "left" } as const;
const PARSE_ERR: ParseErrorDto = {
  kind: "unmatchedOpeningBracket",
  instructionIndex: 0,
  position: { byteOffset: 0, utf16Offset: 0, line: 1, column: 1 },
};
const IO_ERR: ApiError = { kind: "io", message: "failed to read file: denied", field: null };
const SESSION_ERR: ApiError = { kind: "sessionNotFound", message: "session not found", field: null };

/** 6 つの画面状態(表駆動テストの軸) */
const PHASES: readonly UiPhase[] = ["noSession", "idle", "running", "pausing", "halted", "failed"];

/** 開始済み(セッション 7・プログラム 3・出力なし)の待機状態 */
function started(): DebuggerState {
  return apply(initialDebuggerState(), {
    type: "sessionStarted",
    sessionId: 7,
    programId: 3,
    state: dto(7),
  });
}

function runningState(): DebuggerState {
  return apply(started(), { type: "runStarted" });
}

/** Pause を受けた実行中処理(pausing) */
function pausingState(): DebuggerState {
  return apply(runningState(), { type: "pauseRequested" });
}

/** 局所 Pause で止まった(一時停止)状態 */
function pausedState(): DebuggerState {
  return apply(pausingState(), { type: "batchResult", result: batch(dto(7, { stepsExecuted: 5 }), { kind: "cancelled" }, 5) });
}

/** ブレークポイントで止まった(待機・スクロール要求あり)状態 */
function breakpointState(): DebuggerState {
  return apply(runningState(), { type: "batchResult", result: batch(dto(7, { lastStop: "breakpoint" }), { kind: "breakpoint" }) });
}

/** 指定の画面状態を作る(セッション 7) */
function inPhase(phase: UiPhase): DebuggerState {
  switch (phase) {
    case "noSession":
      return initialDebuggerState();
    case "idle":
      return started();
    case "running":
      return runningState();
    case "pausing":
      return pausingState();
    case "halted":
      return apply(runningState(), {
        type: "batchResult",
        result: batch(dto(7, { lastStop: "halted" }), { kind: "halted" }),
      });
    case "failed":
      return apply(runningState(), {
        type: "batchResult",
        result: batch(dto(7), { kind: "error", error: RUNTIME_ERR }),
      });
  }
}

describe("初期状態と未開始の表示", () => {
  it("セッションも出力も通知もなく、画面状態は未開始、フラグは立っていない", () => {
    const s = initialDebuggerState();
    expect(s).toEqual({
      phase: "noSession",
      sessionId: null,
      programId: null,
      state: null,
      output: expect.objectContaining({ totalLen: 0, bytes: new Uint8Array(0) }),
      error: null,
      notices: [],
      localPaused: false,
      scrollRequest: false,
    });
  });

  it("未開始の表示は tape_window_of(dp = 0) と同じ形: 窓先頭 0・窓内の dp 0・半径ぶんの右端", () => {
    const shown = initialDisplay(2);
    expect(shown.snapshot).toEqual({ pc: 0, dp: 0, currentCell: 0, stepsExecuted: 0, halted: false });
    expect(shown.tapeWindow).toEqual({ start: 0, cells: [0, 0, 0], pointerOffset: 0 });
    expect(shown.lastStop).toBeNull();
    expect(shown.currentRange).toBeNull();
  });

  it.each([
    [0, 1],
    [2, 3],
    [64, 65],
    [100_000, 30_000],
  ])("半径 %i の窓は %i セルで、添字は負にならず dp は窓の先頭", (radius, length) => {
    const { tapeWindow } = initialDisplay(radius);
    expect(tapeWindow.cells).toHaveLength(length);
    expect(tapeWindow.start).toBe(0);
    expect(tapeWindow.start).toBeGreaterThanOrEqual(0);
    expect(tapeWindow.start + tapeWindow.pointerOffset).toBe(0);
  });

  it("セッションが無い表示は半径 0 の窓(1 セル・dp 0)", () => {
    expect(displayOf(null, 0).tapeWindow).toEqual({ start: 0, cells: [0], pointerOffset: 0 });
  });

  it("セッションがあれば表示はその状態 DTO の値になる", () => {
    const state = dto(7, { stepsExecuted: 4 });
    expect(displayOf(state, 2).snapshot.stepsExecuted).toBe(4);
  });
});

describe("セッション開始 sessionStarted", () => {
  it("開始でセッションとプログラムを記録し、待機になり、出力は応答の内容になる", () => {
    const s = apply(initialDebuggerState(), {
      type: "sessionStarted",
      sessionId: 7,
      programId: 3,
      state: dto(7, { output: chunk(0, 2, [0x61, 0x62]) }),
    });
    expect(s.phase).toBe("idle");
    expect(s.sessionId).toBe(7);
    expect(s.programId).toBe(3);
    expect(s.state?.sessionId).toBe(7);
    expect(s.output.totalLen).toBe(2);
    expect(s.output.bytes).toEqual(new Uint8Array([0x61, 0x62]));
  });

  it("開始は前回の出力・エラー・通知を捨てる", () => {
    const s = apply(
      started(),
      { type: "operationFailed", error: IO_ERR },
      { type: "sessionStarted", sessionId: 8, programId: 4, state: dto(8, { output: chunk(0, 1, [0x41]) }) },
    );
    expect(s.error).toBeNull();
    expect(s.notices).toEqual([]);
    expect(s.output.bytes).toEqual(new Uint8Array([0x41]));
  });

  it("連続する差分は出力に連結し、不連続な差分は出力を置き換える", () => {
    let s = started();
    s = apply(s, { type: "runStarted" }, { type: "batchResult", result: batch(dto(7, { output: chunk(0, 2, [1, 2]) }), { kind: "batchReached" }, 2) });
    s = apply(s, { type: "batchResult", result: batch(dto(7, { output: chunk(2, 3, [3]) }), { kind: "batchReached" }, 1) });
    expect(s.output.bytes).toEqual(new Uint8Array([1, 2, 3]));
    s = apply(s, { type: "batchResult", result: batch(dto(7, { output: chunk(10, 12, [9, 9]) }), { kind: "batchReached" }, 1) });
    expect(s.output.bytes).toEqual(new Uint8Array([9, 9]));
    expect(s.output.totalLen).toBe(12);
  });

  it("新しい ID の開始は未開始・待機・停止中のいずれからも受理し、セッションを入れ替える", () => {
    for (const phase of ["noSession", "idle", "halted", "failed"] as const) {
      const s = debuggerReducer(inPhase(phase), {
        type: "sessionStarted",
        sessionId: 8,
        programId: 4,
        state: dto(8),
      });
      expect(s.phase, phase).toBe("idle");
      expect(s.sessionId, phase).toBe(8);
      expect(s.error, phase).toBeNull();
      expect(s.scrollRequest, phase).toBe(false);
    }
  });

  it.each(["running", "pausing"] as const)("実行中(%s)の開始応答は無視する(同じ参照)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "sessionStarted", sessionId: 8, programId: 4, state: dto(8) })).toBe(s);
  });

  it.each([
    ["同じ ID", 7],
    ["古い ID", 6],
  ] as const)("%s の開始応答は待機・停止中で無視する(同じ参照)", (_label, id) => {
    for (const phase of ["idle", "halted", "failed"] as const) {
      const s = inPhase(phase);
      expect(debuggerReducer(s, { type: "sessionStarted", sessionId: id, programId: 3, state: dto(id) }), phase).toBe(s);
    }
  });

  it("応答の状態 ID が要求の ID と違う開始応答は無視する(同じ参照)", () => {
    const s = inPhase("noSession");
    expect(debuggerReducer(s, { type: "sessionStarted", sessionId: 8, programId: 4, state: dto(9) })).toBe(s);
  });
});

describe("Run・Pause の遷移", () => {
  it("待機から Run で実行中になり、実行エラーと通知を消す", () => {
    const s = apply(started(), { type: "operationFailed", error: SESSION_ERR }, { type: "runStarted" });
    expect(s.phase).toBe("running");
    expect(s.error).toBeNull();
    expect(s.notices).toEqual([]);
    expect(s.localPaused).toBe(false);
  });

  it.each(PHASES.filter((p) => p !== "idle"))("Run は %s では無視する(同じ参照)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "runStarted" })).toBe(s);
  });

  it("Pause は実行中だけ一時停止処理中にし、局所フラグを立てる", () => {
    const s = apply(runningState(), { type: "pauseRequested" });
    expect(s.phase).toBe("pausing");
    expect(s.localPaused).toBe(true);
  });

  it.each(PHASES.filter((p) => p !== "running"))("Pause は %s では無視する(同じ参照)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "pauseRequested" })).toBe(s);
  });

  it("Pause で止まった(cancelled)は待機に戻り、局所フラグを保つ → 状態ラベルは「一時停止」", () => {
    const s = pausedState();
    expect(s.phase).toBe("idle");
    expect(s.localPaused).toBe(true);
    expect(s.scrollRequest).toBe(true);
    expect(deriveRunLabel({ phase: s.phase, inputValid: true, stoppedByPause: s.localPaused }, s.state)).toBe("一時停止");
  });

  it("バッチ間に Pause が届いた(処理中の batchReached)場合も待機に戻り、一時停止になる", () => {
    const s = apply(
      runningState(),
      { type: "pauseRequested" },
      { type: "batchResult", result: batch(dto(7, { stepsExecuted: 1000 }), { kind: "batchReached" }, 1000) },
    );
    expect(s.phase).toBe("idle");
    expect(s.localPaused).toBe(true);
    expect(s.scrollRequest).toBe(true);
  });

  it("Pause 後の Run で局所フラグが下り、次の停止は正しいラベルになる", () => {
    const s = apply(
      pausedState(),
      { type: "runStarted" },
      { type: "batchResult", result: batch(dto(7, { stepsExecuted: 906, lastStop: "halted" }), { kind: "halted" }, 1) },
    );
    expect(s.localPaused).toBe(false);
    expect(s.phase).toBe("halted");
    expect(deriveRunLabel({ phase: s.phase, inputValid: true, stoppedByPause: s.localPaused }, s.state)).toBe("正常終了");
  });

  it("一時停止中に停止が確定しても、古いフラグで「正常終了」「ブレークポイントで停止」が「一時停止」に化けない", () => {
    const halted = apply(
      runningState(),
      { type: "pauseRequested" },
      { type: "batchResult", result: batch(dto(7, { stepsExecuted: 9, lastStop: "halted" }), { kind: "halted" }, 9) },
    );
    expect(halted.localPaused).toBe(false);
    expect(deriveRunLabel({ phase: halted.phase, inputValid: true, stoppedByPause: halted.localPaused }, halted.state)).toBe("正常終了");

    const bp = apply(
      runningState(),
      { type: "pauseRequested" },
      { type: "batchResult", result: batch(dto(7, { stepsExecuted: 3, lastStop: "breakpoint" }), { kind: "breakpoint" }, 3) },
    );
    expect(bp.localPaused).toBe(false);
    expect(deriveRunLabel({ phase: bp.phase, inputValid: true, stoppedByPause: bp.localPaused }, bp.state)).toBe("ブレークポイントで停止");
  });

  const stepExecuted: StepResultDto = { outcome: { kind: "executed" }, state: dto(7, { stepsExecuted: 6 }) };
  it.each<[string, DebuggerAction]>([
    ["Run", { type: "runStarted" }],
    ["Step", { type: "stepResult", result: stepExecuted }],
    ["Reset", { type: "resetApplied", state: dto(7) }],
    ["編集(破棄)", { type: "invalidated" }],
  ])("局所フラグは %s で必ず下りる", (_name, action) => {
    // 一時停止後の状態は待機(idle)なので、Run・Step・Reset・編集のいずれも適用される
    const next = debuggerReducer(pausedState(), action);
    expect(next.localPaused).toBe(false);
    expect(deriveRunLabel({ phase: next.phase, inputValid: true, stoppedByPause: next.localPaused }, next.state)).not.toBe("一時停止");
  });
});

describe("バッチ応答 batchResult(実行中のみ有効)", () => {
  // 実行中(running / pausing)から各停止理由で遷移したときの結果の表
  it.each<[string, UiPhase, BatchResultDto["stop"], { phase: UiPhase; localPaused: boolean; scroll: boolean; error: "runtime" | null }]>([
    ["running・バッチ継続", "running", { kind: "batchReached" }, { phase: "running", localPaused: false, scroll: false, error: null }],
    ["pausing・バッチ継続(Pause 取りこぼし)", "pausing", { kind: "batchReached" }, { phase: "idle", localPaused: true, scroll: true, error: null }],
    ["running・Pause 由来の cancelled", "running", { kind: "cancelled" }, { phase: "idle", localPaused: true, scroll: true, error: null }],
    ["pausing・Pause 由来の cancelled", "pausing", { kind: "cancelled" }, { phase: "idle", localPaused: true, scroll: true, error: null }],
    ["running・ブレークポイント", "running", { kind: "breakpoint" }, { phase: "idle", localPaused: false, scroll: true, error: null }],
    ["pausing・ブレークポイント", "pausing", { kind: "breakpoint" }, { phase: "idle", localPaused: false, scroll: true, error: null }],
    ["running・正常終了", "running", { kind: "halted" }, { phase: "halted", localPaused: false, scroll: true, error: null }],
    ["pausing・正常終了", "pausing", { kind: "halted" }, { phase: "halted", localPaused: false, scroll: true, error: null }],
    ["running・実行エラー", "running", { kind: "error", error: RUNTIME_ERR }, { phase: "failed", localPaused: false, scroll: true, error: "runtime" }],
    ["pausing・実行エラー(局所フラグを残さない)", "pausing", { kind: "error", error: RUNTIME_ERR }, { phase: "failed", localPaused: false, scroll: true, error: "runtime" }],
  ])("%s", (_label, from, stop, expected) => {
    const next = dto(7, { stepsExecuted: 9 });
    const s = apply(inPhase(from), { type: "batchResult", result: batch(next, stop, 9) });
    expect(s.phase).toBe(expected.phase);
    expect(s.localPaused).toBe(expected.localPaused);
    expect(s.scrollRequest).toBe(expected.scroll);
    expect(s.error?.category ?? null).toBe(expected.error);
    expect(s.state).toBe(next);
  });

  it.each(["noSession", "idle", "halted", "failed"] as const)("%s では実行中でないため、同じセッションの応答も無視する(同じ参照)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "batchResult", result: batch(dto(7, { stepsExecuted: 1 }), { kind: "halted" }) })).toBe(s);
    expect(debuggerReducer(s, { type: "batchResult", result: batch(dto(7), { kind: "batchReached" }) })).toBe(s);
  });

  it.each(["running", "pausing"] as const)("%s で別のセッション ID の応答は無視する(同じ参照)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "batchResult", result: batch(dto(99, { stepsExecuted: 1 }), { kind: "halted" }) })).toBe(s);
  });

  it("実行中の batchReached は実行中のまま(スクロール要求は立てない)", () => {
    const s = apply(runningState(), { type: "batchResult", result: batch(dto(7, { stepsExecuted: 1000 }), { kind: "batchReached" }, 1000) });
    expect(s.phase).toBe("running");
    expect(s.scrollRequest).toBe(false);
  });

  it("出力は実行中の応答の差分を反映する", () => {
    const s = apply(runningState(), { type: "batchResult", result: batch(dto(7, { output: chunk(0, 1, [0x41]) }), { kind: "batchReached" }, 1) });
    expect(s.output.bytes).toEqual(new Uint8Array([0x41]));
  });
});

describe("Step 応答 stepResult(待機中のみ有効)", () => {
  it("1 命令の実行は待機のまま状態と出力を反映し、実行エラー・通知を消し、停止位置へのスクロール要求を立てる", () => {
    const s = apply(
      started(),
      { type: "operationFailed", error: IO_ERR },
      { type: "stepResult", result: { outcome: { kind: "executed" }, state: dto(7, { stepsExecuted: 1, output: chunk(0, 1, [0x41]) }) } },
    );
    expect(s.phase).toBe("idle");
    expect(s.error).toBeNull();
    expect(s.notices).toEqual([]);
    expect(s.state?.snapshot.stepsExecuted).toBe(1);
    expect(s.output.bytes).toEqual(new Uint8Array([0x41]));
    // 要件 7.2: Step で止まったとき(1 命令後)もスクロールを求める。tasks.md 5.3 の文言より要件を優先する
    expect(s.scrollRequest).toBe(true);
  });

  it("最後の命令で正常終了は停止状態になり、スクロール要求を立てる", () => {
    const s = apply(started(), { type: "stepResult", result: { outcome: { kind: "halted" }, state: dto(7, { lastStop: "halted" }) } });
    expect(s.phase).toBe("halted");
    expect(s.scrollRequest).toBe(true);
  });

  it("Step の実行エラーは失敗状態・実行エラーになり、スクロール要求を立てる", () => {
    const err = { kind: "stepLimitExceeded", limit: 1, pc: 0 } as const;
    const s = apply(started(), { type: "stepResult", result: { outcome: { kind: "error", error: err }, state: dto(7) } });
    expect(s.phase).toBe("failed");
    expect(s.error).toEqual({ category: "runtime", error: err });
    expect(s.localPaused).toBe(false);
    expect(s.scrollRequest).toBe(true);
  });

  it.each(["noSession", "running", "pausing", "halted", "failed"] as const)("%s では Step の応答を反映しない(同じ参照)", (phase) => {
    const s = inPhase(phase);
    const result: StepResultDto = { outcome: { kind: "executed" }, state: dto(7, { stepsExecuted: 1 }) };
    expect(debuggerReducer(s, { type: "stepResult", result })).toBe(s);
  });

  it("別のセッション ID の Step 応答は無視する(同じ参照)", () => {
    const s = started();
    const result: StepResultDto = { outcome: { kind: "executed" }, state: dto(99, { stepsExecuted: 1 }) };
    expect(debuggerReducer(s, { type: "stepResult", result })).toBe(s);
  });
});

describe("Reset 応答 resetApplied(実行中はガード)", () => {
  it("Reset は停止状態から待機に戻り、出力・エラー・通知・スクロール要求を捨てて応答の出力で置き換える", () => {
    const halted = apply(
      runningState(),
      { type: "batchResult", result: batch(dto(7, { output: chunk(0, 1, [0x41]), lastStop: "halted" }), { kind: "halted" }) },
    );
    expect(halted.phase).toBe("halted");
    expect(halted.output.totalLen).toBe(1);
    expect(halted.scrollRequest).toBe(true);
    const next = apply(halted, { type: "resetApplied", state: dto(7) });
    expect(next.phase).toBe("idle");
    expect(next.error).toBeNull();
    expect(next.notices).toEqual([]);
    expect(next.scrollRequest).toBe(false);
    expect(next.output.totalLen).toBe(0);
    expect(next.output.bytes).toEqual(new Uint8Array([]));
    expect(next.state?.lastStop).toBeNull();
  });

  it("Reset は失敗状態からも待機に戻る", () => {
    const next = apply(inPhase("failed"), { type: "resetApplied", state: dto(7) });
    expect(next.phase).toBe("idle");
    expect(next.error).toBeNull();
  });

  it.each(["running", "pausing"] as const)("実行中(%s)の Reset 応答は無視する(同じ参照)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "resetApplied", state: dto(7) })).toBe(s);
  });

  it("セッションが無い状態・別セッションの Reset 応答は無視する(同じ参照)", () => {
    const none = inPhase("noSession");
    expect(debuggerReducer(none, { type: "resetApplied", state: dto(7) })).toBe(none);
    const halted = inPhase("halted");
    expect(debuggerReducer(halted, { type: "resetApplied", state: dto(99) })).toBe(halted);
  });
});

describe("解析エラー parseFailed(未開始でだけ有効)", () => {
  it("解析エラーはセッションを作らず、未開始のまま解析エラーとして保持する", () => {
    const s = apply(inPhase("noSession"), { type: "parseFailed", error: PARSE_ERR });
    expect(s.phase).toBe("noSession");
    expect(s.sessionId).toBeNull();
    expect(s.state).toBeNull();
    expect(s.error).toEqual({ category: "parse", error: PARSE_ERR });
    expect(s.notices).toEqual([]);
  });

  it.each(["idle", "running", "pausing", "halted", "failed"] as const)("%s では解析エラーを反映しない(同じ参照)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "parseFailed", error: PARSE_ERR })).toBe(s);
  });
});

describe("操作の失敗 operationFailed(通知として保持し、error を上書きしない)", () => {
  it.each(["noSession", "idle", "halted", "failed"] as const)("%s では通知を追加し、画面状態・セッション・error を変えない", (phase) => {
    const before = inPhase(phase);
    const s = apply(before, { type: "operationFailed", error: IO_ERR });
    expect(s.phase).toBe(before.phase);
    expect(s.sessionId).toBe(before.sessionId);
    expect(s.error).toBe(before.error);
    expect(s.notices).toEqual([IO_ERR]);
    expect(s.localPaused).toBe(false);
  });

  it("解析エラー表示中の操作失敗は解析エラーを保持する", () => {
    const parsed = apply(inPhase("noSession"), { type: "parseFailed", error: PARSE_ERR });
    const s = apply(parsed, { type: "operationFailed", error: IO_ERR });
    expect(s.error).toEqual({ category: "parse", error: PARSE_ERR });
    expect(s.notices).toEqual([IO_ERR]);
  });

  it("実行エラー表示中の操作失敗は実行エラーを保持し、両方を並べて持つ", () => {
    const s = apply(inPhase("failed"), { type: "operationFailed", error: IO_ERR });
    expect(s.phase).toBe("failed");
    expect(s.error).toEqual({ category: "runtime", error: RUNTIME_ERR });
    expect(s.notices).toEqual([IO_ERR]);
    expect(s.scrollRequest).toBe(true);
  });

  it.each(["running", "pausing"] as const)("実行中(%s)の操作失敗は待機へ戻し、局所フラグを下ろす(セッションは生かす)", (phase) => {
    // 出力を先に作ってから Pause を受ける(pausing 中の batchReached は停止になるため順序を守る)
    const running = apply(runningState(), {
      type: "batchResult",
      result: batch(dto(7, { output: chunk(0, 1, [0x41]) }), { kind: "batchReached" }, 1),
    });
    const before = phase === "pausing" ? apply(running, { type: "pauseRequested" }) : running;
    expect(before.phase).toBe(phase);
    const s = apply(before, { type: "operationFailed", error: SESSION_ERR });
    expect(s.phase).toBe("idle");
    expect(s.localPaused).toBe(false);
    expect(s.sessionId).toBe(7);
    expect(s.state).toBe(before.state);
    expect(s.output.bytes).toEqual(new Uint8Array([0x41]));
    expect(s.error).toBeNull();
    expect(s.notices).toEqual([SESSION_ERR]);
  });

  it("実行中の操作失敗の後は、待機から Run で続けられる", () => {
    const failedDuringRun = apply(pausingState(), { type: "operationFailed", error: SESSION_ERR });
    const resumed = apply(failedDuringRun, { type: "runStarted" });
    expect(resumed.phase).toBe("running");
    expect(resumed.notices).toEqual([]);
  });

  it("通知は最新の 1 件だけを保持する(次の失敗で置き換える)", () => {
    const s = apply(started(), { type: "operationFailed", error: IO_ERR }, { type: "operationFailed", error: SESSION_ERR });
    expect(s.notices).toEqual([SESSION_ERR]);
  });
});

describe("無効化 invalidated(実行中は無視)", () => {
  it.each(["idle", "halted", "failed"] as const)("%s からは未開始に戻し、セッション・出力・エラー・通知・スクロール要求を消す", (phase) => {
    const s = apply(inPhase(phase), { type: "operationFailed", error: IO_ERR }, { type: "invalidated" });
    expect(s.phase).toBe("noSession");
    expect(s.sessionId).toBeNull();
    expect(s.programId).toBeNull();
    expect(s.state).toBeNull();
    expect(s.output.totalLen).toBe(0);
    expect(s.error).toBeNull();
    expect(s.notices).toEqual([]);
    expect(s.scrollRequest).toBe(false);
  });

  it("未開始で変化がない無効化は同じ参照を返す", () => {
    const s = initialDebuggerState();
    expect(debuggerReducer(s, { type: "invalidated" })).toBe(s);
  });

  it("未開始でも解析エラー・通知は編集で消えるため、無効化は変化として新しい状態を返す", () => {
    const parsed = apply(initialDebuggerState(), { type: "parseFailed", error: PARSE_ERR });
    expect(debuggerReducer(parsed, { type: "invalidated" }).error).toBeNull();
    const noticed = apply(initialDebuggerState(), { type: "operationFailed", error: IO_ERR });
    expect(debuggerReducer(noticed, { type: "invalidated" }).notices).toEqual([]);
  });

  it.each(["running", "pausing"] as const)("実行中(%s)の無効化は無視する(同じ参照。編集は実行中に無効)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "invalidated" })).toBe(s);
  });

  it("破棄後に届いた古いセッションの遅延応答は表示に反映されない(同じ参照)", () => {
    const invalidated = apply(started(), { type: "invalidated" });
    const late = batch(dto(7, { stepsExecuted: 50, output: chunk(0, 3, [1, 2, 3]) }), { kind: "halted" });
    expect(debuggerReducer(invalidated, { type: "batchResult", result: late })).toBe(invalidated);
    expect(debuggerReducer(invalidated, { type: "resetApplied", state: late.state })).toBe(invalidated);
    expect(debuggerReducer(invalidated, { type: "stepResult", result: { outcome: { kind: "executed" }, state: late.state } })).toBe(invalidated);
  });

  it("操作エラーは次の操作・無効化で消える(待機への Run・Step・Reset・編集も消す)", () => {
    const withError = apply(started(), { type: "operationFailed", error: IO_ERR });
    expect(debuggerReducer(withError, { type: "invalidated" }).notices).toEqual([]);
    expect(debuggerReducer(withError, { type: "resetApplied", state: dto(7) }).notices).toEqual([]);
    expect(debuggerReducer(withError, { type: "runStarted" }).notices).toEqual([]);
    expect(debuggerReducer(withError, { type: "stepResult", result: { outcome: { kind: "executed" }, state: dto(7) } }).notices).toEqual([]);
  });
});

describe("スクロール要求 scrollRequest", () => {
  it("要求は実行中から停止状態へ遷移したときに立ち、消費で降りる", () => {
    const stopped = breakpointState();
    expect(stopped.scrollRequest).toBe(true);
    const consumed = debuggerReducer(stopped, { type: "scrollConsumed" });
    expect(consumed.scrollRequest).toBe(false);
    expect(consumed.phase).toBe("idle");
  });

  it.each([
    ["ブレークポイント停止", breakpointState],
    ["正常終了", () => inPhase("halted")],
    ["実行エラー", () => inPhase("failed")],
  ] as const)("%s の要求は消費で降り、画面状態・エラーは変えない", (_label, make) => {
    const before = make();
    expect(before.scrollRequest).toBe(true);
    const after = debuggerReducer(before, { type: "scrollConsumed" });
    expect(after.scrollRequest).toBe(false);
    expect(after.phase).toBe(before.phase);
    expect(after.error).toBe(before.error);
  });

  it.each(["noSession", "idle", "running", "pausing"] as const)("立っていない %s の要求を消費しても同じ参照を返す", (phase) => {
    const s = inPhase(phase);
    expect(s.scrollRequest).toBe(false);
    expect(debuggerReducer(s, { type: "scrollConsumed" })).toBe(s);
  });
});

describe("テープ窓の再取得 tapeWindowApplied(現在のセッションの窓だけを差し替える)", () => {
  const newWindow: TapeWindowDto = { start: -2, cells: [0, 1, 0, 0, 0], pointerOffset: 2 };

  it("現在のセッションの窓だけを差し替え、ほかのフィールドは同じ値のまま保つ", () => {
    const before = apply(breakpointState(), { type: "operationFailed", error: IO_ERR });
    const after = debuggerReducer(before, { type: "tapeWindowApplied", sessionId: 7, window: newWindow });
    expect(after).not.toBe(before);
    expect(after.state).toEqual({ ...before.state, tapeWindow: newWindow });
    expect(after.state?.snapshot).toBe(before.state?.snapshot);
    expect(after.phase).toBe(before.phase);
    expect(after.sessionId).toBe(before.sessionId);
    expect(after.programId).toBe(before.programId);
    expect(after.output).toBe(before.output);
    expect(after.error).toBe(before.error);
    expect(after.notices).toBe(before.notices);
    expect(after.localPaused).toBe(before.localPaused);
    expect(after.scrollRequest).toBe(before.scrollRequest);
  });

  it.each(["idle", "running", "pausing", "halted", "failed"] as const)(
    "%s でも現在のセッションの窓なら差し替え、画面状態(phase・出力・フラグ)は変えない",
    (phase) => {
      const before = inPhase(phase);
      const after = debuggerReducer(before, { type: "tapeWindowApplied", sessionId: 7, window: newWindow });
      expect(after.state?.tapeWindow).toEqual(newWindow);
      expect(after.phase).toBe(before.phase);
      expect(after.localPaused).toBe(before.localPaused);
      expect(after.scrollRequest).toBe(before.scrollRequest);
      expect(after.output).toBe(before.output);
    },
  );

  it.each(PHASES)("%s: sessionId が違う窓の応答は無視する(同じ参照)", (phase) => {
    const s = inPhase(phase);
    expect(debuggerReducer(s, { type: "tapeWindowApplied", sessionId: 6, window: newWindow })).toBe(s);
  });

  it("未開始(state が null)の状態では、ID が一致しても同じ参照を返す", () => {
    const s = initialDebuggerState();
    expect(debuggerReducer(s, { type: "tapeWindowApplied", sessionId: 7, window: newWindow })).toBe(s);
  });

  it("破棄後に届いた窓の応答は表示に反映されない(同じ参照)", () => {
    const invalidated = apply(started(), { type: "invalidated" });
    expect(debuggerReducer(invalidated, { type: "tapeWindowApplied", sessionId: 7, window: newWindow })).toBe(invalidated);
  });
});

describe("クライアント通知 ClientNotice(UI が検出した拒否も operationFailed で扱う)", () => {
  const BP_REJECTED = { kind: "client", code: "breakpointRejected" } as const;
  const SWITCH_REJECTED = { kind: "client", code: "inputSwitchRejected", reason: "notUtf8" } as const;

  it("ブレークポイントの拒否を通知として保持し、error を上書きしない", () => {
    const before = apply(inPhase("failed"));
    const s = apply(before, { type: "operationFailed", error: BP_REJECTED });
    expect(s.notices).toEqual([BP_REJECTED]);
    expect(s.error).toBe(before.error);
  });

  it("入力モード切替の拒否は理由を保持する(最新の 1 件だけ)", () => {
    const s = apply(
      started(),
      { type: "operationFailed", error: BP_REJECTED },
      { type: "operationFailed", error: SWITCH_REJECTED },
    );
    expect(s.notices).toEqual([SWITCH_REJECTED]);
  });

  it("次に受理された操作(Step)で消え、無変更の操作では同じ参照を保つ", () => {
    const noticed = apply(started(), { type: "operationFailed", error: SWITCH_REJECTED });
    const stepped = debuggerReducer(noticed, { type: "stepResult", result: { outcome: { kind: "executed" }, state: dto(7) } });
    expect(stepped.notices).toEqual([]);
    // 無効な操作(実行中の Step など)は通知を消さず、同じ参照を返す
    expect(debuggerReducer(noticed, { type: "pauseRequested" })).toBe(noticed);
  });
});
