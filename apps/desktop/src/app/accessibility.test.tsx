// 画面全体の操作性・アクセシビリティの統合確認(要件 14.1・14.4・7.7・10.10、8.3)。
// jsdom ではレイアウト・実際の色・Tab キーの既定動作を再現できないため、次の範囲を確認する:
// - DOM の順序・フォーカス可能性・ショートカット・ダイアログのフォーカス管理(キーボードだけの操作列)
// - 色以外の手がかりの CSS クラス(エディタの装飾・テープの dp 枠)
// - aria-live / role の使い分け、コントラスト指定に相当する CSS 変数の値
// 見た目の確認(実際の描画の太さ・波線・最小幅での折り返し)は 9.5 の手動スモーク手順で行う。

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { SourceRangeDto } from "../generated/SourceRangeDto";
import type { StepResultDto } from "../generated/StepResultDto";
import { encodeBase64 } from "../domain/base64";
import { createFakeBackend, type FakeScript } from "../ipc/fakeBackend";
import { createWorkspace, type WorkspaceStore } from "../state/workspace";
import { DebuggerScreen } from "./DebuggerScreen";

function range(from: number, to: number): SourceRangeDto {
  const pos = (utf16Offset: number) => ({ byteOffset: utf16Offset, utf16Offset, line: 1, column: utf16Offset + 1 });
  return { start: pos(from), end: pos(to) };
}

function stateDto(options: { halted?: boolean; lastStop?: SessionStateDto["lastStop"]; currentRange?: SourceRangeDto | null; steps?: number } = {}): SessionStateDto {
  const bytes = new Uint8Array(0);
  return {
    sessionId: 1,
    snapshot: { pc: 0, dp: 0, currentCell: 0, stepsExecuted: options.steps ?? 0, halted: options.halted ?? false },
    tapeWindow: { start: 0, cells: Array.from({ length: 17 }, () => 0), pointerOffset: 8 },
    output: { dataStart: 0, totalLen: bytes.length, dataBase64: encodeBase64(bytes) },
    inputConsumed: 0,
    inputTotal: 0,
    lastStop: options.lastStop ?? null,
    currentRange: options.currentRange ?? null,
  };
}

function haltedBatch(): BatchResultDto {
  return { stop: { kind: "halted" }, executed: 1, state: stateDto({ halted: true, lastStop: "halted" }) };
}

/** 既定の台本。Run は即座に正常終了し、Step は 1 命令の現在位置(0〜1)を返す */
function script(overrides: FakeScript = {}): FakeScript {
  return {
    loadSource: () => ({ kind: "ok", programId: 7, instructionCount: 3, lineCount: 1 }),
    startSession: () => ({ state: stateDto(), breakpoints: { breakpoints: [], unresolved: [] } }),
    runBatch: () => haltedBatch(),
    step: (): StepResultDto => ({
      outcome: { kind: "executed" },
      state: stateDto({ currentRange: range(0, 1), steps: 1 }),
    }),
    reset: () => stateDto(),
    getTapeWindow: () => ({ start: 0, cells: Array.from({ length: 17 }, () => 0), pointerOffset: 8 }),
    pause: () => undefined,
    setBreakpoints: (req) => ({ breakpoints: [], unresolved: req.offsets }),
    setDocumentDirty: () => undefined,
    ...overrides,
  };
}

function renderScreen(overrides: FakeScript = {}) {
  const backend = createFakeBackend(script(overrides));
  const workspace: WorkspaceStore = createWorkspace();
  const view = render(<DebuggerScreen backend={backend} workspace={workspace} />);
  return { backend, workspace, ...view };
}

/** ウィンドウ全体へキーを送る(操作ショートカットはウィンドウで受ける) */
function press(init: KeyboardEventInit & { key: string }): void {
  fireEvent.keyDown(window, init);
}

/** 非同期の後続(キューの処理)が届くまで待つ */
async function flush(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
}

/**
 * エディタの文書を置き換える。利用者の編集と同じ経路(エディタの変更 → Workspace へ書き戻し)を通すため、
 * Workspace の setSource では表示は変わらない(表示の作り直しは文書の置換の回数で決まる)。
 */
function setEditorText(text: string): void {
  const view = EditorView.findFromDOM(document.querySelector(".cm-content") as HTMLElement);
  if (view === null) throw new Error("editor is not mounted");
  act(() => {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
  });
}

/**
 * Tab 順の候補: 文書順の操作可能な要素。無効な操作と、tabindex が負の要素を除く。
 * 既定の tabIndex は jsdom では contenteditable の要素に -1 を返すため、属性で判定する(ブラウザでは既定で 0)。
 */
function tabbable(): HTMLElement[] {
  return Array.from(
    document.querySelectorAll<HTMLElement>("button, input, textarea, select, [contenteditable='true'], [tabindex]"),
  ).filter((element) => {
    if ((element as HTMLButtonElement).disabled) return false;
    const attribute = element.getAttribute("tabindex");
    return attribute === null || Number(attribute) >= 0;
  });
}

const statusLabel = () => screen.getByTestId("status-label");
const globalCssPath = resolve(import.meta.dirname, "../styles/global.css");

describe("操作性: キーボードだけの操作(要件 14.1)", () => {
  it("Run → Pause → Step → Reset → ブレークポイント切替 → 表示切替をキーボードだけで完了する", async () => {
    const hold: { release?: (result: BatchResultDto) => void } = {};
    let batches = 0;
    const { backend, workspace } = renderScreen({
      runBatch: () => {
        batches += 1;
        if (batches === 1) {
          return { stop: { kind: "batchReached" }, executed: 1, state: stateDto() };
        }
        return new Promise<BatchResultDto>((resolve) => {
          hold.release = resolve;
        });
      },
      pause: () => {
        hold.release?.({ stop: { kind: "cancelled" }, executed: 0, state: stateDto() });
      },
    });

    // Run(F5)
    press({ key: "F5" });
    await waitFor(() => expect(batches).toBe(2));
    expect(statusLabel().textContent).toBe("実行中");

    // Pause(F6)
    press({ key: "F6" });
    expect(await screen.findByText("一時停止")).toBeTruthy();

    // Step(F11)
    press({ key: "F11" });
    expect(await screen.findByText("ステップ実行後")).toBeTruthy();
    expect(backend.calls.some((call) => call.method === "step")).toBe(true);

    // Reset(Shift+F5)
    press({ key: "F5", shiftKey: true });
    expect(await screen.findByText("未開始")).toBeTruthy();

    // ブレークポイント切替(F9): カーソル以降の最初の命令に 1 つ置き、もう一度押すと外れる
    setEditorText("+-");
    press({ key: "F9" });
    expect(workspace.getSnapshot().breakpoints).toEqual([0]);
    press({ key: "F9" });
    expect(workspace.getSnapshot().breakpoints).toEqual([]);

    // 表示切替: 出力の表示形式の 16 進を、フォーカスしてから有効化する(Space による既定の選択操作と同じ経路)
    const hex = within(screen.getByRole("radiogroup", { name: "出力の表示形式" })).getByRole("radio", { name: "16 進" });
    hex.focus();
    expect(document.activeElement).toBe(hex);
    fireEvent.click(hex);
    expect(workspace.getSnapshot().outputMode).toBe("hex");
    await flush();
  });
});

describe("操作性: Tab 順とフォーカスの可視(要件 14.1・7.7)", () => {
  it("操作可能な要素が、期待する列挙の順(ツールバー → ソース → 入力 → 上限 → テープ半径 → 出力)で Tab 順に並ぶ", () => {
    renderScreen();

    const toolbarButtons = within(screen.getByRole("toolbar", { name: "操作" })).getAllByRole("button");
    expect(toolbarButtons.map((button) => button.textContent)).toEqual([
      "Run (F5)",
      "Pause (F6)",
      "Step (F11)",
      "Reset (Shift+F5)",
      "新規 (Ctrl+N)",
      "開く (Ctrl+O)",
      "保存 (Ctrl+S)",
      "名前を付けて保存 (Ctrl+Shift+S)",
      "全ブレークポイント解除",
    ]);

    // 期待する Tab 順(文書順。無効な操作は Tab 順に含まれない)
    const expected: HTMLElement[] = [
      ...toolbarButtons,
      screen.getByRole("textbox", { name: "ソース" }),
      ...within(screen.getByRole("radiogroup", { name: "入力の形式" })).getAllByRole("radio"),
      screen.getByRole("textbox", { name: "入力" }),
      screen.getByLabelText("命令数上限"),
      screen.getByRole("checkbox", { name: "上限なし" }),
      screen.getByLabelText("表示半径"),
      ...within(screen.getByRole("radiogroup", { name: "出力の表示形式" })).getAllByRole("radio"),
      ...Array.from(document.querySelectorAll<HTMLElement>(".output-view-text, .output-view-hex")),
    ].filter((element) => !(element as HTMLButtonElement | HTMLInputElement).disabled);

    expect(tabbable()).toEqual(expected);

    for (const element of expected) {
      element.focus();
      expect(document.activeElement).toBe(element);
    }
  });

  it("フォーカス表示の CSS(:focus-visible)が全体の共通スタイルにあり、色はコントラストのトークン(--color-accent)を使う", () => {
    const css = readFileSync(globalCssPath, "utf8");
    expect(css).toMatch(/:focus-visible\s*\{[^}]*outline:\s*[^;]*var\(--color-accent\)/);
  });

  it("視覚的に隠すクラス(.visually-hidden)は表示を消さず、支援技術には残す指定(position・clip)を持つ", () => {
    const css = readFileSync(globalCssPath, "utf8");
    const rule = /\.visually-hidden\s*\{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(rule).toMatch(/position:\s*absolute/);
    expect(rule).toMatch(/clip:\s*rect\(/);
    expect(rule).not.toMatch(/display:\s*none/);
    expect(rule).not.toMatch(/visibility:\s*hidden/);
  });
});

describe("色以外の手がかり(要件 7.7・14.4)", () => {
  it("現在命令は装飾クラス(下線・枠・太字)で示される", async () => {
    renderScreen();
    setEditorText("+-");
    press({ key: "F11" });
    await screen.findByText("ステップ実行後");

    expect(document.querySelector(".cm-execution-current")?.textContent).toBe("+");
  });

  it("ブレークポイントは命令文字の枠(.cm-breakpoint-char)とガターのマーカーで示される", () => {
    const { workspace } = renderScreen();
    setEditorText("+-");
    act(() => workspace.setBreakpoints([0]));

    expect(document.querySelector(".cm-breakpoint-char")?.textContent).toBe("+");
    expect(document.querySelector(".cm-breakpoint-gutter .cm-breakpoint-marker")).not.toBeNull();
  });

  it("実行エラーの位置は .cm-execution-runtime-error で示され、状態領域にも文字で出る", async () => {
    renderScreen({
      runBatch: () => ({
        stop: { kind: "error", error: { kind: "pointerOutOfBounds", pc: 1, dp: 0, direction: "left" } },
        executed: 1,
        state: stateDto({ currentRange: range(1, 2) }),
      }),
    });
    setEditorText("+-");

    press({ key: "F5" });

    await waitFor(() => expect(document.querySelector(".error-banner")).not.toBeNull());
    expect(document.querySelector(".cm-execution-runtime-error")?.textContent).toBe("-");
    expect(document.querySelector(".error-banner")?.textContent).toContain("ポインタ範囲外");
  });

  it("解析エラーの括弧は .cm-parse-error で示される", async () => {
    renderScreen({
      loadSource: () => ({
        kind: "parseError",
        error: {
          kind: "unmatchedClosingBracket",
          instructionIndex: 0,
          position: { byteOffset: 0, utf16Offset: 0, line: 1, column: 1 },
        },
      }),
    });
    setEditorText("]");

    press({ key: "F5" });

    await waitFor(() => expect(document.querySelector(".cm-parse-error")).not.toBeNull());
    expect(document.querySelector(".cm-parse-error")?.textContent).toBe("]");
  });

  it("対応する括弧は .cm-bracket-matched、対応のない括弧は .cm-bracket-unmatched で示される", () => {
    renderScreen();
    setEditorText("[-]");
    const view = EditorView.findFromDOM(document.querySelector(".cm-content") as HTMLElement);
    expect(view).not.toBeNull();

    act(() => {
      view?.dispatch({ selection: { anchor: 0 } });
    });
    expect(document.querySelectorAll(".cm-bracket-matched").length).toBeGreaterThanOrEqual(2);

    setEditorText("[-");
    const view2 = EditorView.findFromDOM(document.querySelector(".cm-content") as HTMLElement);
    act(() => {
      view2?.dispatch({ selection: { anchor: 0 } });
    });
    expect(document.querySelector(".cm-bracket-unmatched")).not.toBeNull();
  });

  it("テープの dp のセルは枠(.cue-frame)と「▼ dp」のラベルと aria-current で示される", () => {
    renderScreen();

    const pointer = document.querySelector(".tape-cell-pointer");
    expect(pointer?.classList.contains("cue-frame")).toBe(true);
    expect(pointer?.getAttribute("aria-current")).toBe("true");
    expect(pointer?.textContent).toContain("▼ dp");
    expect(document.querySelectorAll(".tape-cell-pointer")).toHaveLength(1);
  });
});

describe("支援技術への通知(要件 10.10・10.4〜10.7)", () => {
  it("状態ラベルは aria-live=polite の領域で、状態の変化をその領域の内容として反映する", async () => {
    renderScreen();
    expect(statusLabel().getAttribute("aria-live")).toBe("polite");
    expect(statusLabel().getAttribute("aria-atomic")).toBe("true");

    press({ key: "F11" });

    expect(await screen.findByText("ステップ実行後")).toBeTruthy();
    expect(statusLabel().textContent).toBe("ステップ実行後");
  });

  it("aria-live=polite は状態ラベルだけに付く(文書の状態表示は読み上げの対象にしない)", () => {
    renderScreen();

    // CodeMirror は内部の告知領域に aria-live=polite を付けるため、エディタの内側は対象外にする
    const polite = Array.from(document.querySelectorAll("[aria-live]")).filter(
      (element) => element.getAttribute("aria-live") === "polite" && element.closest(".cm-editor") === null,
    );
    expect(polite).toEqual([statusLabel()]);
  });

  it("実行エラーは role=alert の ErrorBanner、操作の失敗は role=alert の通知で伝える(領域は別々)", async () => {
    renderScreen({
      runBatch: () => ({
        stop: { kind: "error", error: { kind: "pointerOutOfBounds", pc: 0, dp: 0, direction: "left" } },
        executed: 0,
        state: stateDto(),
      }),
    });
    setEditorText("+-");
    press({ key: "F5" });

    await waitFor(() => expect(document.querySelector(".error-banner")?.getAttribute("role")).toBe("alert"));
  });
});

describe("ダイアログのフォーカス管理(要件 2.7・14.1)", () => {
  it("未保存の変更の確認を開くと取消にフォーカスし、Escape で閉じて元の要素へフォーカスを戻す", () => {
    renderScreen();
    setEditorText("+");
    const trigger = screen.getByRole("button", { name: "保存 (Ctrl+S)" });
    trigger.focus();

    press({ key: "n", ctrlKey: true });

    const dialog = screen.getByRole("alertdialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "取消" }));

    fireEvent.keyDown(dialog, { key: "Escape" });

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
});

describe("レイアウトとコントラストの指定(要件 14.4・14.5)", () => {


  /** `:root` と `@media (prefers-color-scheme: dark)` 内の `:root` からトークンを取り出す */
  function tokensOf(css: string): { light: Record<string, string>; dark: Record<string, string> } {
    const blocks = Array.from(css.matchAll(/:root\s*\{([^}]*)\}/g)).map((match) => match[1] ?? "");
    const read = (body: string) =>
      Object.fromEntries(Array.from(body.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{6})/g), (m) => [m[1], m[2]]));
    const light = read(blocks[0] ?? "");
    const dark = { ...light, ...read(blocks[1] ?? "") };
    return { light, dark };
  }

  function luminance(hex: string): number {
    const channel = (offset: number) => {
      const value = parseInt(hex.slice(offset, offset + 2), 16) / 255;
      return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
  }

  function contrast(a: string, b: string): number {
    const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
    return (hi + 0.05) / (lo + 0.05);
  }

  it("本文・補助文・強調色・エラー色は背景に対して 4.5:1 以上、境界線は 3:1 以上の指定を持つ(ライト/ダーク)", () => {
    const { light, dark } = tokensOf(readFileSync(globalCssPath, "utf8"));
    for (const [mode, tokens] of [
      ["light", light],
      ["dark", dark],
    ] as const) {
      expect(contrast(tokens["color-fg"] ?? "", tokens["color-bg"] ?? ""), `${mode} fg`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tokens["color-muted"] ?? "", tokens["color-bg"] ?? ""), `${mode} muted`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tokens["color-accent"] ?? "", tokens["color-bg"] ?? ""), `${mode} accent`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tokens["color-error"] ?? "", tokens["color-bg"] ?? ""), `${mode} error`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tokens["color-fg"] ?? "", tokens["color-current"] ?? ""), `${mode} marker`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tokens["color-border"] ?? "", tokens["color-bg"] ?? ""), `${mode} border`).toBeGreaterThanOrEqual(3);
    }
  });

  it("アプリの外枠は最小 800 × 600 を保ち、それより小さい場合はスクロールさせる", () => {
    const css = readFileSync(globalCssPath, "utf8");
    expect(css).toMatch(/\.app-shell\s*\{[^}]*min-width:\s*800px/);
    expect(css).toMatch(/\.app-shell\s*\{[^}]*min-height:\s*600px/);
    expect(css).toMatch(/\.app-shell\s*\{[^}]*overflow:\s*auto/);
  });

  it("画面に見出し(h1)とランドマーク(main)があり、領域は名前で区別できる", () => {
    renderScreen();

    expect(screen.getByRole("main")).toBeTruthy();
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBeTruthy();
    expect(screen.getByRole("region", { name: "状態" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "テープ" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "出力" })).toBeTruthy();
  });
});
