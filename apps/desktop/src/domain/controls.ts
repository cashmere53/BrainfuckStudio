// 画面状態から各操作の有効/無効を導く真理値表。React/Tauri に依存しない。
// 表は design.md「操作の有効/無効」と要件 4.8 / 4.10 / 4.11 / 5.10 / 14.3 に対応する。

/** 画面状態(セッションの有無と実行の段階) */
export type UiPhase = "noSession" | "idle" | "running" | "pausing" | "halted" | "failed";

export type UiState = {
  phase: UiPhase;
  /** 16 進入力が有効か(テキスト入力は常に有効)。セッション未開始時の Run/Step に効く */
  inputValid: boolean;
};

export type Controls = {
  run: boolean;
  pause: boolean;
  step: boolean;
  reset: boolean;
  /**
   * ソース編集・入力変更・命令数上限・ファイル操作・ブレークポイント切替。
   * 実行中・一時停止処理中は無効(要件 5.10)。
   */
  edit: boolean;
};

/**
 * 画面状態から操作の有効/無効を導く。
 * - Run / Step: セッション未開始なら入力が有効なときのみ。Idle は常に有効。停止状態・実行中・一時停止処理中は無効
 * - Pause: 実行中のみ
 * - Reset: セッションがあり実行中・一時停止処理中以外
 * - edit: 実行中・一時停止処理中以外
 */
export function availability(ui: UiState): Controls {
  switch (ui.phase) {
    case "noSession":
      return { run: ui.inputValid, pause: false, step: ui.inputValid, reset: false, edit: true };
    case "idle":
      return { run: true, pause: false, step: true, reset: true, edit: true };
    case "running":
      return { run: false, pause: true, step: false, reset: false, edit: false };
    case "pausing":
      return { run: false, pause: false, step: false, reset: false, edit: false };
    case "halted":
    case "failed":
      return { run: false, pause: false, step: false, reset: true, edit: true };
  }
}
