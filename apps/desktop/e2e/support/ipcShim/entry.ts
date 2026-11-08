// ブラウザ側の IPC シム。製品のスクリプトより前に読み込まれ、すべての invoke を Node の中継へ転送する。
// イベントは `shouldMockEvents` で模擬し、中継へは届かせない。閉じる要求は操作口から発火する。
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { CLOSE_REQUESTED_EVENT } from "../../../src/ipc/backend.ts";
import { type InvokeOutcome, unwrapOutcome } from "./envelope.ts";

declare global {
  interface Window {
    /** フィクスチャが exposeFunction で公開する転送関数 */
    __bfE2eInvoke(cmd: string, payload: unknown): Promise<InvokeOutcome>;
    /** シムが公開する操作口 */
    __bfE2e: { emitCloseRequested(): Promise<void> };
  }
}

mockIPC(
  (cmd, payload) => window.__bfE2eInvoke(cmd, payload ?? null).then(unwrapOutcome),
  { shouldMockEvents: true },
);

window.__bfE2e = {
  emitCloseRequested: () => emit(CLOSE_REQUESTED_EVENT),
};
