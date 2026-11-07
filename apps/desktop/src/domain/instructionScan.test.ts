import { describe, expect, it } from "vitest";
import { Text } from "@codemirror/state";
import { INSTRUCTION_CHARS, firstInstructionFrom, firstInstructionInLine } from "./instructionScan";

/** 改行コード(LF / CRLF / CR)を含む文字列から文書を作る */
function doc(source: string): Text {
  return Text.of(source.split(/\r\n|\r|\n/));
}

describe("INSTRUCTION_CHARS", () => {
  it("is exactly the eight Brainfuck instruction characters", () => {
    expect(INSTRUCTION_CHARS).toBe("><+-.,[]");
  });
});

describe("firstInstructionInLine: 行内で最初の命令の位置(UTF-16、文書先頭からの絶対位置)", () => {
  it("returns null for a comment-only line", () => {
    expect(firstInstructionInLine(doc("hello world\n+"), 1)).toBeNull();
  });

  it("returns null for an empty line", () => {
    expect(firstInstructionInLine(doc("+\n\n+"), 2)).toBeNull();
  });

  it("skips a leading comment and returns the first instruction in the line", () => {
    // "abc+" の '+' は位置 3
    expect(firstInstructionInLine(doc("abc+>"), 1)).toBe(3);
  });

  it("returns the line start when the line begins with an instruction", () => {
    // 1 行目 "+" は 0、2 行目 "ab-" の '-' は 2 + 2 = 4
    expect(firstInstructionInLine(doc("+\nab-"), 2)).toBe(4);
  });

  it("counts a multi-byte character as its UTF-16 length (あ = 1 unit)", () => {
    // "あい+" の '+' は位置 2
    expect(firstInstructionInLine(doc("あい+"), 1)).toBe(2);
  });

  it("counts an emoji as two UTF-16 units before the instruction", () => {
    // "😀+" の '+' は位置 2(サロゲートペア 2 単位)
    expect(firstInstructionInLine(doc("😀+"), 1)).toBe(2);
  });

  it("treats CRLF and CR as one line break (the CodeMirror document model)", () => {
    // 文書は改行を 1 単位として数える: "ab\r\n>" は "ab\n>" と同じ。'>' は 3
    expect(firstInstructionInLine(doc("ab\r\n>"), 2)).toBe(3);
    // "ab\r>" の '>' も 3
    expect(firstInstructionInLine(doc("ab\r>"), 2)).toBe(3);
  });

  it("finds the first instruction in a line that mixes comments and instructions", () => {
    // "# a . b" の '.' は位置 4
    expect(firstInstructionInLine(doc("# a . b ,"), 1)).toBe(4);
  });

  it("does not look into the next line", () => {
    expect(firstInstructionInLine(doc("comment\n+"), 1)).toBeNull();
  });

  it("throws a RangeError for a line number outside the document", () => {
    expect(() => firstInstructionInLine(doc("+"), 0)).toThrow(RangeError);
    expect(() => firstInstructionInLine(doc("+"), 2)).toThrow(RangeError);
  });
});

describe("firstInstructionFrom: カーソル位置以降で最初の命令の位置(UTF-16)", () => {
  it("includes the cursor position itself", () => {
    expect(firstInstructionFrom(doc("a+b"), 1)).toBe(1);
  });

  it("returns the first instruction after the cursor on the same line", () => {
    expect(firstInstructionFrom(doc("+ab-"), 1)).toBe(3);
  });

  it("crosses line breaks to find the next instruction", () => {
    // "ab\n\nx+" の '+' は 5
    expect(firstInstructionFrom(doc("ab\n\nx+"), 0)).toBe(5);
    // カーソルが 2 行目の途中でも次の行の命令を見つける
    expect(firstInstructionFrom(doc("ab\nc\n[x"), 1)).toBe(5);
  });

  it("crosses CRLF line breaks, counting each break as one unit", () => {
    // "ab\r\ncd\r\n-" は文書上 "ab\ncd\n-"。'-' は 6(ab=2, 改行=1, cd=2, 改行=1)
    expect(firstInstructionFrom(doc("ab\r\ncd\r\n-"), 0)).toBe(6);
  });

  it("skips multi-byte and surrogate-pair comments", () => {
    // "あ😀 ," の ',' は 0 始まりで 4(あ=1, 😀=2, space=1)
    expect(firstInstructionFrom(doc("あ😀 ,"), 0)).toBe(4);
  });

  it("returns null when there is no instruction until the end of the document", () => {
    expect(firstInstructionFrom(doc("+\ncomment\n\ntail"), 1)).toBeNull();
  });

  it("returns null when the cursor is at the end of the document", () => {
    expect(firstInstructionFrom(doc("+"), 1)).toBeNull();
  });

  it("returns null for an empty document", () => {
    expect(firstInstructionFrom(doc(""), 0)).toBeNull();
  });

  it("throws a RangeError for a position outside the document", () => {
    expect(() => firstInstructionFrom(doc("+"), -1)).toThrow(RangeError);
    expect(() => firstInstructionFrom(doc("+"), 2)).toThrow(RangeError);
  });
});
