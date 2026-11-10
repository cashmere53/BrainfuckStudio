// 比較の状態: 結果・実行中・要求 ID・実行時の共有値のスナップショット・エラーを持つ reducer と ComparisonController。
// 通信は ComparisonBackend(ipc)、要求の導出は deriveRequest(domain)に任せ、ここでは状態遷移と判定だけを行う。
// デバッガの状態・実行コントローラには触れない(要件 11.11)。React に依存しない。
//
// 方針:
// - 実行中(phase = running)は新しい実行を始めない(要件 11.3)。
// - 応答は「実行中かつ要求 ID が最新」のときだけ反映する。破棄(discarded)は要求 ID を進めるため、届き遅れた応答は無視される。
// - 結果が古いかは、実行時に使った共有値(usedSnapshot)と現在の共有値の値比較で決める(要件 11.8)。

import type { ApiError } from "../../generated/ApiError";
import type { ComparisonInfoDto } from "../../generated/ComparisonInfoDto";
import type { ComparisonResultDto } from "../../generated/ComparisonResultDto";
import { normalizeApiError } from "../../ipc/errors";
import type { WorkspaceValues } from "../../state/workspace";
import { deriveRequest, type Selection } from "../domain/comparisonRequest";
import type { ComparisonBackend } from "../ipc";

export type ComparisonPhase = "idle" | "running" | "done" | "failed";

export type ComparisonState = {
  phase: ComparisonPhase;
  /** 最新の要求 ID(開始・破棄のたびに進む) */
  requestId: number;
  /** 完了した比較の結果(解析エラーを含む)。完了以外では null */
  result: ComparisonResultDto | null;
  /** 実行時に使った共有値。「古い結果」の判定に使う。開始前・未実行では null */
  usedSnapshot: WorkspaceValues | null;
  /** 操作エラー(通知用)。失敗以外では null */
  error: ApiError | null;
};

export const initialComparisonState: ComparisonState = {
  phase: "idle",
  requestId: 0,
  result: null,
  usedSnapshot: null,
  error: null,
};

export type ComparisonAction =
  | { type: "runStarted"; requestId: number; snapshot: WorkspaceValues }
  | { type: "runSucceeded"; requestId: number; result: ComparisonResultDto }
  | { type: "runFailed"; requestId: number; error: ApiError }
  | { type: "discarded" };

/** 実行中で、かつ要求 ID が最新の応答か */
function isCurrentRun(state: ComparisonState, requestId: number): boolean {
  return state.phase === "running" && state.requestId === requestId;
}

export function comparisonReducer(state: ComparisonState, action: ComparisonAction): ComparisonState {
  switch (action.type) {
    case "runStarted":
      return {
        phase: "running",
        requestId: action.requestId,
        result: null,
        usedSnapshot: action.snapshot,
        error: null,
      };
    case "runSucceeded":
      if (!isCurrentRun(state, action.requestId)) {
        return state;
      }
      return { ...state, phase: "done", result: action.result, error: null };
    case "runFailed":
      if (!isCurrentRun(state, action.requestId)) {
        return state;
      }
      return { ...state, phase: "failed", result: null, error: action.error };
    case "discarded":
      // 要求 ID を進めて、破棄前の要求の応答を無効にする
      return { ...initialComparisonState, requestId: state.requestId + 1 };
  }
}

/**
 * 結果が古いか(要件 11.8)。実行時の共有値と現在の値が、比較の結果に効く項目(ソース・入力の方式と内容・上限の指定)で異なれば古い。
 * スナップショットが無ければ古くない。
 */
export function isResultStale(snapshot: WorkspaceValues | null, current: WorkspaceValues): boolean {
  if (snapshot === null) {
    return false;
  }
  return (
    snapshot.source !== current.source ||
    snapshot.input.mode !== current.input.mode ||
    snapshot.input.text !== current.input.text ||
    snapshot.limit.text !== current.limit.text ||
    snapshot.limit.unlimited !== current.limit.unlimited
  );
}

/** 比較の入力として読む共有値の口(desktop-gui の WorkspaceStore が満たす) */
export type ComparisonWorkspace = { getSnapshot(): WorkspaceValues };

export type ComparisonControllerDeps = {
  backend: ComparisonBackend;
  workspace: ComparisonWorkspace;
};

/**
 * 画面の外で保持する比較の選択(方式のチェックと繰り返し回数の入力欄)。タブ切替で画面が作り直されても既定に戻らないよう、
 * 画面より長く生きるコントローラが持つ。デバッガの状態には触れない。
 */
export type ComparisonForm = {
  /** 最適化インタプリタを比較に含めるか */
  optimized: boolean;
  /** WASM コンパイル実行を比較に含めるか */
  wasm: boolean;
  /** 繰り返し回数の入力欄の文字列(不正値も含む)。null は利用者が未編集で、取得した既定の回数を使う */
  repetitionsText: string | null;
};

export const initialComparisonForm: ComparisonForm = { optimized: true, wasm: true, repetitionsText: null };

export type ComparisonFormStore = {
  getState(): ComparisonForm;
  /** 状態の変化を購読する。解除関数を返す */
  subscribe(listener: () => void): () => void;
  /** 指定した項目だけを変える(値が変わったときだけ通知する) */
  update(patch: Partial<ComparisonForm>): void;
};

export function createComparisonFormStore(): ComparisonFormStore {
  let form: ComparisonForm = initialComparisonForm;
  const listeners = new Set<() => void>();
  return {
    getState: () => form,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    update(patch) {
      const next = { ...form, ...patch };
      if (next.optimized === form.optimized && next.wasm === form.wasm && next.repetitionsText === form.repetitionsText) {
        return;
      }
      form = next;
      for (const listener of [...listeners]) {
        listener();
      }
    },
  };
}

export type ComparisonController = {
  getState(): ComparisonState;
  /** 画面の外で保持する方式と繰り返し回数 */
  form: ComparisonFormStore;
  /** 状態の変化を購読する。解除関数を返す */
  subscribe(listener: () => void): () => void;
  /**
   * 現在の共有値と選択から要求を作り、比較を実行する。送信して応答を反映したら true。
   * 実行中・要求を作れない(無効な選択)場合は送らず false(状態は変えない)。
   */
  run(selection: Selection, info: ComparisonInfoDto): Promise<boolean>;
  /** 結果を捨てる。実行中の応答は届いても反映されない */
  discard(): void;
};

export function createComparisonController({ backend, workspace }: ComparisonControllerDeps): ComparisonController {
  let state: ComparisonState = initialComparisonState;
  const listeners = new Set<() => void>();
  const form = createComparisonFormStore();

  const dispatch = (action: ComparisonAction): void => {
    const next = comparisonReducer(state, action);
    if (next === state) {
      return;
    }
    state = next;
    for (const listener of [...listeners]) {
      listener();
    }
  };

  return {
    getState: () => state,
    form,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    async run(selection, info) {
      if (state.phase === "running") {
        return false;
      }
      const current = workspace.getSnapshot();
      const derived = deriveRequest(current, selection, info);
      if (!derived.ok) {
        return false;
      }
      const requestId = state.requestId + 1;
      dispatch({
        type: "runStarted",
        requestId,
        snapshot: { source: current.source, input: { ...current.input }, limit: { ...current.limit } },
      });
      try {
        const result = await backend.run(derived.request);
        dispatch({ type: "runSucceeded", requestId, result });
      } catch (rejection: unknown) {
        dispatch({ type: "runFailed", requestId, error: normalizeApiError(rejection) });
      }
      return true;
    },
    discard() {
      dispatch({ type: "discarded" });
    },
  };
}
