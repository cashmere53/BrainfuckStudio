import { describe, expect, it } from "vitest";
import type { OutputChunkDto } from "../generated/OutputChunkDto";
import { encodeBase64 } from "./base64";
import { OUTPUT_TAIL_LIMIT } from "./limits";
import { applyChunk, emptyOutput, type OutputBuffer } from "./outputBuffer";
import { renderHex, renderText, summarizeOutput } from "./outputFormat";

function chunk(dataStart: number, totalLen: number, bytes: number[]): OutputChunkDto {
  return { dataStart, totalLen, dataBase64: encodeBase64(new Uint8Array(bytes)) };
}

/** 1 チャンクの出力から表示用バッファを作る */
function bufferOf(bytes: number[]): OutputBuffer {
  return applyChunk(emptyOutput(), chunk(0, bytes.length, bytes));
}

describe("renderText", () => {
  it("returns an empty string for empty output", () => {
    expect(renderText(emptyOutput())).toBe("");
  });

  it("decodes ASCII as is", () => {
    expect(renderText(bufferOf([0x48, 0x69]))).toBe("Hi");
  });

  it("decodes multi-byte UTF-8 characters", () => {
    // 「あ」= E3 81 82, 😀 = F0 9F 98 80
    expect(renderText(bufferOf([0xe3, 0x81, 0x82, 0xf0, 0x9f, 0x98, 0x80]))).toBe("あ😀");
  });

  it("does not lose a UTF-8 character split across two chunks", () => {
    // 「あ」(E3 81 82) を 1 バイト目と 2 バイト目以降の 2 チャンクに分ける
    let buf = applyChunk(emptyOutput(), chunk(0, 1, [0xe3]));
    buf = applyChunk(buf, chunk(1, 3, [0x81, 0x82]));
    expect(renderText(buf)).toBe("あ");
  });

  it("does not lose a character split across three chunks", () => {
    // 😀 = F0 9F 98 80
    let buf = applyChunk(emptyOutput(), chunk(0, 1, [0xf0]));
    buf = applyChunk(buf, chunk(1, 2, [0x9f]));
    buf = applyChunk(buf, chunk(2, 4, [0x98, 0x80]));
    expect(renderText(buf)).toBe("😀");
  });

  it("replaces invalid UTF-8 bytes with U+FFFD", () => {
    expect(renderText(bufferOf([0x61, 0xff, 0x62]))).toBe("a�b");
  });

  it("keeps newlines and tabs as they are", () => {
    expect(renderText(bufferOf([0x61, 0x0a, 0x09, 0x62]))).toBe("a\n\tb");
  });

  it("shows other C0 control characters and DEL as visible symbols", () => {
    // 0x00 → U+2400, 0x1B → U+241B, 0x7F → U+2421
    expect(renderText(bufferOf([0x00, 0x1b, 0x7f]))).toBe("␀␛␡");
  });

  it("keeps a leading BOM as U+FEFF instead of stripping it", () => {
    expect(renderText(bufferOf([0xef, 0xbb, 0xbf, 0x78]))).toBe("﻿x");
  });

  it("starts from the retained tail when output was truncated", () => {
    // 先頭の 0x5A('Z')は省略され、保持される末尾の 65,536 バイトはすべて 0x41('A')になる
    const bytes = Array.from({ length: OUTPUT_TAIL_LIMIT + 1 }, (_, i) => (i === 0 ? 0x5a : 0x41));
    const text = renderText(bufferOf(bytes));
    expect(text.length).toBe(OUTPUT_TAIL_LIMIT);
    expect(text.startsWith("A")).toBe(true);
  });
});

describe("renderHex", () => {
  it("returns no rows for empty output", () => {
    expect(renderHex(emptyOutput())).toEqual([]);
  });

  it("renders 16 bytes per row as 2-digit uppercase hex with absolute offsets", () => {
    const bytes = Array.from({ length: 17 }, (_, i) => 0xa0 + i);
    const rows = renderHex(bufferOf(bytes));
    expect(rows).toHaveLength(2);
    expect(rows[0]).toEqual({
      offset: "00000000",
      hex: "A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 AA AB AC AD AE AF",
    });
    expect(rows[1]).toEqual({ offset: "00000010", hex: "B0" });
  });

  it("renders 0x00 and 0xFF bytes as 00 and FF", () => {
    expect(renderHex(bufferOf([0x00, 0xff]))).toEqual([{ offset: "00000000", hex: "00 FF" }]);
  });

  it("keeps absolute byte positions when output was truncated", () => {
    const total = OUTPUT_TAIL_LIMIT + 4;
    const buf = applyChunk(
      emptyOutput(),
      chunk(0, total, Array.from({ length: total }, () => 0x01)),
    );
    const rows = renderHex(buf);
    expect(rows[0]?.offset).toBe("00000004");
    expect(rows[0]?.hex.startsWith("01 01")).toBe(true);
    expect(rows[rows.length - 1]?.offset).toBe(
      (4 + Math.floor((OUTPUT_TAIL_LIMIT - 1) / 16) * 16).toString(16).toUpperCase().padStart(8, "0"),
    );
  });
});

describe("summarizeOutput", () => {
  it("reports an empty, untruncated output", () => {
    expect(summarizeOutput(emptyOutput())).toEqual({
      empty: true,
      totalLen: 0,
      truncated: false,
      omittedLen: 0,
    });
  });

  it("reports the exact total and omitted length when truncated", () => {
    const total = OUTPUT_TAIL_LIMIT + 123;
    const buf = applyChunk(
      emptyOutput(),
      chunk(0, total, Array.from({ length: total }, () => 0x20)),
    );
    expect(summarizeOutput(buf)).toEqual({
      empty: false,
      totalLen: total,
      truncated: true,
      omittedLen: 123,
    });
  });
});
