// RunController: 実行セッションの開始・Step・Reset を組み立てる(5.4)。
// Backend(ipc)・Workspace ストア(文書の真実)・debuggerStore の reducer を使い、IPC もストアのロジックも再実装しない。
// 表示状態は `host`(useReducer の dispatch/getState に相当)へ action として流す。React に依存しない。
//
// 方針:
// - コマンドは `enqueue` で直列に発行する(Pause だけが追い越す。Pause は 5.6 で追加する)。
// - 応答は reducer が `sessionId` で照合するため、ここでは反映の action を流すだけでよい。
// - 失敗は `failOperation` に集約し、セッション無効化(sessionNotFound / staleProgram)の判別を行う。
//   実行中: operationFailed → invalidated → operationFailed、非実行中: invalidated → operationFailed(破棄の順序は tasks.md 5.5 の無効化規則)。
//   実行中の分岐は 5.6 の Run ループ(runStarted)で初めて到達する。5.4 は runStarted を発行しないため、ここには到達しない。

import type { ApiError } from "../generated/ApiError";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { StepLimitDto } from "../generated/StepLimitDto";
import { encodeBase64 } from "../domain/base64";
import { parseInput } from "../domain/inputBytes";
import { MAX_SOURCE_BYTES } from "../domain/limits";
import type { Backend } from "../ipc/backend";
import { isSessionInvalidating, normalizeApiError } from "../ipc/errors";
import type { DebuggerAction, DebuggerState } from "./debuggerStore";
import type { WorkspaceStore } from "./workspace";

/** 画面状態の読み書き口(React の useReducer が返す値と dispatch に相当) */
export type RunHost = {
  getState(): DebuggerState;
  dispatch(action: DebuggerAction): void;
};

export type RunControllerDeps = {
  backend: Backend;
  workspace: WorkspaceStore;
  host: RunHost;
};

export type RunController = {
  /** セッションが無ければ開始する。開始できたら true(解析エラー・入力不正・失敗では false) */
  ensureSession(): Promise<boolean>;
  /** 1 命令実行する。セッションが無ければ先に開始する */
  step(): Promise<void>;
  /** セッションを初期状態へ戻す(ブレークポイントは保持)。セッションが無ければ何もしない */
  reset(): Promise<void>;
};

/** ソースの UTF-8 バイト数(上限検査に使う) */
function utf8Length(text: string): number {
  return new TextEncoder().encode(text).length;
}

function isRunningPhase(phase: DebuggerState["phase"]): boolean {
  return phase === "running" || phase === "pausing";
}

export function createRunController({ backend, workspace, host }: RunControllerDeps): RunController {
  const dispatch = (action: DebuggerAction): void => host.dispatch(action);

  // 操作の失敗を画面へ反映する。セッション無効化の種類は破棄の順序を守る(5.5 の無効化規則と同じ形)。
  const failOperation = (error: ApiError): void => {
    const running = isRunningPhase(host.getState().phase);
    if (!isSessionInvalidating(error)) {
      dispatch({ type: "operationFailed", error });
      return;
    }
    if (running) {
      dispatch({ type: "operationFailed", error });
      dispatch({ type: "invalidated" });
      dispatch({ type: "operationFailed", error });
      return;
    }
    dispatch({ type: "invalidated" });
    dispatch({ type: "operationFailed", error });
  };

  // 直列化: 前の要求が終わってから次を発行する(失敗は各タスク内で処理済みのため連鎖を止めない)
  let tail: Promise<void> = Promise.resolve();
  const enqueue = <T>(task: () => Promise<T>): Promise<T> => {
    const next = tail.then(task);
    tail = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  };

  // 開始フロー: 入力の検証 → 直前の有効な上限(または上限なし) → ソースサイズ → 読み込み → 解析 → セッション開始
  const startSession = async (): Promise<boolean> => {
    if (host.getState().sessionId !== null) {
      return true;
    }
    const snapshot = workspace.getSnapshot();

    // 入力が不正な間は開始しない(入力欄の検証結果がそのまま示される)
    const input = parseInput(snapshot.input);
    if (!input.ok) {
      return false;
    }

    // 命令数上限の入力欄が不正でも、直前に有効だった値で開始する
    const limit: StepLimitDto = snapshot.limit.unlimited
      ? { kind: "unlimited" }
      : { kind: "limited", maxSteps: snapshot.limit.lastValid.maxSteps };

    // 編集中の内容(未保存を含む)を対象にする。1 MiB を超えるなら開始せず通知する(要件 2.14)
    if (utf8Length(snapshot.source) > MAX_SOURCE_BYTES) {
      // 文面は Rust 側 `ApiErrorKind::SourceTooLarge` と同じ固定の英語文(表示は apiErrorMessageJa が日本語にする)
      dispatch({
        type: "operationFailed",
        error: { kind: "sourceTooLarge", message: "source is too large", field: "source" },
      });
      return false;
    }

    try {
      const loaded = await backend.loadSource({ source: snapshot.source });
      if (loaded.kind === "parseError") {
        dispatch({ type: "parseFailed", error: loaded.error });
        return false;
      }
      const started = await backend.startSession({
        programId: loaded.programId,
        inputBase64: encodeBase64(input.bytes),
        limit,
        breakpointOffsets: [...snapshot.breakpoints],
        view: { tapeRadius: snapshot.tapeRadius, outputFrom: 0 },
      });
      dispatch({
        type: "sessionStarted",
        sessionId: started.state.sessionId,
        programId: loaded.programId,
        state: started.state,
      });
      return host.getState().sessionId === started.state.sessionId;
    } catch (rejection: unknown) {
      failOperation(normalizeApiError(rejection));
      return false;
    }
  };

  const ensureSession = (): Promise<boolean> => enqueue(startSession);

  const step = (): Promise<void> =>
    enqueue(async () => {
      if (host.getState().phase === "noSession" && !(await startSession())) {
        return;
      }
      const current = host.getState();
      // 待機中(idle)のみ Step できる。停止状態・実行エラー・実行中は無効(要件 4.8 / 4.11)
      if (current.phase !== "idle" || current.sessionId === null) {
        return;
      }
      const snapshot = workspace.getSnapshot();
      try {
        const result = await backend.step({
          sessionId: current.sessionId,
          view: { tapeRadius: snapshot.tapeRadius, outputFrom: current.output.end },
        });
        dispatch({ type: "stepResult", result });
      } catch (rejection: unknown) {
        failOperation(normalizeApiError(rejection));
      }
    });

  const reset = (): Promise<void> =>
    enqueue(async () => {
      const current = host.getState();
      if (current.sessionId === null || isRunningPhase(current.phase)) {
        return;
      }
      const snapshot = workspace.getSnapshot();
      try {
        const state: SessionStateDto = await backend.reset({
          sessionId: current.sessionId,
          view: { tapeRadius: snapshot.tapeRadius, outputFrom: 0 },
        });
        dispatch({ type: "resetApplied", state });
      } catch (rejection: unknown) {
        failOperation(normalizeApiError(rejection));
      }
    });

  return { ensureSession, step, reset };
}
