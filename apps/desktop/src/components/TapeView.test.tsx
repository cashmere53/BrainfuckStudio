import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TapeWindowDto } from "../generated/TapeWindowDto";
import { TapeView, type TapeViewProps, tapeCellCharacter } from "./TapeView";

function renderView(overrides: Partial<TapeViewProps> = {}) {
  const props: TapeViewProps = {
    tapeWindow: null,
    radius: 16,
    onRadiusChange: vi.fn(),
    ...overrides,
  };
  render(<TapeView {...props} />);
  return props;
}

function cellItems(): HTMLElement[] {
  return screen.queryAllByRole("listitem");
}

function cellAt(index: number): HTMLElement {
  return screen.getByRole("listitem", { name: new RegExp(`^セル ${index}[ 、]`) });
}

describe("TapeView(要件 8.1〜8.3・8.5・8.7・14.4)", () => {
  it("窓の各セルを添字・10 進・16 進・ASCII 文字つきで表示する(要件 8.2)", () => {
    const window: TapeWindowDto = { start: 3, cells: [0, 65, 255], pointerOffset: 1 };
    renderView({ tapeWindow: window, radius: 1 });

    const cell = cellAt(4);
    expect(cell.textContent).toContain("4");
    expect(cell.textContent).toContain("65");
    expect(cell.textContent).toContain("41");
    expect(cell.textContent).toContain("A");
    expect(cellAt(3).textContent).toContain("00");
    expect(cellAt(5).textContent).toContain("FF");
  });

  it("窓は DTO のセル数だけ表示し、端で切り詰められた窓を正しく示す(要件 8.1)", () => {
    // テープ左端 (dp = 0, 半径 3) の窓: 添字 0..3 の 4 セルのみ。負の添字は出ない
    renderView({ tapeWindow: { start: 0, cells: [0, 0, 0, 0], pointerOffset: 0 }, radius: 3 });
    expect(cellItems()).toHaveLength(4);
    expect(screen.queryByRole("listitem", { name: /^セル -/ })).toBeNull();
    expect(cellAt(0)).toBeTruthy();
    expect(cellAt(3)).toBeTruthy();
  });

  it("半径 0 では dp のセル 1 つだけを表示し、それを強調する(要件 8.5)", () => {
    renderView({ tapeWindow: { start: 7, cells: [9], pointerOffset: 0 }, radius: 0 });
    expect(cellItems()).toHaveLength(1);
    expect(cellAt(7).getAttribute("aria-current")).toBe("true");
    expect(screen.getByText("▼ dp")).toBeTruthy();
  });

  it("dp のセルを枠・ラベル・aria-current で強調し、他のセルは強調しない(要件 8.3)", () => {
    renderView({ tapeWindow: { start: 0, cells: [1, 2, 3], pointerOffset: 2 }, radius: 2 });
    const pointer = cellAt(2);
    expect(pointer.getAttribute("aria-current")).toBe("true");
    expect(pointer.classList.contains("cue-frame")).toBe(true);
    expect(within(pointer).getByText("▼ dp")).toBeTruthy();
    expect(cellAt(0).getAttribute("aria-current")).toBeNull();
    expect(cellAt(1).getAttribute("aria-current")).toBeNull();
    expect(screen.getAllByText("▼ dp")).toHaveLength(1);
  });

  it("未開始(窓が null)では半径に応じた全 0 の初期窓を dp 0 で表示する(要件 8.7)", () => {
    renderView({ tapeWindow: null, radius: 2 });
    expect(cellItems()).toHaveLength(3);
    for (const index of [0, 1, 2]) {
      expect(cellAt(index).textContent).toContain("00");
    }
    expect(cellAt(0).getAttribute("aria-current")).toBe("true");
    expect(screen.getByText("▼ dp")).toBeTruthy();
  });

  it("16 進の span は常に 2 桁の大文字で表示する(要件 8.2)", () => {
    renderView({ tapeWindow: { start: 0, cells: [0x00, 0x05, 0x0a, 0x41, 0xff], pointerOffset: 0 }, radius: 4 });
    const hexOf = (index: number) => cellAt(index).querySelector(".tape-cell-hex")?.textContent;
    expect(hexOf(0)).toBe("00");
    expect(hexOf(1)).toBe("05");
    expect(hexOf(2)).toBe("0A");
    expect(hexOf(3)).toBe("41");
    expect(hexOf(4)).toBe("FF");
  });

  it("空白(0x20)は文字の span に「␠」と表示する(要件 8.2)", () => {
    renderView({ tapeWindow: { start: 0, cells: [0x20], pointerOffset: 0 }, radius: 0 });
    expect(cellAt(0).querySelector(".tape-cell-char")?.textContent).toBe("␠");
  });

  it("半径が範囲外・非整数のときは 0〜64 の整数に丸めて select と初期窓に渡す(要件 8.5)", () => {
    const { unmount } = render(<TapeView tapeWindow={null} radius={100} onRadiusChange={vi.fn()} />);
    expect((screen.getByRole("combobox", { name: "表示半径" }) as HTMLSelectElement).value).toBe("64");
    expect(cellItems()).toHaveLength(65);
    unmount();

    const low = render(<TapeView tapeWindow={null} radius={-3} onRadiusChange={vi.fn()} />);
    expect((screen.getByRole("combobox", { name: "表示半径" }) as HTMLSelectElement).value).toBe("0");
    expect(cellItems()).toHaveLength(1);
    low.unmount();

    render(<TapeView tapeWindow={null} radius={2.6} onRadiusChange={vi.fn()} />);
    expect((screen.getByRole("combobox", { name: "表示半径" }) as HTMLSelectElement).value).toBe("3");
    // 初期窓は dp 0 始まりのため radius + 1 セル(3 + 1 = 4)
    expect(cellItems()).toHaveLength(4);
  });

  it("半径セレクタは 0〜64 を選べ、変更を onRadiusChange に整数で通知する(要件 8.5)", () => {
    const props = renderView({ radius: 16 });
    const select = screen.getByRole("combobox", { name: "表示半径" }) as HTMLSelectElement;
    expect(select.value).toBe("16");
    const values = Array.from(select.options).map((option) => option.value);
    expect(values[0]).toBe("0");
    expect(values[values.length - 1]).toBe("64");
    expect(values).toHaveLength(65);
    fireEvent.change(select, { target: { value: "5" } });
    expect(props.onRadiusChange).toHaveBeenCalledWith(5);
  });
});

describe("tapeCellCharacter(要件 8.2: 表示可能な ASCII のみ文字を出す)", () => {
  it("表示可能な ASCII(0x21〜0x7E)はその文字を返す", () => {
    expect(tapeCellCharacter(0x41)).toBe("A");
    expect(tapeCellCharacter(0x21)).toBe("!");
    expect(tapeCellCharacter(0x7e)).toBe("~");
  });

  it("空白(0x20)は空白として識別できる記号を返す", () => {
    expect(tapeCellCharacter(0x20)).toBe("␠");
  });

  it("制御文字・0x7F 以上は文字を出さず、表示なしを示す記号を返す", () => {
    expect(tapeCellCharacter(0x00)).toBe("—");
    expect(tapeCellCharacter(0x0a)).toBe("—");
    expect(tapeCellCharacter(0x7f)).toBe("—");
    expect(tapeCellCharacter(0xff)).toBe("—");
  });
});
