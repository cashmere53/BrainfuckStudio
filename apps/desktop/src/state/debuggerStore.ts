// デバッガ表示状態のストア(reducer)。画面状態・セッション ID・プログラム ID・最新の状態 DTO・出力バッファ・
// 実行/解析エラー(error)・操作の通知(notices)・局所 Pause フラグ・停止時のスクロール要求を保持する。
// React/Tauri に依存しない純粋な reducer で、画面は useReducer(debuggerReducer, initialDebuggerState()) で使う。
// 表は design.md「debuggerStore」「画面状態機械」「Error Strategy」と要件 1.3 / 7.2 / 8.7 / 10.1 / 10.7 / 10.8 に対応する。
//
// 不変条件: 画面状態機械で定められていない遷移(無効な操作・破棄済みセッションの応答など)は、
// 状態を変えず同じ参照を返す。これにより購読側は無変更の応答で再描画しない。

import type { ApiError } from "../generated/ApiError";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { ParseErrorDto } from "../generated/ParseErrorDto";
import type { RuntimeErrorDto } from "../generated/RuntimeErrorDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { StepResultDto } from "../generated/StepResultDto";
import type { UiPhase } from "../domain/controls";
import { applyChunk, emptyOutput, type OutputBuffer } from "../domain/outputBuffer";

/**
 * 表示中のエラー(画面を停止させる結果)。解析エラーと実行エラーを区別する。
 * - `parse`: 解析エラー(セッションは作られない)。`error` 欄に出す
 * - `runtime`: 実行エラー(状態は直前の値を保持)。`error` 欄に出す
 *
 * 操作の失敗(ファイル I/O・不正な要求・通信など)は `DebuggerState.notices` に分けて持つ。
 * 操作の失敗は `error` を上書きしない(要件 10.7)。
 */
export type DisplayError =
  | { category: "parse"; error: ParseErrorDto }
  | { category: "runtime"; error: RuntimeErrorDto };

export type DebuggerState = {
  phase: UiPhase;
  /** 現在のセッション ID。セッションが無ければ null */
  sessionId: number | null;
  /** 現在のプログラム ID。セッションが無ければ null */
  programId: number | null;
  /** 最新の状態 DTO。セッションが無ければ null */
  state: SessionStateDto | null;
  /** 表示用の出力バッファ(末尾 65,536 バイト + 総バイト数) */
  output: OutputBuffer;
  /** 実行結果のエラー(解析または実行)。無ければ null */
  error: DisplayError | null;
  /**
   * 操作の通知(操作エラー)。最新の 1 件だけを保持する配列。
   * 規則: 操作が失敗すると追加(直前の通知は置き換える)。次に受理された操作
   * (Run / Step / Reset / 開始 / 編集・無効化)で消える。`error` とは独立に維持される。
   */
  notices: readonly ApiError[];
  /**
   * RunController の局所 Pause で止まったか(状態ラベル「一時停止」の判定に使う)。
   * Run・Step・Reset・編集・停止の確定(局所 Pause 以外の停止理由)で必ず下ろす。
   */
  localPaused: boolean;
  /**
   * 停止時のエディタの停止位置へのスクロール要求。実行中から停止状態へ遷移したとき、
   * および Step の 1 命令後(executed / halted / error)に立つ。エディタが消費したら降ろす。
   * Step でも立てるのは要件 7.2(Step で止まったとき)を優先するため(tasks.md 5.3 の文言より広い)。
   */
  scrollRequest: boolean;
};

export type DebuggerAction =
  | { type: "sessionStarted"; sessionId: number; programId: number; state: SessionStateDto }
  | { type: "runStarted" }
  | { type: "pauseRequested" }
  | { type: "batchResult"; result: BatchResultDto }
  | { type: "stepResult"; result: StepResultDto }
  | { type: "resetApplied"; state: SessionStateDto }
  | { type: "parseFailed"; error: ParseErrorDto }
  | { type: "operationFailed"; error: ApiError }
  | { type: "invalidated" }
  | { type: "scrollConsumed" };

/** 画面に出す値(セッションの状態 DTO のうち表示に使う部分) */
export type DisplayValues = Pick<
  SessionStateDto,
  "snapshot" | "tapeWindow" | "currentRange" | "inputConsumed" | "inputTotal" | "lastStop"
>;

/** テープのセル数(brainfuck-core の `TAPE_SIZE` と同じ)。窓の右端の切り詰めに使う */
const TAPE_SIZE = 30_000;

/**
 * 未開始の初期表示。`tape_window_of(tape, dp = 0, radius)` と同じ形にする:
 * 窓先頭は 0、右端は `min(radius, TAPE_SIZE - 1)`、窓内の `dp` は 0。負の添字は出ない。
 */
export function initialDisplay(tapeRadius: number): DisplayValues {
  const end = Math.min(tapeRadius, TAPE_SIZE - 1);
  return {
    snapshot: { pc: 0, dp: 0, currentCell: 0, stepsExecuted: 0, halted: false },
    tapeWindow: {
      start: 0,
      cells: Array.from({ length: end + 1 }, () => 0),
      pointerOffset: 0,
    },
    currentRange: null,
    inputConsumed: 0,
    inputTotal: 0,
    lastStop: null,
  };
}

/** 画面に出す値: セッションがあればその状態、無ければ未開始の表示 */
export function displayOf(state: SessionStateDto | null, tapeRadius: number): DisplayValues {
  return state ?? initialDisplay(tapeRadius);
}

/** セッションが無い初期状態(画面の起動直後・無効化後) */
export function initialDebuggerState(): DebuggerState {
  return {
    phase: "noSession",
    sessionId: null,
    programId: null,
    state: null,
    output: emptyOutput(),
    error: null,
    notices: [],
    localPaused: false,
    scrollRequest: false,
  };
}

export function debuggerReducer(s: DebuggerState, action: DebuggerAction): DebuggerState {
  switch (action.type) {
    case "sessionStarted":
      return applySessionStarted(s, action);

    case "runStarted":
      if (s.phase !== "idle") return s;
      return { ...s, phase: "running", localPaused: false, error: null, notices: [] };

    case "pauseRequested":
      if (s.phase !== "running") return s;
      return { ...s, phase: "pausing", localPaused: true };

    case "batchResult":
      return applyBatch(s, action.result);

    case "stepResult":
      return applyStep(s, action.result);

    case "resetApplied":
      if (isRunningPhase(s.phase) || !isCurrent(s, action.state)) return s;
      return {
        ...s,
        phase: "idle",
        state: action.state,
        output: applyChunk(emptyOutput(), action.state.output),
        error: null,
        notices: [],
        localPaused: false,
        // 未消費のスクロール要求は、リセットで表示が先頭に戻るため不要になる
        scrollRequest: false,
      };

    case "parseFailed":
      // 解析エラーはセッション開始の途中(未開始の画面)でだけ起きる。画面状態機械の NoSession → NoSession
      if (s.phase !== "noSession") return s;
      return { ...initialDebuggerState(), error: { category: "parse", error: action.error } };

    case "operationFailed":
      // 操作の失敗は通知として追加する。`error`・セッション・出力は変えない(要件 10.7)
      if (isRunningPhase(s.phase)) {
        // 実行中(run_batch など)の操作が失敗した場合: 実行ループはここで止まるため待機へ戻し、
        // セッションは生きているので操作を続けられる状態に保つ(要件 10.7「操作を継続できる状態に保つ」)。
        // 画面状態機械に明示の遷移は無いが、この規則は要件 10.7 に基づく。
        return { ...s, phase: "idle", localPaused: false, notices: [action.error] };
      }
      return { ...s, notices: [action.error] };

    case "invalidated":
      // 実行中は編集不可(要件 5.10)のため、実行中の無効化は起きない。起きても無視する
      if (isRunningPhase(s.phase)) return s;
      // 編集は通知とエラーも消す(「次の操作で消える」規則)。既に初期なら同じ参照を返す
      if (isInitial(s)) return s;
      return initialDebuggerState();

    case "scrollConsumed":
      if (!s.scrollRequest) return s;
      return { ...s, scrollRequest: false };
  }
}

/** 実行中(通常実行または一時停止処理中)か */
function isRunningPhase(phase: UiPhase): boolean {
  return phase === "running" || phase === "pausing";
}

/** 応答のセッションが現在のセッションと一致するか(破棄済みセッションの遅延応答を捨てる) */
function isCurrent(s: DebuggerState, state: SessionStateDto): boolean {
  return s.sessionId !== null && s.sessionId === state.sessionId;
}

/** 初期状態と同じか(無効化で変化が無いときに同じ参照を返すため) */
function isInitial(s: DebuggerState): boolean {
  return (
    s.phase === "noSession" &&
    s.sessionId === null &&
    s.programId === null &&
    s.state === null &&
    s.output.totalLen === 0 &&
    s.error === null &&
    s.notices.length === 0 &&
    !s.localPaused &&
    !s.scrollRequest
  );
}

/**
 * セッション開始の応答を反映する。
 * - 実行中は無視する(実行ループのセッションを入れ替えないため)
 * - 現在のセッションが無い、または ID が現在より新しいときだけ受理する(古い ID・同じ ID は無視)
 */
function applySessionStarted(
  s: DebuggerState,
  action: Extract<DebuggerAction, { type: "sessionStarted" }>,
): DebuggerState {
  if (isRunningPhase(s.phase)) return s;
  if (action.state.sessionId !== action.sessionId) return s;
  if (s.sessionId !== null && action.sessionId <= s.sessionId) return s;
  return {
    phase: "idle",
    sessionId: action.sessionId,
    programId: action.programId,
    state: action.state,
    output: applyChunk(emptyOutput(), action.state.output),
    error: null,
    notices: [],
    localPaused: false,
    scrollRequest: false,
  };
}

/**
 * バッチの応答を反映する。実行中(running / pausing)のみ有効。
 * - `batchReached`: 実行中なら継続(実行中のまま)。一時停止処理中なら次のバッチを発行しない停止として待機へ戻す
 * - `cancelled`: Pause による停止。待機へ戻り、局所フラグを保つ(「一時停止」)
 * - `breakpoint` / `halted` / `error`: 停止の確定。局所フラグは必ず下ろす
 */
function applyBatch(s: DebuggerState, result: BatchResultDto): DebuggerState {
  if (!isRunningPhase(s.phase) || !isCurrent(s, result.state)) return s;
  const updated: DebuggerState = {
    ...s,
    state: result.state,
    output: applyChunk(s.output, result.state.output),
  };
  switch (result.stop.kind) {
    case "batchReached":
      if (s.phase === "running") return updated;
      return stopped(updated, "idle", true, null);
    case "cancelled":
      return stopped(updated, "idle", true, null);
    case "breakpoint":
      return stopped(updated, "idle", false, null);
    case "halted":
      return stopped(updated, "halted", false, null);
    case "error":
      return stopped(updated, "failed", false, { category: "runtime", error: result.stop.error });
  }
}

/**
 * Step の応答を反映する。待機中のみ有効。
 * Step は操作なので通知を消し、局所フラグを下ろす。1 命令後は停止位置へ移るため、
 * executed / halted / error のいずれでもスクロール要求を立てる(要件 7.2)。
 */
function applyStep(s: DebuggerState, result: StepResultDto): DebuggerState {
  if (s.phase !== "idle" || !isCurrent(s, result.state)) return s;
  const base: DebuggerState = {
    ...s,
    state: result.state,
    output: applyChunk(s.output, result.state.output),
    error: null,
    notices: [],
    localPaused: false,
    scrollRequest: true,
  };
  switch (result.outcome.kind) {
    case "executed":
      return base;
    case "halted":
      return { ...base, phase: "halted" };
    case "error":
      return { ...base, phase: "failed", error: { category: "runtime", error: result.outcome.error } };
  }
}

/** 実行中から停止状態へ遷移する。停止時のスクロール要求を立てる */
function stopped(s: DebuggerState, phase: UiPhase, localPaused: boolean, error: DisplayError | null): DebuggerState {
  return { ...s, phase, localPaused, error, scrollRequest: true };
}
