// 比較画面の通信境界: 比較用のコマンド(情報取得・比較実行)の型付き窓口と、テスト用の台本式の偽実装。
// コマンド呼び出し・`{ args }` への包み・拒否値の ApiError 正規化は desktop-gui の `invokeCommand` が行い、ここでは再実装しない。
// デバッガの窓口(`Backend`)には触れない(要件 11.11)。

import type { ApiError } from "../generated/ApiError";
import type { ComparisonInfoDto } from "../generated/ComparisonInfoDto";
import type { ComparisonResultDto } from "../generated/ComparisonResultDto";
import type { RunComparisonRequest } from "../generated/RunComparisonRequest";
import { invokeCommand } from "../ipc/backend";
import { normalizeApiError } from "../ipc/errors";

/** 比較の Tauri コマンド名(Rust の関数名と一致) */
export const COMPARISON_COMMANDS = {
  getInfo: "get_comparison_info",
  run: "run_comparison",
} as const;

/** 比較の窓口。失敗は `ApiError` として reject する */
export interface ComparisonBackend {
  getInfo(): Promise<ComparisonInfoDto>;
  run(req: RunComparisonRequest): Promise<ComparisonResultDto>;
}

/** 本物の IPC 実装。`invokeCommand` を通して `{ args }` の規約と ApiError の正規化を得る */
export const tauriComparisonBackend: ComparisonBackend = {
  getInfo: () => invokeCommand<ComparisonInfoDto>(COMPARISON_COMMANDS.getInfo),
  run: (req) => invokeCommand<ComparisonResultDto>(COMPARISON_COMMANDS.run, req),
};

/** 台本: メソッドごとに値を返すか、例外を投げて失敗を表す */
export type ComparisonScript = {
  getInfo?: () => ComparisonInfoDto | Promise<ComparisonInfoDto>;
  run?: (req: RunComparisonRequest) => ComparisonResultDto | Promise<ComparisonResultDto>;
};

/** 記録された呼び出し */
export type ComparisonFakeCall = { method: keyof ComparisonBackend; args: unknown[] };

export type FakeComparisonBackend = ComparisonBackend & {
  /** 呼び出しの記録(呼ばれた順) */
  readonly calls: readonly ComparisonFakeCall[];
};

/**
 * テスト用の偽バックエンド。通信は行わず台本どおりに返す。台本に無いメソッドは internal で reject する。
 * 台本が投げた値は契約どおり `ApiError` に正規化する。
 */
export function createFakeComparisonBackend(script: ComparisonScript = {}): FakeComparisonBackend {
  const calls: ComparisonFakeCall[] = [];

  const run = async <M extends keyof ComparisonBackend>(
    method: M,
    args: Parameters<ComparisonBackend[M]>,
  ): Promise<unknown> => {
    calls.push({ method, args: [...args] });
    const responder = script[method] as ((...a: unknown[]) => unknown) | undefined;
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
    getInfo: () => run("getInfo", []) as Promise<ComparisonInfoDto>,
    run: (req) => run("run", [req]) as Promise<ComparisonResultDto>,
  };
}
