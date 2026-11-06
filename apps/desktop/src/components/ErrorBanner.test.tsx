import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DisplayError } from "../state/debuggerStore";
import { ErrorBanner } from "./ErrorBanner";

function position(line: number, column: number) {
  return { byteOffset: 0, utf16Offset: 0, line, column };
}

describe("ErrorBanner(要件 10.4・10.5・10.6・10.7 の区別)", () => {
  it("エラーが無いときは何も描画しない", () => {
    const { container } = render(<ErrorBanner error={null} />);
    expect(container.innerHTML).toBe("");
  });

  it("解析エラー(対応する開き括弧がない閉じ括弧)の種類・命令添字・行/列を表示する(要件 10.4)", () => {
    const error: DisplayError = {
      category: "parse",
      error: {
        kind: "unmatchedClosingBracket",
        instructionIndex: 3,
        position: { byteOffset: 9, utf16Offset: 9, line: 2, column: 5 },
      },
    };
    render(<ErrorBanner error={error} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("解析エラー");
    expect(alert.textContent).toContain("対応する開き括弧がない閉じ括弧");
    expect(alert.textContent).toContain("命令添字: 3");
    expect(alert.textContent).toContain("行 2・列 5");
  });

  it("解析エラー(対応する閉じ括弧がない開き括弧)を日本語で表示する(要件 10.4)", () => {
    const error: DisplayError = {
      category: "parse",
      error: { kind: "unmatchedOpeningBracket", instructionIndex: 0, position: position(1, 1) },
    };
    render(<ErrorBanner error={error} />);
    expect(screen.getByRole("alert").textContent).toContain("対応する閉じ括弧がない開き括弧");
  });

  it("ポインタ範囲外の実行エラーの pc・dp・方向を表示する(要件 10.5)", () => {
    const error: DisplayError = {
      category: "runtime",
      error: { kind: "pointerOutOfBounds", pc: 4, dp: 0, direction: "left" },
    };
    render(<ErrorBanner error={error} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("実行エラー");
    expect(alert.textContent).toContain("ポインタ範囲外");
    expect(alert.textContent).toContain("pc: 4");
    expect(alert.textContent).toContain("dp: 0");
    expect(alert.textContent).toContain("方向: 左");
  });

  it("右方向の範囲外も「右」と表示する(要件 10.5)", () => {
    const error: DisplayError = {
      category: "runtime",
      error: { kind: "pointerOutOfBounds", pc: 7, dp: 29999, direction: "right" },
    };
    render(<ErrorBanner error={error} />);
    expect(screen.getByRole("alert").textContent).toContain("方向: 右");
  });

  it("命令数上限超過の上限と pc を表示する(要件 10.6)", () => {
    const error: DisplayError = {
      category: "runtime",
      error: { kind: "stepLimitExceeded", limit: 1000, pc: 12 },
    };
    render(<ErrorBanner error={error} />);
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("命令数の上限(1000 命令)を超えました");
    expect(alert.textContent).toContain("pc: 12");
  });

  it("実行エラーは role=alert の領域に出し、色だけに頼らずテキストで種類を示す(要件 10.7 の区別)", () => {
    const error: DisplayError = {
      category: "runtime",
      error: { kind: "stepLimitExceeded", limit: 5, pc: 0 },
    };
    render(<ErrorBanner error={error} />);
    const alert = screen.getByRole("alert");
    // 操作エラーの見出し(「操作エラー」)とは混同しない
    expect(alert.textContent).not.toContain("操作エラー");
  });
});
