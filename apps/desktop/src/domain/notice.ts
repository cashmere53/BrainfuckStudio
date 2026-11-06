// 画面の通知の型(要件 10.7・3.7・6.3)。操作の失敗は、バックエンドの拒否 `ApiError` か、
// UI が自分で検出した拒否 `ClientNotice`(ブレークポイント設定不可・入力モード切替の拒否)のどちらかで表す。
// 文面は ipc/errors.ts の `noticeMessageJa` が種類から決める(ここでは文面を持たない)。

import type { ApiError } from "../generated/ApiError";
import type { SwitchRejectReason } from "./inputBytes";

/** UI が検出した拒否。バックエンドには届かない */
export type ClientNotice =
  | { kind: "client"; code: "breakpointRejected" }
  | { kind: "client"; code: "inputSwitchRejected"; reason: SwitchRejectReason };

/** 画面に出す通知。`kind` が `"client"` なら ClientNotice、それ以外は ApiError */
export type Notice = ApiError | ClientNotice;

export function isClientNotice(notice: Notice): notice is ClientNotice {
  return notice.kind === "client";
}
