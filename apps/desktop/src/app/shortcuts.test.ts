import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  isReloadKey,
  shortcutKeyOf,
  shortcutOf,
  type ShortcutKeyInput,
  useDefaultContextMenuSuppressed,
  useShortcuts,
} from "./shortcuts";

/** 既定は修飾キーなし・繰り返しなし・IME 変換なし */
function key(overrides: Partial<ShortcutKeyInput> & { key: string }): ShortcutKeyInput {
  return {
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    repeat: false,
    isComposing: false,
    keyCode: 0,
    ...overrides,
  };
}

describe("shortcutOf: デバッガの操作ショートカット(要件 14.1・14.2)", () => {
  it("F5 は Run、Shift+F5 は Reset、F6 は Pause、F11 は Step、F9 はブレークポイント切替", () => {
    expect(shortcutOf(key({ key: "F5" }))).toBe("run");
    expect(shortcutOf(key({ key: "F5", shiftKey: true }))).toBe("reset");
    expect(shortcutOf(key({ key: "F6" }))).toBe("pause");
    expect(shortcutOf(key({ key: "F11" }))).toBe("step");
    expect(shortcutOf(key({ key: "F9" }))).toBe("toggleBreakpoint");
  });

  it("Ctrl+N は新規、Ctrl+O は開く、Ctrl+S は保存、Ctrl+Shift+S は名前を付けて保存", () => {
    expect(shortcutOf(key({ key: "n", ctrlKey: true }))).toBe("new");
    expect(shortcutOf(key({ key: "o", ctrlKey: true }))).toBe("open");
    expect(shortcutOf(key({ key: "s", ctrlKey: true }))).toBe("save");
    // Shift を押すと key は大文字になる環境があるため、大小を区別しない
    expect(shortcutOf(key({ key: "S", ctrlKey: true, shiftKey: true }))).toBe("saveAs");
    expect(shortcutOf(key({ key: "s", ctrlKey: true, shiftKey: true }))).toBe("saveAs");
  });
});

describe("shortcutOf: 修飾キーの完全一致(他の組み合わせは割り当てない)", () => {
  it("修飾キーが余計に付くと対応しない", () => {
    expect(shortcutOf(key({ key: "F5", ctrlKey: true }))).toBeNull();
    expect(shortcutOf(key({ key: "F5", altKey: true }))).toBeNull();
    expect(shortcutOf(key({ key: "F9", shiftKey: true }))).toBeNull();
    expect(shortcutOf(key({ key: "s", ctrlKey: true, altKey: true }))).toBeNull();
    expect(shortcutOf(key({ key: "n", ctrlKey: true, shiftKey: true }))).toBeNull();
    expect(shortcutOf(key({ key: "o", ctrlKey: true, metaKey: true }))).toBeNull();
  });

  it("修飾キーなしの文字キーや Ctrl なしの S は対応しない(文字入力を奪わない)", () => {
    expect(shortcutOf(key({ key: "s" }))).toBeNull();
    expect(shortcutOf(key({ key: "n" }))).toBeNull();
    expect(shortcutOf(key({ key: "S", shiftKey: true }))).toBeNull();
    expect(shortcutOf(key({ key: "Enter" }))).toBeNull();
  });
});

describe("shortcutOf: IME 変換中・キーの繰り返し", () => {
  it("IME の変換中は対応しない(変換確定のキーで操作を発火させない)", () => {
    expect(shortcutOf(key({ key: "s", ctrlKey: true, isComposing: true }))).toBeNull();
    expect(shortcutOf(key({ key: "F5", isComposing: true }))).toBeNull();
    expect(shortcutOf(key({ key: "Process", keyCode: 229 }))).toBeNull();
  });

  it("押しっぱなし(繰り返し)では発火しない(Run や保存が連続発火しない)", () => {
    expect(shortcutOf(key({ key: "F5", repeat: true }))).toBeNull();
    expect(shortcutOf(key({ key: "F9", repeat: true }))).toBeNull();
    expect(shortcutOf(key({ key: "s", ctrlKey: true, repeat: true }))).toBeNull();
  });
});

describe("shortcutKeyOf: 割り当て済みのキーの対応(押しっぱなしを含む。既定動作の抑止に使う)", () => {
  it("押しっぱなしでも割り当て済みのキーは対応を返す(既定動作を止めるため)", () => {
    expect(shortcutKeyOf(key({ key: "F5", repeat: true }))).toBe("run");
    expect(shortcutKeyOf(key({ key: "s", ctrlKey: true, repeat: true }))).toBe("save");
    expect(shortcutKeyOf(key({ key: "n", ctrlKey: true, repeat: true }))).toBe("new");
  });

  it("IME の変換中・割り当て外は対応しない(既定動作を抑止しない)", () => {
    expect(shortcutKeyOf(key({ key: "s", ctrlKey: true, isComposing: true }))).toBeNull();
    expect(shortcutKeyOf(key({ key: "F5", ctrlKey: true }))).toBeNull();
  });
});

describe("isReloadKey: リロードに当たるキー(割り当ては無いが既定動作を抑止する)", () => {
  it("Ctrl+R・Ctrl+Shift+R・Ctrl+F5・Ctrl+Shift+F5 はリロード", () => {
    expect(isReloadKey(key({ key: "r", ctrlKey: true }))).toBe(true);
    expect(isReloadKey(key({ key: "R", ctrlKey: true, shiftKey: true }))).toBe(true);
    expect(isReloadKey(key({ key: "F5", ctrlKey: true }))).toBe(true);
    expect(isReloadKey(key({ key: "F5", ctrlKey: true, shiftKey: true }))).toBe(true);
  });

  it("F5 単独・Ctrl なしの R・IME の変換中はリロードとみなさない", () => {
    expect(isReloadKey(key({ key: "F5" }))).toBe(false);
    expect(isReloadKey(key({ key: "r" }))).toBe(false);
    expect(isReloadKey(key({ key: "r", ctrlKey: true, isComposing: true }))).toBe(false);
  });
});

/** ウィンドウへ keydown を送る(`dispatchEvent` の戻り値は preventDefault の有無) */
function dispatch(target: EventTarget, init: KeyboardEventInit & { key: string }): boolean {
  return target.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
}

describe("useShortcuts: 購読の段階と解除", () => {
  it("バブル段階で受けるため、内側で既定動作を取ったキーは扱わない(F9 の二重切替の防止)", () => {
    const handler = vi.fn();
    renderHook(() => useShortcuts(handler));
    const inner = document.createElement("div");
    document.body.appendChild(inner);
    inner.addEventListener("keydown", (event) => event.preventDefault());

    dispatch(inner, { key: "F9" });

    expect(handler).not.toHaveBeenCalled();
    inner.remove();
  });

  it("割り当て済みのキーは既定動作を抑止し、押しっぱなしでは発火しない", () => {
    const handler = vi.fn();
    renderHook(() => useShortcuts(handler));

    const first = dispatch(window, { key: "F5" });
    const repeated = dispatch(window, { key: "F5", repeat: true });

    expect(first).toBe(false);
    expect(repeated).toBe(false);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler.mock.calls[0]?.[0]).toBe("run");
  });

  it("リロードのキーは既定動作を抑止するが、発火はしない", () => {
    const handler = vi.fn();
    renderHook(() => useShortcuts(handler));

    expect(dispatch(window, { key: "r", ctrlKey: true })).toBe(false);
    expect(dispatch(window, { key: "F5", ctrlKey: true })).toBe(false);
    expect(handler).not.toHaveBeenCalled();
  });

  it("割り当て外のキーは既定動作を抑止しない", () => {
    renderHook(() => useShortcuts(vi.fn()));

    expect(dispatch(window, { key: "a" })).toBe(true);
  });

  it("アンマウントで購読を解除する", () => {
    const handler = vi.fn();
    const { unmount } = renderHook(() => useShortcuts(handler));

    unmount();
    dispatch(window, { key: "F5" });

    expect(handler).not.toHaveBeenCalled();
  });
});

describe("useDefaultContextMenuSuppressed: 既定のコンテキストメニューの抑止(未保存の文書を失わないため)", () => {
  it("右クリックの既定メニュー(最新の情報に更新など)を抑止し、アンマウントで解除する", () => {
    const { unmount } = renderHook(() => useDefaultContextMenuSuppressed());

    const shown = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    window.dispatchEvent(shown);
    expect(shown.defaultPrevented).toBe(true);

    unmount();
    const after = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    window.dispatchEvent(after);
    expect(after.defaultPrevented).toBe(false);
  });
});

describe("useDefaultContextMenuSuppressed: 編集可能な要素の上では抑止しない(9.6 レビュー指摘)", () => {
  function contextMenuOn(target: Element): boolean {
    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    target.dispatchEvent(event);
    return event.defaultPrevented;
  }

  it("入力欄・テキスト欄・編集可能な contenteditable の上では既定のメニューを残す", () => {
    renderHook(() => useDefaultContextMenuSuppressed());
    const input = document.createElement("input");
    const textarea = document.createElement("textarea");
    const editable = document.createElement("div");
    editable.setAttribute("contenteditable", "true");
    const inner = document.createElement("span");
    editable.appendChild(inner);
    document.body.append(input, textarea, editable);

    try {
      expect(contextMenuOn(input)).toBe(false);
      expect(contextMenuOn(textarea)).toBe(false);
      expect(contextMenuOn(editable)).toBe(false);
      expect(contextMenuOn(inner)).toBe(false);
    } finally {
      input.remove();
      textarea.remove();
      editable.remove();
    }
  });

  it("読み取り専用の contenteditable と、それ以外の要素の上では既定のメニューを抑止する", () => {
    renderHook(() => useDefaultContextMenuSuppressed());
    const readonly = document.createElement("div");
    readonly.setAttribute("contenteditable", "false");
    const plain = document.createElement("p");
    document.body.append(readonly, plain);

    try {
      expect(contextMenuOn(readonly)).toBe(true);
      expect(contextMenuOn(plain)).toBe(true);
      expect(contextMenuOn(document.body)).toBe(true);
    } finally {
      readonly.remove();
      plain.remove();
    }
  });
});
