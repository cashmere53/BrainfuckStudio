import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TapeWindowDto } from "../../generated/TapeWindowDto";
import { TapeStrip, tapeStripCharacter } from "./TapeStrip";

function cellAt(index: number): HTMLElement {
  return screen.getByRole("listitem", { name: new RegExp(`^セル ${index}、`) });
}

describe("TapeStrip(要件 10.3)", () => {
  it("窓の各セルを添字・10 進・16 進・文字つきで横一列に表示する", () => {
    const window: TapeWindowDto = { start: 3, cells: [0, 65, 255], pointerOffset: 1 };
    render(<TapeStrip window={window} />);

    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(cellAt(3).getAttribute("aria-label")).toBe("セル 3、10 進 0、16 進 00、文字 表示なし");
    expect(cellAt(4).getAttribute("aria-label")).toBe("セル 4、10 進 65、16 進 41、文字 A");
    expect(cellAt(5).getAttribute("aria-label")).toBe("セル 5、10 進 255、16 進 FF、文字 表示なし");
    expect(cellAt(4).querySelector(".tape-strip-cell-index")?.textContent).toBe("4");
    expect(cellAt(4).querySelector(".tape-strip-cell-dec")?.textContent).toBe("65");
    expect(cellAt(4).querySelector(".tape-strip-cell-hex")?.textContent).toBe("41");
    expect(cellAt(4).querySelector(".tape-strip-cell-char")?.textContent).toBe("A");
  });

  it("dp のセルだけに aria-current と「▼ dp」の文言を付けて強調する(色だけに頼らない)", () => {
    const window: TapeWindowDto = { start: 3, cells: [0, 65, 255], pointerOffset: 1 };
    render(<TapeStrip window={window} />);

    expect(cellAt(4).getAttribute("aria-current")).toBe("true");
    expect(cellAt(4).textContent).toContain("▼ dp");
    expect(cellAt(3).getAttribute("aria-current")).toBeNull();
    expect(cellAt(3).textContent).not.toContain("▼ dp");
    expect(cellAt(5).getAttribute("aria-current")).toBeNull();
    expect(screen.getByText("▼ dp").textContent).toBe("▼ dp");
  });

  it("一覧に名前を付け、横に並んだ窓をスクロール可能な領域として示す", () => {
    render(<TapeStrip window={{ start: 0, cells: [1], pointerOffset: 0 }} />);
    expect(screen.getByRole("list", { name: "テープ(ポインタ周辺)" })).toBeTruthy();
  });

  it("文字の表示規則: 可視 ASCII は文字、空白は記号、それ以外は表示なし", () => {
    expect(tapeStripCharacter(0x20)).toBe("␠");
    expect(tapeStripCharacter(0x21)).toBe("!");
    expect(tapeStripCharacter(0x7e)).toBe("~");
    expect(tapeStripCharacter(0x7f)).toBe("—");
    expect(tapeStripCharacter(0)).toBe("—");
    expect(tapeStripCharacter(255)).toBe("—");
  });
});
