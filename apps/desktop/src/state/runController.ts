// RunController: 実行セッションの開始・Step・Reset(5.4)と、無効化・古い応答の排除・ブレークポイント同期・窓の再取得(5.5)。
// Backend(ipc)・Workspace ストア(文書の真実)・debuggerStore の reducer を使い、IPC もストアのロジックも再実装しない。
// 表示状態は `host`(useReducer の dispatch/getState に相当)へ action として流す。React に依存しない。
//
// 方針:
// - コマンドは `enqueue` で直列に発行する(Pause だけが追い越す。Pause は 5.6 で追加する)。
// - 無効化の世代(`generation`): 編集・文書置換・セッション無効化のたびに進める。要求の発行時の世代と
//   応答時の世代が違えば、その応答は破棄する(破棄後に届いた古い応答が表示を書き換えないため)。
//   応答の sessionId は reducer でも照合する(二重の防御)。
// - 失敗は `failOperation` に集約し、セッション無効化(sessionNotFound / staleProgram)の判別を行う。
//   実行中: operationFailed → invalidated → operationFailed、非実行中: invalidated → operationFailed(破棄の順序は tasks.md 5.5 の無効化規則)。
//   実行中の分岐は 5.6 の Run ループ(runStarted)で初めて到達する。
// - Workspace の変更は購読で受ける: ソース・入力・有効な上限・文書の置換はセッションを破棄する(実行中を除く)。
//   ブレークポイントは置換コマンドで同期し、テープ半径は窓だけを取り直す。

import type { ApiError } from "../generated/ApiError";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { StepLimitDto } from "../generated/StepLimitDto";
import { encodeBase64 } from "../domain/base64";
import { parseInput } from "../domain/inputBytes";
import { MAX_SOURCE_BYTES } from "../domain/limits";
import type { Backend } from "../ipc/backend";
import { isSessionInvalidating, normalizeApiError } from "../ipc/errors";
import type { DebuggerAction, DebuggerState } from "./debuggerStore";
import type { WorkspaceState, WorkspaceStore } from "./workspace";

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
  /** Workspace の購読を解除する */
  dispose(): void;
};

/** ソースの UTF-8 バイト数(上限検査に使う) */
function utf8Length(text: string): number {
  return new TextEncoder().encode(text).length;
}

function isRunningPhase(phase: DebuggerState["phase"]): boolean {
  return phase === "running" || phase === "pausing";
}

/** 同じ昇順の数列か */
function sameNumbers(a: readonly number[], b: readonly number[]): boolean {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

/**
 * 実行に効く上限の値。入力欄が不正でも採用される値(直前の有効値)で比べるため、
 * 不正な入力の間は変化しない。
 */
function effectiveLimitKey(limit: WorkspaceState["limit"]): string {
  return limit.unlimited ? "unlimited" : `limited:${limit.lastValid.maxSteps}`;
}

/** セッションを破棄しうる編集(ソース・入力・有効な上限・文書の置換)が起きたか */
function executionInputsChanged(before: WorkspaceState, next: WorkspaceState): boolean {
  return (
    before.source !== next.source ||
    before.input.mode !== next.input.mode ||
    before.input.text !== next.input.text ||
    effectiveLimitKey(before.limit) !== effectiveLimitKey(next.limit) ||
    before.documentRevision !== next.documentRevision
  );
}

export function createRunController({ backend, workspace, host }: RunControllerDeps): RunController {
  const dispatch = (action: DebuggerAction): void => host.dispatch(action);

  // 無効化の世代。無効化のたびに進め、要求時の世代と違う応答は捨てる
  let generation = 0;
  // バックエンドが受理済みのブレークポイント集合(セッションごと)。同じ集合の再送を避ける(冪等)
  let synced: { sessionId: number; offsets: number[] } | null = null;

  // セッションを未開始へ戻す(世代を進め、同期状態を捨てる)
  const invalidate = (): void => {
    generation += 1;
    synced = null;
    dispatch({ type: "invalidated" });
  };

  // 操作の失敗を画面へ反映する。セッション無効化の種類は破棄の順序を守る(5.5 の無効化規則と同じ形)。
  const failOperation = (error: ApiError): void => {
    const running = isRunningPhase(host.getState().phase);
    if (!isSessionInvalidating(error)) {
      dispatch({ type: "operationFailed", error });
      return;
    }
    if (running) {
      dispatch({ type: "operationFailed", error });
      invalidate();
      dispatch({ type: "operationFailed", error });
      return;
    }
    invalidate();
    dispatch({ type: "operationFailed", error });
  };

  // 受理された応答の反映: 未解決の位置は受理集合から外し、設定が応答時点と同じなら Workspace からも外す。
  // Workspace の変更は購読を経て同期を発行するが、受理集合と一致するため再送しない(収束する)。
  const acceptBreakpoints = (sessionId: number, requested: readonly number[], unresolved: readonly number[]): void => {
    const dropped = new Set(unresolved);
    const accepted = requested.filter((offset) => !dropped.has(offset));
    synced = { sessionId, offsets: accepted };
    if (unresolved.length > 0 && sameNumbers(workspace.getSnapshot().breakpoints, requested)) {
      workspace.setBreakpoints(accepted);
    }
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

    const gen = generation;
    try {
      const loaded = await backend.loadSource({ source: snapshot.source });
      if (gen !== generation) {
        return false;
      }
      if (loaded.kind === "parseError") {
        dispatch({ type: "parseFailed", error: loaded.error });
        return false;
      }
      const requested = [...snapshot.breakpoints];
      const started = await backend.startSession({
        programId: loaded.programId,
        inputBase64: encodeBase64(input.bytes),
        limit,
        breakpointOffsets: requested,
        view: { tapeRadius: snapshot.tapeRadius, outputFrom: 0 },
      });
      if (gen !== generation) {
        return false;
      }
      dispatch({
        type: "sessionStarted",
        sessionId: started.state.sessionId,
        programId: loaded.programId,
        state: started.state,
      });
      acceptBreakpoints(started.state.sessionId, requested, started.breakpoints.unresolved);
      return host.getState().sessionId === started.state.sessionId;
    } catch (rejection: unknown) {
      if (gen !== generation) {
        return false;
      }
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
      const gen = generation;
      try {
        const result = await backend.step({
          sessionId: current.sessionId,
          view: { tapeRadius: snapshot.tapeRadius, outputFrom: current.output.end },
        });
        // 破棄・置換の後に届いた応答は捨てる
        if (gen !== generation) {
          return;
        }
        dispatch({ type: "stepResult", result });
      } catch (rejection: unknown) {
        if (gen !== generation) {
          return;
        }
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
      const gen = generation;
      try {
        const state: SessionStateDto = await backend.reset({
          sessionId: current.sessionId,
          view: { tapeRadius: snapshot.tapeRadius, outputFrom: 0 },
        });
        if (gen !== generation) {
          return;
        }
        dispatch({ type: "resetApplied", state });
      } catch (rejection: unknown) {
        if (gen !== generation) {
          return;
        }
        failOperation(normalizeApiError(rejection));
      }
    });

  // ブレークポイント同期: 現在のセッションへ現在の集合を置換として送る。受理済みと同じなら送らない(冪等)
  const syncBreakpoints = (): Promise<void> =>
    enqueue(async () => {
      const sessionId = host.getState().sessionId;
      if (sessionId === null) {
        return;
      }
      const requested = [...workspace.getSnapshot().breakpoints];
      if (synced !== null && synced.sessionId === sessionId && sameNumbers(synced.offsets, requested)) {
        return;
      }
      const gen = generation;
      try {
        const result = await backend.setBreakpoints({ sessionId, offsets: requested });
        if (gen !== generation) {
          return;
        }
        acceptBreakpoints(sessionId, requested, result.unresolved);
      } catch (rejection: unknown) {
        if (gen !== generation) {
          return;
        }
        failOperation(normalizeApiError(rejection));
      }
    });

  // テープ窓の再取得: 半径の変更で窓だけを取り直す。セッションは破棄しない
  const refreshWindow = (): Promise<void> =>
    enqueue(async () => {
      const sessionId = host.getState().sessionId;
      if (sessionId === null) {
        return;
      }
      const radius = workspace.getSnapshot().tapeRadius;
      const gen = generation;
      try {
        const window = await backend.getTapeWindow({ sessionId, radius });
        if (gen !== generation) {
          return;
        }
        dispatch({ type: "tapeWindowApplied", sessionId, window });
      } catch (rejection: unknown) {
        if (gen !== generation) {
          return;
        }
        failOperation(normalizeApiError(rejection));
      }
    });

  // Workspace の変更を購読する。実行中の編集は画面が受け付けないため、無効化は実行中には行わない。
  let previous = workspace.getSnapshot();
  const unsubscribe = workspace.subscribe(() => {
    const before = previous;
    const next = workspace.getSnapshot();
    previous = next;
    if (executionInputsChanged(before, next) && !isRunningPhase(host.getState().phase)) {
      invalidate();
    }
    if (!sameNumbers(before.breakpoints, next.breakpoints)) {
      void syncBreakpoints();
    }
    if (before.tapeRadius !== next.tapeRadius) {
      void refreshWindow();
    }
  });

  return { ensureSession, step, reset, dispose: unsubscribe };
}
