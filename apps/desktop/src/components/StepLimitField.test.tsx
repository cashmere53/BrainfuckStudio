import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StepLimitField, type StepLimitFieldProps } from "./StepLimitField";

function renderField(overrides: Partial<StepLimitFieldProps> = {}) {
  const props: StepLimitFieldProps = {
    text: "10000000",
    unlimited: false,
    onTextChange: vi.fn(),
    onUnlimitedChange: vi.fn(),
    ...overrides,
  };
  render(<StepLimitField {...props} />);
  return props;
}

function limitInput(): HTMLInputElement {
  return screen.getByRole("textbox", { name: "命令数上限" }) as HTMLInputElement;
}

describe("StepLimitField(要件 5.1〜5.4)", () => {
  it("既定値 10,000,000 を入力欄に表示し、有効な値では誤りを表示しない(要件 5.1)", () => {
    renderField();
    expect(limitInput().value).toBe("10000000");
    expect(limitInput().getAttribute("aria-invalid")).toBe("false");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("入力変更は文字列のまま onTextChange に渡す", () => {
    const props = renderField();
    fireEvent.change(limitInput(), { target: { value: "42" } });
    expect(props.onTextChange).toHaveBeenCalledWith("42");
  });

  it("整数でない値は理由を欄の近くに表示し、aria-invalid を立てる(要件 5.3)", () => {
    renderField({ text: "1.5" });
    expect(screen.getByText(/10 進整数/)).toBeTruthy();
    expect(limitInput().getAttribute("aria-invalid")).toBe("true");
  });

  it("0 以下の値は下限エラーを表示する(要件 5.3)", () => {
    renderField({ text: "0" });
    expect(screen.getByText(/1 以上/)).toBeTruthy();
  });

  it("2^53 − 1 を超える値は上限エラーを表示する(要件 5.3)", () => {
    renderField({ text: "9007199254740992" });
    expect(screen.getByText(/9,007,199,254,740,991 以下/)).toBeTruthy();
  });

  it("空欄は未入力のエラーを表示する", () => {
    renderField({ text: "" });
    expect(screen.getByText(/入力してください/)).toBeTruthy();
  });

  it("「上限なし」を選ぶと入力欄を無効化し、増え続けうることの警告を表示する(要件 5.4)", () => {
    renderField({ text: "not a number", unlimited: true });
    expect(limitInput().disabled).toBe(true);
    expect(screen.getByText(/出力やメモリ使用量が増え続けるおそれ/)).toBeTruthy();
    expect(screen.queryByText(/10 進整数/)).toBeNull();
  });

  it("「上限なし」のチェック変更は onUnlimitedChange に真偽値で渡す(要件 5.4)", () => {
    const props = renderField();
    fireEvent.click(screen.getByRole("checkbox", { name: /上限なし/ }));
    expect(props.onUnlimitedChange).toHaveBeenCalledWith(true);
  });

  it("上限ありの間は警告を表示しない", () => {
    renderField();
    expect(screen.queryByText(/増え続けるおそれ/)).toBeNull();
  });

  // --- レビュー差し戻し(ラウンド 1): 支援技術への通知 ---

  it("不正値の理由は role=alert の要素で通知され、日本語の文言が一致する(要件 5.3, 14.4)", () => {
    renderField({ text: "1.5" });
    expect(screen.getByRole("alert").textContent).toBe("入力エラー: 命令数上限は 10 進整数で入力してください");
  });

  it("空欄・下限・上限超過それぞれの文言が role=alert で一致する(要件 5.3)", () => {
    const cases: Array<[string, string]> = [
      ["", "入力エラー: 命令数上限を入力してください"],
      ["0", "入力エラー: 命令数上限は 1 以上で入力してください"],
      ["9007199254740992", "入力エラー: 命令数上限は 9,007,199,254,740,991 以下で入力してください"],
    ];
    for (const [text, message] of cases) {
      const { unmount } = render(
        <StepLimitField text={text} unlimited={false} onTextChange={vi.fn()} onUnlimitedChange={vi.fn()} />,
      );
      expect(screen.getByRole("alert").textContent).toBe(message);
      unmount();
    }
  });

  it("有効な値では role=alert を出さない(不正値から有効値への再描画で消えることも確認する)", () => {
    const common = { unlimited: false, onTextChange: vi.fn(), onUnlimitedChange: vi.fn() };
    const { rerender } = render(<StepLimitField text="abc" {...common} />);
    expect(screen.getByRole("alert")).toBeTruthy();
    rerender(<StepLimitField text="10000000" {...common} />);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("「上限なし」の警告は role=status で通知される(要件 5.4, 14.4)", () => {
    renderField({ unlimited: true });
    const status = screen.getByRole("status");
    expect(status.textContent).toContain("増え続けるおそれ");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("上限ありのときは role=status の要素を出さない", () => {
    renderField();
    expect(screen.queryByRole("status")).toBeNull();
  });

  // --- レビュー差し戻し(ラウンド 1): aria-describedby の関連付け ---

  it("不正値のとき入力欄の aria-describedby が表示中のエラー要素の id を指す", () => {
    renderField({ text: "0" });
    const alert = screen.getByRole("alert");
    expect(alert.id).not.toBe("");
    expect(limitInput().getAttribute("aria-describedby")).toBe(alert.id);
  });

  it("有効な値のときは入力欄の aria-describedby を付けない", () => {
    renderField({ text: "10000000" });
    expect(limitInput().hasAttribute("aria-describedby")).toBe(false);
  });

  // --- レビュー差し戻し(ラウンド 1): 実行中の編集不可(要件 5.10) ---

  it("disabled のとき数値欄と「上限なし」のチェックを操作できない(要件 5.10)", () => {
    renderField({ disabled: true });
    expect(limitInput().disabled).toBe(true);
    expect((screen.getByRole("checkbox", { name: /上限なし/ }) as HTMLInputElement).disabled).toBe(true);
  });

  it("disabled でないときは既定どおり編集可能である", () => {
    renderField();
    expect(limitInput().disabled).toBe(false);
    expect((screen.getByRole("checkbox", { name: /上限なし/ }) as HTMLInputElement).disabled).toBe(false);
  });
});
