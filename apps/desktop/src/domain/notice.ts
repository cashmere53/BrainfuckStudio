// 画面の通知の型(要件 10.7・3.7・6.3)。操作の失敗は、バックエンドの拒否 `ApiError` か、
// UI が自分で検出した拒否 `ClientNotice`(ブレークポイント設定不可・入力モード切替の拒否)のどちらかで表す。
// 文面は ipc/errors.ts の `noticeMessageJa` が種類から決める(ここでは文面を持たない)。

import type { ApiError } from "../generated/ApiError";
import type { SwitchRejectReason } from "./inputBytes";

/** UI が検出した拒否。バックエンドには届かない */
export type ClientNotice =
  | { kind: "client"; code: "breakpointRejected" }
  | { kind: "client"; code: "inputSwitchRejected"; reason: SwitchRejectReason }
  /** 開いたファイルの不正な UTF-8 を置換文字へ置き換えて読み込んだ(成功の通知。要件 2.12) */
  | { kind: "client"; code: "lossyReplaced" };

/** 画面に出す通知。`kind` が `"client"` なら ClientNotice、それ以外は ApiError */
export type Notice = ApiError | ClientNotice;

export function isClientNotice(notice: Notice): notice is ClientNotice {
  return notice.kind === "client";
}

/** 通知の重要度。error は操作の失敗、info は成功・情報(見出しを「通知」として分ける。要件 2.12) */
export type NoticeSeverity = "error" | "info";

export function noticeSeverityOf(notice: Notice): NoticeSeverity {
  return isClientNotice(notice) && notice.code === "lossyReplaced" ? "info" : "error";
}
