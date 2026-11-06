// テスト用の台本式の偽バックエンド。実行は行わず、台本どおりの応答を返す(インメモリ)。
// 台本に無いメソッドは ApiError(internal)で reject する(テストの書き漏れを表に出す)。
// 台本の関数が投げた値は契約どおり `ApiError` に正規化して reject する。

import type { ApiError } from "../generated/ApiError";
import type { Backend } from "./backend";
import { normalizeApiError } from "./errors";

/** 購読を除く、コマンドに対応するメソッド名 */
export type CommandMethod = Exclude<keyof Backend, "onCloseRequested">;

type Resolved<M extends keyof Backend> = Awaited<ReturnType<Backend[M]>>;

/** メソッドごとの台本。値を返すか、ApiError を投げて失敗を表す */
export type FakeScript = {
  [M in CommandMethod]?: (...args: Parameters<Backend[M]>) => Resolved<M> | Promise<Resolved<M>>;
};

/** 記録された呼び出し */
export type FakeCall = { method: CommandMethod; args: unknown[] };

export type FakeBackend = Backend & {
  /** 呼び出しの記録(呼ばれた順) */
  readonly calls: readonly FakeCall[];
  /** 登録中の閉じる要求の購読者へ通知する */
  emitCloseRequested(): void;
};

export function createFakeBackend(script: FakeScript = {}): FakeBackend {
  const calls: FakeCall[] = [];
  const closeHandlers = new Set<() => void>();

  // 台本を呼び、失敗は ApiError へ正規化する。台本が無ければ internal で reject する。
  const run = async <M extends CommandMethod>(
    method: M,
    args: Parameters<Backend[M]>,
  ): Promise<Resolved<M>> => {
    calls.push({ method, args: [...args] });
    const responder = script[method] as
      | ((...a: Parameters<Backend[M]>) => Resolved<M> | Promise<Resolved<M>>)
      | undefined;
    if (responder === undefined) {
      const error: ApiError = { kind: "internal", message: `no script for ${method}`, field: null };
      throw error;
    }
    try {
      return await responder(...args);
    } catch (rejection: unknown) {
      throw normalizeApiError(rejection);
    }
  };

  return {
    get calls() {
      return calls;
    },
    loadSource: (...args) => run("loadSource", [args[0]]),
    startSession: (...args) => run("startSession", [args[0]]),
    runBatch: (...args) => run("runBatch", [args[0]]),
    step: (...args) => run("step", [args[0]]),
    reset: (...args) => run("reset", [args[0]]),
    getSnapshot: (...args) => run("getSnapshot", [args[0]]),
    getTapeWindow: (...args) => run("getTapeWindow", [args[0]]),
    setBreakpoints: (...args) => run("setBreakpoints", [args[0]]),
    pause: (...args) => run("pause", [args[0]]),
    newDocument: () => run("newDocument", []),
    openSourceFile: () => run("openSourceFile", []),
    saveSourceFile: (...args) => run("saveSourceFile", [args[0]]),
    setDocumentDirty: (...args) => run("setDocumentDirty", [args[0]]),
    confirmExit: () => run("confirmExit", []),
    onCloseRequested: async (handler) => {
      closeHandlers.add(handler);
      return () => {
        closeHandlers.delete(handler);
      };
    },
    emitCloseRequested() {
      for (const handler of [...closeHandlers]) {
        handler();
      }
    },
  };
}
