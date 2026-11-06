// バックエンドの型付き窓口。コマンドごとに型付けした `Backend` インタフェースと、本物の IPC 実装 `tauriBackend`。
// 引数は `{ args }` の 1 個にまとめる規約(Rust 側の `args` 引数と一致)。拒否値は `ApiError` に正規化する。
// `invokeCommand<T>()` は比較画面のバックエンドも再利用するため、コマンド名は任意の文字列を受け付ける。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { BreakpointsDto } from "../generated/BreakpointsDto";
import type { LoadSourceRequest } from "../generated/LoadSourceRequest";
import type { LoadSourceResult } from "../generated/LoadSourceResult";
import type { OpenFileResult } from "../generated/OpenFileResult";
import type { PauseRequest } from "../generated/PauseRequest";
import type { RunBatchRequest } from "../generated/RunBatchRequest";
import type { SaveFileRequest } from "../generated/SaveFileRequest";
import type { SaveFileResult } from "../generated/SaveFileResult";
import type { SessionStartedDto } from "../generated/SessionStartedDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { SessionViewRequest } from "../generated/SessionViewRequest";
import type { SetBreakpointsRequest } from "../generated/SetBreakpointsRequest";
import type { StartSessionRequest } from "../generated/StartSessionRequest";
import type { StepResultDto } from "../generated/StepResultDto";
import type { TapeWindowDto } from "../generated/TapeWindowDto";
import type { TapeWindowRequest } from "../generated/TapeWindowRequest";
import { normalizeApiError } from "./errors";

/** フロントから呼ぶコマンド名(Rust の関数名と一致。contract.test.ts が names.rs・ケイパビリティと比べる) */
export const COMMANDS = {
  loadSource: "load_source",
  startSession: "start_session",
  runBatch: "run_batch",
  step: "step",
  reset: "reset",
  getSnapshot: "get_snapshot",
  getTapeWindow: "get_tape_window",
  setBreakpoints: "set_breakpoints",
  pause: "pause",
  newDocument: "new_document",
  openSourceFile: "open_source_file",
  saveSourceFile: "save_source_file",
  setDocumentDirty: "set_document_dirty",
  confirmExit: "confirm_exit",
} as const;

/** ウィンドウを閉じる要求のイベント名(Rust の `window::CLOSE_REQUESTED_EVENT` と一致) */
export const CLOSE_REQUESTED_EVENT = "close-requested";

/** バックエンドの窓口。失敗は `ApiError` として reject する */
export interface Backend {
  loadSource(req: LoadSourceRequest): Promise<LoadSourceResult>;
  startSession(req: StartSessionRequest): Promise<SessionStartedDto>;
  runBatch(req: RunBatchRequest): Promise<BatchResultDto>;
  step(req: SessionViewRequest): Promise<StepResultDto>;
  reset(req: SessionViewRequest): Promise<SessionStateDto>;
  getSnapshot(req: SessionViewRequest): Promise<SessionStateDto>;
  getTapeWindow(req: TapeWindowRequest): Promise<TapeWindowDto>;
  setBreakpoints(req: SetBreakpointsRequest): Promise<BreakpointsDto>;
  pause(req: PauseRequest): Promise<void>;
  newDocument(): Promise<void>;
  openSourceFile(): Promise<OpenFileResult>;
  saveSourceFile(req: SaveFileRequest): Promise<SaveFileResult>;
  setDocumentDirty(dirty: boolean): Promise<void>;
  confirmExit(): Promise<void>;
  /** 閉じる要求の購読。解除関数を返す */
  onCloseRequested(handler: () => void): Promise<() => void>;
}

/**
 * コマンドを呼び、拒否値を `ApiError` に正規化して reject する共有ヘルパ。
 * `args` を渡すと `{ args }` の 1 個にまとめ、省略時は引数を渡さない。
 */
export async function invokeCommand<T>(name: string, args?: object): Promise<T> {
  try {
    return await invoke<T>(name, args === undefined ? undefined : { args });
  } catch (rejection: unknown) {
    throw normalizeApiError(rejection);
  }
}

/** 本物の IPC 実装(Tauri の invoke / listen)。 */
export const tauriBackend: Backend = {
  loadSource: (req) => invokeCommand(COMMANDS.loadSource, req),
  startSession: (req) => invokeCommand(COMMANDS.startSession, req),
  runBatch: (req) => invokeCommand(COMMANDS.runBatch, req),
  step: (req) => invokeCommand(COMMANDS.step, req),
  reset: (req) => invokeCommand(COMMANDS.reset, req),
  getSnapshot: (req) => invokeCommand(COMMANDS.getSnapshot, req),
  getTapeWindow: (req) => invokeCommand(COMMANDS.getTapeWindow, req),
  setBreakpoints: (req) => invokeCommand(COMMANDS.setBreakpoints, req),
  pause: (req) => invokeCommand<void>(COMMANDS.pause, req),
  newDocument: () => invokeCommand<void>(COMMANDS.newDocument),
  openSourceFile: () => invokeCommand(COMMANDS.openSourceFile),
  saveSourceFile: (req) => invokeCommand(COMMANDS.saveSourceFile, req),
  setDocumentDirty: (dirty) => invokeCommand<void>(COMMANDS.setDocumentDirty, { dirty }),
  confirmExit: () => invokeCommand<void>(COMMANDS.confirmExit),
  onCloseRequested: async (handler) => {
    try {
      return await listen<void>(CLOSE_REQUESTED_EVENT, () => {
        handler();
      });
    } catch (rejection: unknown) {
      throw normalizeApiError(rejection);
    }
  },
};
