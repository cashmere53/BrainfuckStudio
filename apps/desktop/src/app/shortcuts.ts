// デバッガ画面の操作ショートカット(要件 14.1・14.2)。
// - 対応: F5 Run / Shift+F5 Reset / F6 Pause / F11 Step / F9 ブレークポイント切替 / Ctrl+N 新規 / Ctrl+O 開く /
//   Ctrl+S 保存 / Ctrl+Shift+S 名前を付けて保存。Toolbar のラベルと一致させる。
// - 修飾キーは完全一致で判定する(Ctrl+F5 や Alt+S などは割り当てない)。IME 変換中は素通しする。
// - 登録はウィンドウのバブル段階で行う。エディタ内のキーマップが処理したキー(`defaultPrevented`)は無視し、
//   F9 がエディタとグローバルの両方で切り替わる二重切替を防ぐ。
// - 割り当て済みのキーは、押しっぱなし(repeat)でも既定動作(ブラウザのリロード・ページの保存など)を抑止する。
//   操作の発火だけを repeat で止める(連続発火しない)。操作が有効かどうかは呼び出し側が決める。
// - リロードに当たるキー(Ctrl+R・Ctrl+Shift+R・Ctrl+F5・Ctrl+Shift+F5)は割り当て外だが、既定動作を抑止する。
//   リロードは CloseRequested を経ないため、未保存の確認なしで編集内容が失われるのを防ぐ。発火はしない。

import { useEffect, useRef } from "react";

export type ShortcutId = "run" | "pause" | "step" | "reset" | "toggleBreakpoint" | "new" | "open" | "save" | "saveAs";

/** 判定に使う KeyboardEvent の項目(テストで組み立てやすいように部分型にする) */
export type ShortcutKeyInput = Pick<
  KeyboardEvent,
  "key" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey" | "repeat" | "isComposing" | "keyCode"
>;

/** IME の変換中に届くキーコード(`isComposing` が立たない環境の保険) */
const IME_KEY_CODE = 229;

function isImeInput(event: ShortcutKeyInput): boolean {
  return event.isComposing || event.keyCode === IME_KEY_CODE;
}

/**
 * キーの対応(押しっぱなしを含む)。割り当てが無いキー、IME 変換中のキーは null。
 * 既定動作を抑止するかどうかの判定に使う。発火してよいかは `shortcutOf` が決める。
 */
export function shortcutKeyOf(event: ShortcutKeyInput): ShortcutId | null {
  if (isImeInput(event)) return null;
  if (event.altKey || event.metaKey) return null;

  const { key, ctrlKey, shiftKey } = event;
  if (!ctrlKey) {
    if (key === "F5") return shiftKey ? "reset" : "run";
    if (shiftKey) return null;
    switch (key) {
      case "F6":
        return "pause";
      case "F11":
        return "step";
      case "F9":
        return "toggleBreakpoint";
      default:
        return null;
    }
  }

  // Ctrl あり。Shift を押すと key が大文字になる環境があるため、文字は大小を区別しない
  switch (key.toLowerCase()) {
    case "n":
      return shiftKey ? null : "new";
    case "o":
      return shiftKey ? null : "open";
    case "s":
      return shiftKey ? "saveAs" : "save";
    default:
      return null;
  }
}

/** 発火してよいショートカット: 割り当てがあり、押しっぱなし(repeat)でないキー。それ以外は null */
export function shortcutOf(event: ShortcutKeyInput): ShortcutId | null {
  if (event.repeat) return null;
  return shortcutKeyOf(event);
}

/** ブラウザのリロードに当たるキー(割り当ては無い。既定動作だけを抑止する) */
export function isReloadKey(event: ShortcutKeyInput): boolean {
  if (isImeInput(event)) return false;
  if (!event.ctrlKey || event.altKey || event.metaKey) return false;
  const key = event.key.toLowerCase();
  return key === "r" || key === "f5";
}

/**
 * ウィンドウの keydown を購読し、対応するショートカットを `onShortcut` へ渡す。
 * 対応するキー(押しっぱなしを含む)とリロードのキーは `preventDefault` する。
 * 発火は押しっぱなし(repeat)では行わない。操作の有効/無効は `onShortcut` の側で判定する。
 * 最新の `onShortcut` を使う(購読は一度だけ張り、アンマウントで解除する)。
 */
export function useShortcuts(onShortcut: (id: ShortcutId, event: KeyboardEvent) => void): void {
  const latest = useRef(onShortcut);
  useEffect(() => {
    latest.current = onShortcut;
  });

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      // エディタのキーマップなど、内側で処理済みのキーは扱わない(F9 の二重切替の防止)
      if (event.defaultPrevented) return;
      if (isReloadKey(event)) {
        event.preventDefault();
        return;
      }
      const id = shortcutKeyOf(event);
      if (id === null) return;
      event.preventDefault();
      if (event.repeat) return;
      latest.current(id, event);
    };
    window.addEventListener("keydown", listener);
    return () => {
      window.removeEventListener("keydown", listener);
    };
  }, []);
}

/**
 * 編集可能な要素の上か(入力欄・テキスト欄・contenteditable)。ここでは既定のメニュー(切り取り・コピー・貼り付け)を残す。
 * `contenteditable="false"`(読み取り専用のエディタ)は編集可能ではないため含めない。
 */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
  if (!(target instanceof Element)) return false;
  const editable = target.closest("[contenteditable]");
  return editable !== null && editable.getAttribute("contenteditable") !== "false";
}

/**
 * 既定のコンテキストメニュー(WebView2 の「最新の情報に更新」など)を抑止する。
 * 再読み込みに当たる操作は、未保存の確認を経ずに文書を失わせるため、右クリックからも出さない。
 * ただし編集可能な要素の上では抑止しない(CodeMirror の切り取り・コピー・貼り付けを残す)。
 * アンマウントで解除する。
 */
export function useDefaultContextMenuSuppressed(): void {
  useEffect(() => {
    const suppress = (event: MouseEvent) => {
      if (isEditableTarget(event.target)) return;
      event.preventDefault();
    };
    window.addEventListener("contextmenu", suppress);
    return () => {
      window.removeEventListener("contextmenu", suppress);
    };
  }, []);
}
