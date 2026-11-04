// セッション状態から状態ラベル(未開始/実行中/…)を導く純粋関数。React/Tauri に依存しない。
// 要件 10.2 のラベル集合と design.md「runLabel」に対応する。

import type { SessionStateDto } from "../generated/SessionStateDto";
import type { UiState as ControlUiState } from "./controls";

/** 画面状態。局所 Pause(RunController のフラグ)で止まったかを追加で持つ */
export type UiState = ControlUiState & {
  /**
   * RunController の局所 Pause で停止したか。バックエンドの `lastStop` が `null` でも
   * 「一時停止」と表示するために使う(バッチ間に Pause が届いた場合)。
   */
  stoppedByPause?: boolean;
};

/** ラベル導出に必要なセッションの部分(`SessionStateDto` の部分集合) */
export type RunSession = Pick<SessionStateDto, "lastStop"> & {
  snapshot: Pick<SessionStateDto["snapshot"], "stepsExecuted">;
};

export type RunLabel = "未開始" | "実行中" | "一時停止" | "ブレークポイントで停止" | "ステップ実行後" | "正常終了" | "実行エラー";

/**
 * 状態ラベルを導出する。
 * - セッションなし → 未開始
 * - Running / Pausing → 実行中(Pausing は停止が確定するまで実行中とみなす)
 * - Failed → 実行エラー
 * - Halted / Idle: 局所 Pause なら一時停止。それ以外は `lastStop` で分類し、
 *   `null` のときは実行数が 0 なら未開始、1 以上ならステップ実行後
 */
export function deriveRunLabel(ui: UiState, session: RunSession | null): RunLabel {
  switch (ui.phase) {
    case "noSession":
      return "未開始";
    case "running":
    case "pausing":
      return "実行中";
    case "failed":
      return "実行エラー";
    case "idle":
    case "halted":
      break;
  }
  if (session === null) {
    return "未開始";
  }
  if (ui.stoppedByPause === true) {
    return "一時停止";
  }
  switch (session.lastStop) {
    case "halted":
      return "正常終了";
    case "breakpoint":
      return "ブレークポイントで停止";
    case "cancelled":
    case "batchReached":
      return "一時停止";
    case null:
      return session.snapshot.stepsExecuted === 0 ? "未開始" : "ステップ実行後";
  }
}
