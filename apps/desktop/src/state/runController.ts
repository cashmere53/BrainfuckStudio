// RunController: 実行セッションの開始・Step・Reset(5.4)と、無効化・古い応答の排除・ブレークポイント同期・窓の再取得(5.5)。
// Backend(ipc)・Workspace ストア(文書の真実)・debuggerStore の reducer を使い、IPC もストアのロジックも再実装しない。
// 表示状態は `host`(useReducer の dispatch/getState に相当)へ action として流す。React に依存しない。
//
// 方針:
// - コマンドは `enqueue` で直列に発行する。Pause だけが追い越す(5.6): 局所フラグを立ててから backend の pause を
//   キューの外で直接呼ぶ。backend の pause はロックを取らないため、run_batch の実行中でも待たされない。
// - Run(5.6): 停止理由が batchReached かつ待機中でない限りバッチを反復する。バッチ幅は往復時間で適応する
//   (batchPolicy)。表示は描画フレームへ間引き、停止時は最終応答を必ず反映する。ループは描画を待たない
//   (隠れたウィンドウで描画が止まっても進行する)。
// - 無効化の世代(`generation`): 編集・文書置換・セッション無効化のたびに進める。要求の発行時の世代と
//   応答時の世代が違えば、その応答は破棄する(破棄後に届いた古い応答が表示を書き換えないため)。
//   応答の sessionId は reducer でも照合する(二重の防御)。
// - 失敗は `failOperation` に集約し、セッション無効化(sessionNotFound / staleProgram)の判別を行う。
//   実行中: operationFailed → invalidated → operationFailed、非実行中: invalidated → operationFailed(破棄の順序は tasks.md 5.5 の無効化規則)。
//   実行中の分岐は 5.6 の Run ループ(runStarted)で初めて到達する。
// - Workspace の変更は購読で受ける: ソース・入力・有効な上限・文書の置換はセッションを破棄する(実行中を除く)。
//   ブレークポイントは置換コマンドで同期し、テープ半径は窓だけを取り直す。

import type { ApiError } from "../generated/ApiError";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { StepLimitDto } from "../generated/StepLimitDto";
import { nextBatchSize } from "../domain/batchPolicy";
import { encodeBase64 } from "../domain/base64";
import { parseInput } from "../domain/inputBytes";
import { DEFAULT_BATCH_SIZE, MAX_SOURCE_BYTES } from "../domain/limits";
import { applyChunk, type OutputBuffer } from "../domain/outputBuffer";
import type { Backend } from "../ipc/backend";
import { isSessionInvalidating, normalizeApiError } from "../ipc/errors";
import type { DebuggerAction, DebuggerState } from "./debuggerStore";
import type { WorkspaceState, WorkspaceStore } from "./workspace";

/** 画面状態の読み書き口(React の useReducer が返す値と dispatch に相当) */
export type RunHost = {
  getState(): DebuggerState;
  dispatch(action: DebuggerAction): void;
};

/** 描画フレームの予約口。戻り値は予約の取り消し。テストでは手動の実装を注入する */
export type FrameScheduler = {
  request(callback: () => void): () => void;
};

export type RunControllerDeps = {
  backend: Backend;
  workspace: WorkspaceStore;
  host: RunHost;
  /** 単調増加の時刻(ms)。バッチの往復時間に使う。既定は performance.now */
  now?: () => number;
  /** 描画の間引きに使うフレーム予約。既定は requestAnimationFrame(無ければタイマー) */
  frames?: FrameScheduler;
  /**
   * 実行(Run・Step)を始めてよいか。開始の直前に判定する(ファイル操作の進行中は false にする)。既定は常に true
   */
  canStart?: () => boolean;
};

export type RunController = {
  /** セッションが無ければ開始する。開始できたら true(解析エラー・入力不正・失敗では false) */
  ensureSession(): Promise<boolean>;
  /** 停止するまでバッチを反復する(セッションが無ければ先に開始する)。待機中以外では発行しない */
  run(): Promise<void>;
  /** 実行中のバッチを一時停止する。キューを追い越して即時に発行する。実行中以外は何もしない */
  pause(): Promise<void>;
  /** 1 命令実行する。セッションが無ければ先に開始する */
  step(): Promise<void>;
  /** セッションを初期状態へ戻す(ブレークポイントは保持)。セッションが無ければ何もしない */
  reset(): Promise<void>;
  /** Run・Step の処理(開始処理・実行ループを含む)の途中か。文書の置換を控えるかの判定に使う */
  isBusy(): boolean;
  /** Workspace の購読を解除する。キューに残った処理は発行しない */
  dispose(): void;
};

/** 既定のフレーム予約: requestAnimationFrame。無い環境ではタイマーで代用する */
const defaultFrames: FrameScheduler = {
  request(callback) {
    if (typeof requestAnimationFrame === "function") {
      const id = requestAnimationFrame(() => {
        callback();
      });
      return () => {
        cancelAnimationFrame(id);
      };
    }
    const id = setTimeout(callback, 16);
    return () => {
      clearTimeout(id);
    };
  },
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

export function createRunController({
  backend,
  workspace,
  host,
  now = () => performance.now(),
  frames = defaultFrames,
  canStart = () => true,
}: RunControllerDeps): RunController {
  const dispatch = (action: DebuggerAction): void => host.dispatch(action);

  // 無効化の世代。無効化のたびに進め、要求時の世代と違う応答は捨てる
  let generation = 0;
  // バックエンドが受理済みのブレークポイント集合(セッションごと)。同じ集合の再送を避ける(冪等)
  let synced: { sessionId: number; offsets: number[] } | null = null;
  // 次のバッチの命令数。往復時間で適応し、無効化で既定へ戻す
  let batchSize: number = DEFAULT_BATCH_SIZE;
  // 予約中の描画フレームの取り消し(Run ループが予約する。停止・破棄で取り消す)
  let cancelFrame: (() => void) | null = null;
  // Run ループが動いているか(Pause の失敗を、ループの停止時に反映するか即時に反映するかの判定に使う)
  let loopActive = false;
  // 実行中の Pause の RPC 失敗(ループが止まった時点で、最終応答の反映の後に反映する)
  let pauseFailure: ApiError | null = null;
  // Run・Step の処理の途中か(開始処理・実行ループを含む。isBusy が返す)
  let busy = false;
  // 実行中に実行入力が変わった。停止の後(Run の finally)で無効化する(古いプログラムのセッションを残さない)
  let staleAfterRun = false;
  // 開始・実行中の Run(二重押しを 1 回にまとめるため、未完了の Run の処理を保持する)
  let runInFlight: Promise<void> | null = null;
  // dispose 済み。キューに残った処理は発行しない
  let disposed = false;

  const dropFrame = (): void => {
    if (cancelFrame !== null) {
      cancelFrame();
      cancelFrame = null;
    }
  };

  // セッションを未開始へ戻す(世代を進め、同期状態と適応値を捨てる)
  const invalidate = (): void => {
    generation += 1;
    synced = null;
    batchSize = DEFAULT_BATCH_SIZE;
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

  // 直列化: 前の要求が終わってから次を発行する(失敗は各タスク内で処理済みのため連鎖を止めない)。
  // 破棄の後にキューへ残った処理は発行せず、`skipped` を返す
  let tail: Promise<void> = Promise.resolve();
  const enqueue = <T>(task: () => Promise<T>, skipped: T): Promise<T> => {
    const next = tail.then(() => (disposed ? skipped : task()));
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

  const ensureSession = (): Promise<boolean> => enqueue(async () => (canStart() ? startSession() : false), false);

  const step = (): Promise<void> =>
    enqueue(async () => {
      if (!canStart()) {
        return;
      }
      busy = true;
      try {
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
      } finally {
        busy = false;
      }
    }, undefined);

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
    }, undefined);

  // ブレークポイント同期(キューの中で呼ぶ): 現在のセッションへ現在の集合を置換として送る。
  // 受理済みと同じなら送らない(冪等)。失敗・無効化なら false(呼び出し側は実行を始めない)
  const syncBreakpointsNow = async (): Promise<boolean> => {
    const sessionId = host.getState().sessionId;
    if (sessionId === null) {
      return true;
    }
    const requested = [...workspace.getSnapshot().breakpoints];
    if (synced !== null && synced.sessionId === sessionId && sameNumbers(synced.offsets, requested)) {
      return true;
    }
    const gen = generation;
    try {
      const result = await backend.setBreakpoints({ sessionId, offsets: requested });
      if (gen !== generation) {
        return false;
      }
      acceptBreakpoints(sessionId, requested, result.unresolved);
      return true;
    } catch (rejection: unknown) {
      if (gen !== generation) {
        return false;
      }
      failOperation(normalizeApiError(rejection));
      return false;
    }
  };

  const syncBreakpoints = (): Promise<void> =>
    enqueue(async () => {
      await syncBreakpointsNow();
    }, undefined);

  // Run のループ(キューの中で呼ぶ)。停止理由が batchReached かつ待機中でない限り、次のバッチを発行する。
  // 出力の不変条件: 次に要求する outputFrom は `accepted`(受理済みの出力の末尾)。画面の反映は間引くため
  // 画面の末尾より先行しうるが、要求は受理済みの末尾から行うので、未反映の応答と重ならない。
  // 表示: 応答の出力は `accepted` に積み、描画フレームで「画面の末尾より後」だけを 1 つのチャンクにして送る。
  // 停止(batchReached 以外・一時停止・失敗)では最終応答を必ず反映する。フレームはループの進行を待たない。
  const runLoop = async (sessionId: number): Promise<void> => {
    const gen = generation;
    // 受理済みの出力(画面の末尾より先行しうる)。初期値は画面の出力
    let accepted: OutputBuffer = host.getState().output;
    // 画面へまだ反映していない最新の応答(状態・停止理由の反映に使う)
    let latest: BatchResultDto | null = null;
    loopActive = true;

    // 未反映の応答を画面へ送る。画面の末尾から先の出力だけを連続のチャンクとして送る。
    // 画面の末尾が保持範囲より前なら保持範囲の全体を送る(不連続 → 置換。省略は truncated で示す)。
    // 世代が変わっていれば捨てる(破棄後の応答を表示へ書き込まない)
    const flushPending = (): void => {
      if (latest === null) {
        return;
      }
      const result = latest;
      latest = null;
      if (gen !== generation) {
        return;
      }
      const shown = host.getState().output;
      const from = shown.end >= accepted.bufferStart && shown.end <= accepted.end ? shown.end : accepted.bufferStart;
      const bytes = accepted.bytes.slice(from - accepted.bufferStart);
      dispatch({
        type: "batchResult",
        result: {
          ...result,
          state: {
            ...result.state,
            output: { dataStart: from, totalLen: accepted.totalLen, dataBase64: encodeBase64(bytes) },
          },
        },
      });
    };

    const onFrame = (): void => {
      cancelFrame = null;
      // 一時停止処理中の途中応答は送らない(停止時の最終応答で反映する。途中で止めると後続の応答が捨てられるため)
      if (latest !== null && host.getState().phase === "running") {
        flushPending();
      }
    };

    try {
      for (;;) {
        if (gen !== generation) {
          return;
        }
        const size = batchSize;
        const snapshot = workspace.getSnapshot();
        const started = now();
        let result: BatchResultDto;
        try {
          result = await backend.runBatch({
            sessionId,
            batchSize: size,
            view: { tapeRadius: snapshot.tapeRadius, outputFrom: accepted.end },
          });
        } catch (rejection: unknown) {
          if (gen !== generation) {
            return;
          }
          // 失敗より前の応答を先に反映してから、失敗を反映する(一時停止の失敗は実行の失敗に譲る)
          dropFrame();
          flushPending();
          pauseFailure = null;
          failOperation(normalizeApiError(rejection));
          return;
        }
        if (gen !== generation) {
          return;
        }
        batchSize = nextBatchSize(size, now() - started);
        accepted = applyChunk(accepted, result.state.output);
        latest = result;

        // 停止: バッチ到達以外の停止理由、一時停止処理中(Pause が届いたため次を発行しない)、
        // または Pause の RPC が失敗した(一時停止できなかったため、ここで止めて失敗を示す)
        if (result.stop.kind !== "batchReached" || host.getState().phase !== "running" || pauseFailure !== null) {
          dropFrame();
          flushPending();
          if (pauseFailure !== null) {
            const error = pauseFailure;
            pauseFailure = null;
            failOperation(error);
          }
          return;
        }
        if (cancelFrame === null) {
          cancelFrame = frames.request(onFrame);
        }
      }
    } finally {
      dropFrame();
      latest = null;
      loopActive = false;
    }
  };

  // Run: 待機中でなければ何もしない。開始 → ブレークポイント同期 → ループ(5.5 の同期の後に実行する)。
  // 開始中・実行中の Run は同じ処理に合流する(二重押しで停止位置の要求を積まない)
  const run = (): Promise<void> => {
    if (runInFlight !== null) {
      return runInFlight;
    }
    const task = enqueue(async () => {
      // canStart の判定も try の中に置く。早期 return でも finally で runInFlight を必ず戻す(戻さないと以後の Run が動かない)
      busy = true;
      try {
        if (!canStart()) {
          return;
        }
        if (host.getState().phase === "noSession" && !(await startSession())) {
          return;
        }
        if (host.getState().phase !== "idle") {
          return;
        }
        // ブレークポイントの同期に失敗したら実行しない(次の Run で同じ集合を再送する)
        if (!(await syncBreakpointsNow())) {
          return;
        }
        const current = host.getState();
        if (current.phase !== "idle" || current.sessionId === null) {
          return;
        }
        // 開始の反映: 実行中の表示へ切り替える(局所フラグ・通知・エラーは Run で下ろす)
        dispatch({ type: "runStarted" });
        await runLoop(current.sessionId);
      } finally {
        busy = false;
        runInFlight = null;
        // 実行中に入力が変わっていた場合は、停止の後に未開始へ戻す(表示は停止まで変えない)
        if (staleAfterRun) {
          staleAfterRun = false;
          if (!disposed) {
            invalidate();
          }
        }
      }
    }, undefined);
    runInFlight = task;
    return task;
  };

  // Pause: キューを追い越して即時に発行する。局所フラグを先に立て、backend の pause はロックを取らないため
  // run_batch の実行中でも待たされない。実行中以外・セッションが無い場合は何もしない。
  const pause = async (): Promise<void> => {
    const current = host.getState();
    if (current.phase !== "running" || current.sessionId === null) {
      return;
    }
    const sessionId = current.sessionId;
    dispatch({ type: "pauseRequested" });
    try {
      await backend.pause({ sessionId });
    } catch (rejection: unknown) {
      const error = normalizeApiError(rejection);
      // 実行ループが動いていれば、ループが止まった時点で最終応答を反映してから失敗を示す(表示の順序を保つ)。
      // ループが既に終わっていれば、ここで直接示す
      if (loopActive) {
        pauseFailure = error;
      } else {
        failOperation(error);
      }
    }
  };

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
    }, undefined);

  // Workspace の変更を購読する。実行中の変更は表示を変えず、停止の後に無効化する(staleAfterRun)。
  // 実行中でなければ(開始処理の途中を含む)直ちに無効化し、開始の応答は世代で捨てる。
  let previous = workspace.getSnapshot();
  const unsubscribe = workspace.subscribe(() => {
    const before = previous;
    const next = workspace.getSnapshot();
    previous = next;
    if (executionInputsChanged(before, next)) {
      if (isRunningPhase(host.getState().phase)) {
        staleAfterRun = true;
      } else {
        invalidate();
      }
    }
    if (!sameNumbers(before.breakpoints, next.breakpoints)) {
      void syncBreakpoints();
    }
    if (before.tapeRadius !== next.tapeRadius) {
      void refreshWindow();
    }
  });

  // 破棄: 購読を解除し、世代を進めて進行中の応答を捨て、予約中の描画を取り消す。キューの残りは発行しない
  const dispose = (): void => {
    disposed = true;
    unsubscribe();
    generation += 1;
    dropFrame();
  };

  const isBusy = (): boolean => busy;

  return { ensureSession, run, pause, step, reset, isBusy, dispose };
}
