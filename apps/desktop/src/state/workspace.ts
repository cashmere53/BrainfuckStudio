// Workspace ストア: 文書(ソース・ファイル名・未保存・置換通知)、入力、命令数上限、テープ半径、出力表示モード、
// ブレークポイント位置を保持する外部ストア。後続画面も同じストアを読み取り専用で共有する。
// React 非依存の購読 API を持ち、`useWorkspace` は useSyncExternalStore で購読する。

import { useSyncExternalStore } from "react";
import { getHelloSource } from "../samples/hello";
import type { InputModel } from "../domain/inputBytes";
import { DEFAULT_MAX_STEPS, DEFAULT_TAPE_RADIUS, MAX_TAPE_RADIUS } from "../domain/limits";
import { DEFAULT_STEP_LIMIT_TEXT, parseStepLimit, type LimitedStepLimit } from "../domain/stepLimit";

export type OutputMode = "text" | "hex";

export type DocumentState = {
  /** 紐づいているファイル名(ディレクトリを含まない)。新規文書は null */
  fileName: string | null;
  /** 保存済みの内容と異なる(未保存の変更がある) */
  dirty: boolean;
  /** 直近の読み込みで不正な UTF-8 を置換したことを通知するか */
  lossyNotice: boolean;
};

export type StepLimitInput = {
  /** 入力欄の文字列(不正値も含む) */
  text: string;
  /** 「上限なし」が選ばれているか */
  unlimited: boolean;
  /** 直前の有効な上限(入力欄が不正でも実行開始に使う) */
  lastValid: LimitedStepLimit;
};

export type WorkspaceState = {
  source: string;
  document: DocumentState;
  input: InputModel;
  limit: StepLimitInput;
  tapeRadius: number;
  outputMode: OutputMode;
  /** ブレークポイントの UTF-16 位置(昇順・重複なし) */
  breakpoints: number[];
  /**
   * 文書の置換(開く・新規)の回数。置換は内容が同じでも進むため、購読側は内容の比較に頼らず置換を検出できる
   * (RunController が置換でセッションを破棄するために使う)。
   */
  documentRevision: number;
};

/** 後続画面がリクエスト導出関数の引数型に使う、ソース・入力・上限の 3 項目 */
export type WorkspaceValues = Pick<WorkspaceState, "source" | "input" | "limit">;

export type ReplaceDocumentArgs = { source: string; fileName: string | null; lossy: boolean };

export type WorkspaceStore = {
  getSnapshot(): WorkspaceState;
  subscribe(listener: () => void): () => void;
  setSource(source: string): void;
  /** 保存済みにする。`fileName` を渡すと紐づけ先のファイル名も更新する */
  markSaved(fileName?: string): void;
  replaceDocument(args: ReplaceDocumentArgs): void;
  newDocument(): void;
  setInput(input: InputModel): void;
  setLimitText(text: string): void;
  setUnlimited(unlimited: boolean): void;
  setTapeRadius(radius: number): void;
  setOutputMode(mode: OutputMode): void;
  setBreakpoints(offsets: readonly number[]): void;
};

function initialState(): WorkspaceState {
  return {
    source: getHelloSource(),
    document: { fileName: null, dirty: false, lossyNotice: false },
    input: { mode: "text", text: "" },
    limit: {
      text: DEFAULT_STEP_LIMIT_TEXT,
      unlimited: false,
      lastValid: { kind: "limited", maxSteps: DEFAULT_MAX_STEPS },
    },
    tapeRadius: DEFAULT_TAPE_RADIUS,
    outputMode: "text",
    breakpoints: [],
    documentRevision: 0,
  };
}

export function createWorkspace(): WorkspaceStore {
  let state = initialState();
  const listeners = new Set<() => void>();

  const commit = (next: WorkspaceState): void => {
    state = next;
    for (const listener of [...listeners]) {
      listener();
    }
  };

  return {
    getSnapshot: () => state,

    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    setSource(source) {
      if (source === state.source) {
        return;
      }
      commit({ ...state, source, document: { ...state.document, dirty: true } });
    },

    markSaved(fileName) {
      const nextName = fileName ?? state.document.fileName;
      if (!state.document.dirty && nextName === state.document.fileName) {
        return;
      }
      commit({ ...state, document: { ...state.document, dirty: false, fileName: nextName } });
    },

    replaceDocument({ source, fileName, lossy }) {
      // 新規/開くでは文書を置換し、ブレークポイントを解除する(6.10)。
      // 置換は「文書の置換」というイベントなので、内容が同じでも無変更ガードは置かず常に通知する。
      commit({
        ...state,
        source,
        document: { fileName, dirty: false, lossyNotice: lossy },
        breakpoints: [],
        documentRevision: state.documentRevision + 1,
      });
    },

    newDocument() {
      // 新規文書も置換イベントのため、既に空の文書でも常に通知する(replaceDocument と同じ方針)
      commit({
        ...state,
        source: "",
        document: { fileName: null, dirty: false, lossyNotice: false },
        breakpoints: [],
        documentRevision: state.documentRevision + 1,
      });
    },

    setInput(input) {
      if (input.mode === state.input.mode && input.text === state.input.text) {
        return;
      }
      commit({ ...state, input });
    },

    setLimitText(text) {
      const parsed = parseStepLimit(text);
      const lastValid = parsed.ok ? parsed.limit : state.limit.lastValid;
      // parseStepLimit は毎回新しいオブジェクトを返すため、参照ではなく値で比較する
      if (text === state.limit.text && lastValid.maxSteps === state.limit.lastValid.maxSteps) {
        return;
      }
      commit({ ...state, limit: { ...state.limit, text, lastValid } });
    },

    setUnlimited(unlimited) {
      if (unlimited === state.limit.unlimited) {
        return;
      }
      commit({ ...state, limit: { ...state.limit, unlimited } });
    },

    setTapeRadius(radius) {
      if (!Number.isInteger(radius) || radius < 0 || radius > MAX_TAPE_RADIUS) {
        throw new RangeError(`tapeRadius must be an integer in 0..=${MAX_TAPE_RADIUS}: ${radius}`);
      }
      if (radius === state.tapeRadius) {
        return;
      }
      commit({ ...state, tapeRadius: radius });
    },

    setOutputMode(mode) {
      if (mode === state.outputMode) {
        return;
      }
      commit({ ...state, outputMode: mode });
    },

    setBreakpoints(offsets) {
      const normalized = [...new Set(offsets)].sort((a, b) => a - b);
      const current = state.breakpoints;
      if (normalized.length === current.length && normalized.every((offset, i) => offset === current[i])) {
        return;
      }
      commit({ ...state, breakpoints: normalized });
    },
  };
}

/** アプリ全体で共有する既定のワークスペースストア */
export const workspaceStore: WorkspaceStore = createWorkspace();

/** 読み取り用フック: ストアの変更で再描画し、現在のスナップショットを返す */
export function useWorkspace(store: WorkspaceStore = workspaceStore): WorkspaceState {
  return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}
