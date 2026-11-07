import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MAX_INPUT_BYTES } from "../domain/limits";
import type { InputModel } from "../domain/inputBytes";
import { InputPanel, type InputPanelProps } from "./InputPanel";

function renderPanel(input: InputModel, overrides: Partial<InputPanelProps> = {}) {
  const props: InputPanelProps = {
    input,
    onChange: vi.fn(),
    onSwitchRejected: vi.fn(),
    ...overrides,
  };
  render(<InputPanel {...props} />);
  return props;
}

function textArea(): HTMLTextAreaElement {
  return screen.getByRole("textbox", { name: "入力" }) as HTMLTextAreaElement;
}

describe("InputPanel(要件 3.1〜3.8)", () => {
  it("テキスト/16 進のモード切替をラジオボタンで提供し、現在のモードを選択状態にする", () => {
    renderPanel({ mode: "text", text: "A" });
    expect((screen.getByRole("radio", { name: "テキスト" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole("radio", { name: "16 進" }) as HTMLInputElement).checked).toBe(false);
  });

  it("テキストモードの入力変更は UTF-8 の文字列としてそのまま onChange に渡す(要件 3.2)", () => {
    const props = renderPanel({ mode: "text", text: "" });
    fireEvent.change(textArea(), { target: { value: "あ\n" } });
    expect(props.onChange).toHaveBeenCalledWith({ mode: "text", text: "あ\n" });
  });

  it("テキストモードの総バイト数を表示する(UTF-8 で数える。要件 3.8)", () => {
    renderPanel({ mode: "text", text: "あA" });
    expect(screen.getByText("総バイト数: 4 バイト")).toBeTruthy();
  });

  it("テキストから 16 進へ切り替えると同じバイト列の 16 進表記を onChange に渡す(要件 3.6)", () => {
    const props = renderPanel({ mode: "text", text: "Hi" });
    fireEvent.click(screen.getByRole("radio", { name: "16 進" }));
    expect(props.onChange).toHaveBeenCalledWith({ mode: "hex", text: "48 69" });
    expect(props.onSwitchRejected).not.toHaveBeenCalled();
  });

  it("16 進モードの入力は 16 進数字と空白として解釈し、総バイト数を表示する(要件 3.3)", () => {
    renderPanel({ mode: "hex", text: "0a ff" });
    expect(screen.getByText("総バイト数: 2 バイト")).toBeTruthy();
  });

  it("16 進の不正文字は種類と位置(1 始まり)を入力欄の近くに表示する(要件 3.4)", () => {
    renderPanel({ mode: "hex", text: "4G" });
    const message = screen.getByText(/16 進数字・空白以外の文字/);
    expect(message.textContent).toContain("2 文字目");
    expect(textArea().getAttribute("aria-invalid")).toBe("true");
  });

  it("16 進の総数が奇数なら奇数エラーを表示する(要件 3.4)", () => {
    renderPanel({ mode: "hex", text: "4 8 6" });
    expect(screen.getByText(/16 進数字の総数が奇数/)).toBeTruthy();
  });

  it("入力が 1 MiB を超えると受け付けないことと上限超過を表示する(要件 3.5)", () => {
    renderPanel({ mode: "text", text: "a".repeat(MAX_INPUT_BYTES + 1) });
    expect(screen.getByText(/1 MiB/)).toBeTruthy();
    expect(screen.queryByText(/総バイト数/)).toBeNull();
  });

  it("16 進から UTF-8 として不正なバイト列へ切り替えると、モードを変えず理由を通知する(要件 3.7)", () => {
    const props = renderPanel({ mode: "hex", text: "FF" });
    fireEvent.click(screen.getByRole("radio", { name: "テキスト" }));
    expect(props.onSwitchRejected).toHaveBeenCalledWith("notUtf8");
    expect(props.onChange).not.toHaveBeenCalled();
  });

  it("16 進の奇数桁のまま切り替えると、理由 oddDigits を通知しモードを変えない(要件 3.7)", () => {
    const props = renderPanel({ mode: "hex", text: "4" });
    fireEvent.click(screen.getByRole("radio", { name: "テキスト" }));
    expect(props.onSwitchRejected).toHaveBeenCalledWith("oddDigits");
    expect(props.onChange).not.toHaveBeenCalled();
  });

  it("16 進から UTF-8 として正しいバイト列なら同じバイト列のテキストへ切り替える(要件 3.7)", () => {
    const props = renderPanel({ mode: "hex", text: "E3 81 82" });
    fireEvent.click(screen.getByRole("radio", { name: "テキスト" }));
    expect(props.onChange).toHaveBeenCalledWith({ mode: "text", text: "あ" });
    expect(props.onSwitchRejected).not.toHaveBeenCalled();
  });

  // --- レビュー差し戻し(ラウンド 1): 支援技術への通知 ---

  it("入力エラーは role=alert の要素で通知され、その文言が一致する(要件 3.4, 14.4)", () => {
    renderPanel({ mode: "hex", text: "4G" });
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("入力エラー: 16 進数字・空白以外の文字が含まれています(2 文字目)");
  });

  it("奇数桁の入力エラーも role=alert で通知される(要件 3.4, 14.4)", () => {
    renderPanel({ mode: "hex", text: "4 8 6" });
    expect(screen.getByRole("alert").textContent).toContain("16 進数字の総数が奇数です");
  });

  it("1 MiB 超の入力エラーも role=alert で通知される(要件 3.5, 14.4)", () => {
    renderPanel({ mode: "text", text: "a".repeat(MAX_INPUT_BYTES + 1) });
    expect(screen.getByRole("alert").textContent).toContain("1 MiB");
  });

  it("有効な入力では alert を出さず、総バイト数だけを表示する", () => {
    const { unmount } = render(
      <InputPanel input={{ mode: "hex", text: "4G" }} onChange={vi.fn()} onSwitchRejected={vi.fn()} />,
    );
    expect(screen.getByRole("alert")).toBeTruthy();
    unmount();
    renderPanel({ mode: "hex", text: "0a ff" });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("総バイト数: 2 バイト")).toBeTruthy();
  });

  // --- レビュー差し戻し(ラウンド 1): aria-describedby の関連付け ---

  it("エラー表示中は入力欄の aria-describedby が表示中のエラー要素の id を指す", () => {
    renderPanel({ mode: "hex", text: "4G" });
    const alert = screen.getByRole("alert");
    expect(alert.id).not.toBe("");
    expect(textArea().getAttribute("aria-describedby")).toBe(alert.id);
  });

  it("エラーがないときは入力欄の aria-describedby を付けない", () => {
    renderPanel({ mode: "hex", text: "0a ff" });
    expect(textArea().hasAttribute("aria-describedby")).toBe(false);
  });

  it("エラーが解消すると aria-describedby を外す", () => {
    const noop = { onChange: vi.fn(), onSwitchRejected: vi.fn() };
    const { rerender } = render(<InputPanel input={{ mode: "hex", text: "4G" }} {...noop} />);
    expect(textArea().hasAttribute("aria-describedby")).toBe(true);
    rerender(<InputPanel input={{ mode: "hex", text: "0a" }} {...noop} />);
    expect(textArea().hasAttribute("aria-describedby")).toBe(false);
  });

  // --- レビュー差し戻し(ラウンド 1): 実行中の編集不可(要件 5.10) ---

  it("disabled のとき、モード切替のラジオと入力欄を操作できない(要件 5.10)", () => {
    renderPanel({ mode: "text", text: "abc" }, { disabled: true });
    expect((screen.getByRole("radio", { name: "テキスト" }) as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("radio", { name: "16 進" }) as HTMLInputElement).disabled).toBe(true);
    expect(textArea().disabled).toBe(true);
  });

  it("disabled でないときは既定どおり編集可能である", () => {
    renderPanel({ mode: "text", text: "abc" });
    expect((screen.getByRole("radio", { name: "16 進" }) as HTMLInputElement).disabled).toBe(false);
    expect(textArea().disabled).toBe(false);
  });

  // --- レビュー差し戻し(ラウンド 1): キーボード操作の前提 ---

  // 注: jsdom は矢印キーによるラジオの選択移動を実装しない。ブラウザの矢印キー移動が
  // 1 グループとして働くための前提(ネイティブ radio + 共有 name + radiogroup)を検証する。
  it("モードのラジオは同一の name を共有する 1 つの radiogroup に属し、ブラウザの矢印キー移動が効く", () => {
    renderPanel({ mode: "text", text: "" });
    const text = screen.getByRole("radio", { name: "テキスト" }) as HTMLInputElement;
    const hex = screen.getByRole("radio", { name: "16 進" }) as HTMLInputElement;
    expect(text.type).toBe("radio");
    expect(text.name).not.toBe("");
    expect(text.name).toBe(hex.name);
    const group = screen.getByRole("radiogroup", { name: "入力の形式" });
    expect(group.contains(text)).toBe(true);
    expect(group.contains(hex)).toBe(true);
  });
});
