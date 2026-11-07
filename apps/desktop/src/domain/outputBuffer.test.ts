import { describe, expect, it } from "vitest";
import type { OutputChunkDto } from "../generated/OutputChunkDto";
import { encodeBase64 } from "./base64";
import { OUTPUT_TAIL_LIMIT } from "./limits";
import { applyChunk, emptyOutput, resetOutput } from "./outputBuffer";

/** 出力全体の `dataStart` からのバイト列を DTO の形で作る */
function chunk(dataStart: number, totalLen: number, bytes: number[]): OutputChunkDto {
  return { dataStart, totalLen, dataBase64: encodeBase64(new Uint8Array(bytes)) };
}

/** 長さ n の決まった並びのバイト列(位置 i の値は i mod 256) */
function patterned(n: number, offset = 0): number[] {
  return Array.from({ length: n }, (_, i) => (offset + i) % 256);
}

describe("emptyOutput", () => {
  it("starts with no bytes, zero total and no truncation", () => {
    const buf = emptyOutput();
    expect(buf.bytes).toEqual(new Uint8Array([]));
    expect(buf.totalLen).toBe(0);
    expect(buf.truncated).toBe(false);
    expect(buf.end).toBe(0);
  });
});

describe("applyChunk: continuous chunks", () => {
  it("concatenates a chunk whose dataStart equals the current end", () => {
    let buf = applyChunk(emptyOutput(), chunk(0, 2, [0x61, 0x62]));
    buf = applyChunk(buf, chunk(2, 4, [0x63, 0x64]));
    expect(Array.from(buf.bytes)).toEqual([0x61, 0x62, 0x63, 0x64]);
    expect(buf.totalLen).toBe(4);
    expect(buf.end).toBe(4);
    expect(buf.truncated).toBe(false);
  });

  it("accepts an empty first chunk without losing later bytes", () => {
    let buf = applyChunk(emptyOutput(), chunk(0, 0, []));
    buf = applyChunk(buf, chunk(0, 1, [0x41]));
    expect(Array.from(buf.bytes)).toEqual([0x41]);
    expect(buf.totalLen).toBe(1);
  });
});

describe("applyChunk: gaps and replacement", () => {
  it("replaces the buffer when dataStart is beyond the current end", () => {
    let buf = applyChunk(emptyOutput(), chunk(0, 2, [0x61, 0x62]));
    buf = applyChunk(buf, chunk(5, 7, [0x66, 0x67]));
    expect(Array.from(buf.bytes)).toEqual([0x66, 0x67]);
    expect(buf.totalLen).toBe(7);
    expect(buf.end).toBe(7);
  });

  it("replaces the buffer when dataStart is before the current end", () => {
    let buf = applyChunk(emptyOutput(), chunk(0, 3, [0x61, 0x62, 0x63]));
    buf = applyChunk(buf, chunk(1, 3, [0x7a]));
    expect(Array.from(buf.bytes)).toEqual([0x7a]);
    expect(buf.totalLen).toBe(3);
  });
});

describe("applyChunk: tail retention", () => {
  it("keeps only the last 65,536 bytes and reports the exact total", () => {
    const total = OUTPUT_TAIL_LIMIT + 10;
    const buf = applyChunk(emptyOutput(), chunk(0, total, patterned(total)));
    expect(buf.bytes.length).toBe(OUTPUT_TAIL_LIMIT);
    expect(buf.bufferStart).toBe(10);
    expect(buf.end).toBe(total);
    expect(buf.totalLen).toBe(total);
    expect(buf.truncated).toBe(true);
    expect(buf.bytes[0]).toBe(10 % 256);
    expect(buf.bytes[buf.bytes.length - 1]).toBe((total - 1) % 256);
  });

  it("drops the head as continuous chunks push the buffer past the limit", () => {
    const first = patterned(OUTPUT_TAIL_LIMIT);
    let buf = applyChunk(emptyOutput(), chunk(0, OUTPUT_TAIL_LIMIT, first));
    expect(buf.truncated).toBe(false);

    buf = applyChunk(buf, chunk(OUTPUT_TAIL_LIMIT, OUTPUT_TAIL_LIMIT + 3, [1, 2, 3]));
    expect(buf.bytes.length).toBe(OUTPUT_TAIL_LIMIT);
    expect(buf.bufferStart).toBe(3);
    expect(buf.totalLen).toBe(OUTPUT_TAIL_LIMIT + 3);
    expect(buf.truncated).toBe(true);
    expect(Array.from(buf.bytes.subarray(-3))).toEqual([1, 2, 3]);
    expect(buf.bytes[0]).toBe(first[3]);
  });

  it("does not truncate output of exactly 65,536 bytes", () => {
    const buf = applyChunk(emptyOutput(), chunk(0, OUTPUT_TAIL_LIMIT, patterned(OUTPUT_TAIL_LIMIT)));
    expect(buf.bytes.length).toBe(OUTPUT_TAIL_LIMIT);
    expect(buf.truncated).toBe(false);
  });
});

describe("applyChunk: byte preservation", () => {
  it("keeps 0x00, 0xFF and invalid UTF-8 bytes unchanged", () => {
    const bytes = [0x00, 0xff, 0xc3, 0x28, 0x80];
    const buf = applyChunk(emptyOutput(), chunk(0, bytes.length, bytes));
    expect(Array.from(buf.bytes)).toEqual(bytes);
  });

  it("rejects a chunk whose dataBase64 is not valid Base64", () => {
    expect(() =>
      applyChunk(emptyOutput(), { dataStart: 0, totalLen: 1, dataBase64: "!!!" }),
    ).toThrow(TypeError);
  });
});

describe("resetOutput", () => {
  it("returns an empty buffer regardless of previous content", () => {
    const buf = applyChunk(emptyOutput(), chunk(0, 2, [0x61, 0x62]));
    expect(resetOutput()).toEqual(emptyOutput());
    expect(buf.totalLen).toBe(2);
  });
});
