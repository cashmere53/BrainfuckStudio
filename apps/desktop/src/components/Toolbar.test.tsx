import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { availability, type UiState } from "../domain/controls";
import { Toolbar, type ToolbarProps } from "./Toolbar";

function handlers(): Omit<ToolbarProps, "controls"> {
  return {
    onRun: vi.fn(),
    onPause: vi.fn(),
    onStep: vi.fn(),
    onReset: vi.fn(),
    onNew: vi.fn(),
    onOpen: vi.fn(),
    onSave: vi.fn(),
    onSaveAs: vi.fn(),
    onClearBreakpoints: vi.fn(),
  };
}

function isDisabled(element: HTMLElement): boolean {
  return (element as HTMLButtonElement).disabled;
}

function renderFor(ui: UiState) {
  const props = { controls: availability(ui), ...handlers() };
  render(<Toolbar {...props} />);
  return props;
}

describe("Toolbar の有効/無効(要件 14.3)", () => {
  it("未開始(入力が有効)では Run と Step と編集系だけ有効", () => {
    renderFor({ phase: "noSession", inputValid: true });
    expect(isDisabled(screen.getByRole("button", { name: /^Run/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^Step/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^Pause/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^Reset/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^新規/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^開く/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^保存/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^名前を付けて保存/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /全ブレークポイント解除/ }))).toBe(false);
  });

  it("未開始で 16 進入力が不正なら Run と Step は無効、編集系は有効", () => {
    renderFor({ phase: "noSession", inputValid: false });
    expect(isDisabled(screen.getByRole("button", { name: /^Run/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^Step/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^新規/ }))).toBe(false);
  });

  it("停止中(Idle)は Run・Step・Reset が有効で Pause は無効", () => {
    renderFor({ phase: "idle", inputValid: true });
    expect(isDisabled(screen.getByRole("button", { name: /^Run/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^Step/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^Reset/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^Pause/ }))).toBe(true);
  });

  it("実行中は Pause だけ有効で、ファイル操作と BP 解除は無効", () => {
    renderFor({ phase: "running", inputValid: true });
    expect(isDisabled(screen.getByRole("button", { name: /^Pause/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^Run/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^Step/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^Reset/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^新規/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^開く/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^保存/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^名前を付けて保存/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /全ブレークポイント解除/ }))).toBe(true);
  });

  it("一時停止処理中は全ボタンが無効", () => {
    renderFor({ phase: "pausing", inputValid: true });
    for (const button of screen.getAllByRole("button")) {
      expect(isDisabled(button)).toBe(true);
    }
  });

  it("停止状態(Halted/Failed)は Reset と編集系が有効で Run/Pause/Step は無効", () => {
    renderFor({ phase: "halted", inputValid: true });
    expect(isDisabled(screen.getByRole("button", { name: /^Run/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^Pause/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^Step/ }))).toBe(true);
    expect(isDisabled(screen.getByRole("button", { name: /^Reset/ }))).toBe(false);
    expect(isDisabled(screen.getByRole("button", { name: /^保存/ }))).toBe(false);
  });
});

describe("Toolbar のラベルとショートカット表示(要件 14.1, 14.2)", () => {
  it("Run・Pause・Step・Reset は文字ラベルとショートカットを含む", () => {
    renderFor({ phase: "idle", inputValid: true });
    screen.getByRole("button", { name: "Run (F5)" });
    screen.getByRole("button", { name: "Pause (F6)" });
    screen.getByRole("button", { name: "Step (F11)" });
    screen.getByRole("button", { name: "Reset (Shift+F5)" });
  });

  it("ファイル操作のショートカットがラベルに含まれる", () => {
    renderFor({ phase: "noSession", inputValid: true });
    screen.getByRole("button", { name: "新規 (Ctrl+N)" });
    screen.getByRole("button", { name: "開く (Ctrl+O)" });
    screen.getByRole("button", { name: "保存 (Ctrl+S)" });
    screen.getByRole("button", { name: "名前を付けて保存 (Ctrl+Shift+S)" });
  });
});

describe("Toolbar のクリック(要件 14.1)", () => {
  it("各ボタンは対応するハンドラだけを呼ぶ", () => {
    const props = renderFor({ phase: "idle", inputValid: true });
    fireEvent.click(screen.getByRole("button", { name: "Run (F5)" }));
    fireEvent.click(screen.getByRole("button", { name: "Step (F11)" }));
    fireEvent.click(screen.getByRole("button", { name: "Reset (Shift+F5)" }));
    fireEvent.click(screen.getByRole("button", { name: "新規 (Ctrl+N)" }));
    fireEvent.click(screen.getByRole("button", { name: "開く (Ctrl+O)" }));
    fireEvent.click(screen.getByRole("button", { name: "保存 (Ctrl+S)" }));
    fireEvent.click(screen.getByRole("button", { name: "名前を付けて保存 (Ctrl+Shift+S)" }));
    fireEvent.click(screen.getByRole("button", { name: /全ブレークポイント解除/ }));
    expect(props.onRun).toHaveBeenCalledTimes(1);
    expect(props.onStep).toHaveBeenCalledTimes(1);
    expect(props.onReset).toHaveBeenCalledTimes(1);
    expect(props.onNew).toHaveBeenCalledTimes(1);
    expect(props.onOpen).toHaveBeenCalledTimes(1);
    expect(props.onSave).toHaveBeenCalledTimes(1);
    expect(props.onSaveAs).toHaveBeenCalledTimes(1);
    expect(props.onClearBreakpoints).toHaveBeenCalledTimes(1);
    expect(props.onPause).not.toHaveBeenCalled();
  });

  it("無効なボタンはクリックしてもハンドラを呼ばない", () => {
    const props = renderFor({ phase: "running", inputValid: true });
    fireEvent.click(screen.getByRole("button", { name: "Run (F5)" }));
    expect(props.onRun).not.toHaveBeenCalled();
  });

  it("実行中に Pause をクリックすると onPause だけが 1 回呼ばれる", () => {
    const props = renderFor({ phase: "running", inputValid: true });
    fireEvent.click(screen.getByRole("button", { name: "Pause (F6)" }));
    expect(props.onPause).toHaveBeenCalledTimes(1);
    expect(props.onRun).not.toHaveBeenCalled();
    expect(props.onStep).not.toHaveBeenCalled();
    expect(props.onReset).not.toHaveBeenCalled();
    expect(props.onNew).not.toHaveBeenCalled();
    expect(props.onOpen).not.toHaveBeenCalled();
    expect(props.onSave).not.toHaveBeenCalled();
    expect(props.onSaveAs).not.toHaveBeenCalled();
    expect(props.onClearBreakpoints).not.toHaveBeenCalled();
  });

  // 各ボタンを「そのボタンが有効になる状態」でクリックしたとき、対応するハンドラだけが 1 回呼ばれる
  const cases: ReadonlyArray<{
    label: string;
    name: RegExp;
    phase: UiState["phase"];
    handler: keyof Omit<ToolbarProps, "controls">;
  }> = [
    { label: "Run", name: /^Run/, phase: "idle", handler: "onRun" },
    { label: "Pause", name: /^Pause/, phase: "running", handler: "onPause" },
    { label: "Step", name: /^Step/, phase: "idle", handler: "onStep" },
    { label: "Reset", name: /^Reset/, phase: "idle", handler: "onReset" },
    { label: "新規", name: /^新規/, phase: "noSession", handler: "onNew" },
    { label: "開く", name: /^開く/, phase: "noSession", handler: "onOpen" },
    { label: "保存", name: /^保存/, phase: "noSession", handler: "onSave" },
    { label: "名前を付けて保存", name: /^名前を付けて保存/, phase: "noSession", handler: "onSaveAs" },
    { label: "全ブレークポイント解除", name: /全ブレークポイント解除/, phase: "noSession", handler: "onClearBreakpoints" },
  ];

  it.each(cases)("$label は有効な状態($phase)でクリックすると $handler だけを 1 回呼ぶ", ({ name, phase, handler }) => {
    const props = renderFor({ phase, inputValid: true });
    const button = screen.getByRole("button", { name });
    expect(isDisabled(button)).toBe(false);
    fireEvent.click(button);
    for (const key of Object.keys(handlers()) as Array<keyof Omit<ToolbarProps, "controls">>) {
      if (key === handler) {
        expect(props[key], key).toHaveBeenCalledTimes(1);
      } else {
        expect(props[key], key).not.toHaveBeenCalled();
      }
    }
  });
});
