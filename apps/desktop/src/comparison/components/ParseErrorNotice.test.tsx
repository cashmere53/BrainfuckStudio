import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ParseErrorDto } from "../../generated/ParseErrorDto";
import { ParseErrorNotice } from "./ParseErrorNotice";

describe("ParseErrorNotice(要件 9.6・10.9)", () => {
  it("閉じ括弧の解析エラーは種類・命令添字・行と列を示し、表を出さない", () => {
    const error: ParseErrorDto = {
      kind: "unmatchedClosingBracket",
      instructionIndex: 3,
      position: { byteOffset: 4, utf16Offset: 4, line: 2, column: 5 },
    };
    render(<ParseErrorNotice error={error} />);

    const notice = screen.getByRole("alert");
    expect(notice.textContent).toContain("対応する開き括弧がない閉じ括弧");
    expect(notice.textContent).toContain("命令添字: 3");
    expect(notice.textContent).toContain("位置: 行 2・列 5");
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("開き括弧の解析エラーは対応する種類の文言を示す", () => {
    const error: ParseErrorDto = {
      kind: "unmatchedOpeningBracket",
      instructionIndex: 0,
      position: { byteOffset: 0, utf16Offset: 0, line: 1, column: 1 },
    };
    render(<ParseErrorNotice error={error} />);

    expect(screen.getByRole("alert").textContent).toContain("対応する閉じ括弧がない開き括弧");
  });
});
