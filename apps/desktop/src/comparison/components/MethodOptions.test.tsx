import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MethodOptions, type MethodOptionsProps } from "./MethodOptions";

function renderOptions(overrides: Partial<MethodOptionsProps> = {}) {
  const props: MethodOptionsProps = {
    optimized: true,
    wasm: false,
    repetitionsText: "2",
    maxRepetitions: 5,
    onOptimizedChange: vi.fn(),
    onWasmChange: vi.fn(),
    onRepetitionsTextChange: vi.fn(),
    ...overrides,
  };
  render(<MethodOptions {...props} />);
  return props;
}

describe("MethodOptions(要件 11.9)", () => {
  it("最適化と WASM のチェックボックスは現在の選択を反映する", () => {
    renderOptions({ optimized: true, wasm: false });

    expect((screen.getByRole("checkbox", { name: "最適化インタプリタ" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole("checkbox", { name: "WASM コンパイル実行" }) as HTMLInputElement).checked).toBe(false);
  });

  it("最適化のチェックを切り替えると、新しい値を通知する", () => {
    const props = renderOptions({ optimized: true });

    fireEvent.click(screen.getByRole("checkbox", { name: "最適化インタプリタ" }));

    expect(props.onOptimizedChange).toHaveBeenCalledTimes(1);
    expect(props.onOptimizedChange).toHaveBeenCalledWith(false);
    expect(props.onWasmChange).not.toHaveBeenCalled();
  });

  it("WASM のチェックを切り替えると、新しい値を通知する", () => {
    const props = renderOptions({ wasm: false });

    fireEvent.click(screen.getByRole("checkbox", { name: "WASM コンパイル実行" }));

    expect(props.onWasmChange).toHaveBeenCalledTimes(1);
    expect(props.onWasmChange).toHaveBeenCalledWith(true);
    expect(props.onOptimizedChange).not.toHaveBeenCalled();
  });

  it("素朴インタプリタは選択肢に出さず、基準として常に実行される旨を示す", () => {
    renderOptions();

    expect(screen.queryByRole("checkbox", { name: /素朴/ })).toBeNull();
    expect(screen.getByText(/素朴インタプリタは基準として常に実行されます/)).toBeTruthy();
  });

  it("繰り返し回数はラベル付きの入力で、範囲を示し、入力の変更を通知する", () => {
    const props = renderOptions({ repetitionsText: "2", maxRepetitions: 5 });

    const input = screen.getByLabelText("繰り返し回数(1〜5)") as HTMLInputElement;
    expect(input.value).toBe("2");

    fireEvent.change(input, { target: { value: "3" } });

    expect(props.onRepetitionsTextChange).toHaveBeenCalledWith("3");
  });

  it("無効のとき、すべての操作要素を無効にする", () => {
    renderOptions({ disabled: true });

    expect((screen.getByRole("checkbox", { name: "最適化インタプリタ" }) as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("checkbox", { name: "WASM コンパイル実行" }) as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByLabelText("繰り返し回数(1〜5)") as HTMLInputElement).disabled).toBe(true);
  });
});
