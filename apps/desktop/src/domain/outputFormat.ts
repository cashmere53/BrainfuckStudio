// 出力の表示変換(保持バイト列 → 表示用の値)。バイト列は変更しない。React/Tauri に依存しない。

import type { OutputBuffer } from "./outputBuffer";

/** 16 進表示の 1 行(16 バイト)。`offset` は出力全体における行頭バイト位置(8 桁 16 進) */
export type HexRow = { offset: string; hex: string };

/** 出力の要約。省略の有無と、省略後も正確な総バイト数を表示するために使う */
export type OutputSummary = {
  empty: boolean;
  totalLen: number;
  truncated: boolean;
  omittedLen: number;
};

const HEX_BYTES_PER_ROW = 16;
const CONTROL_PICTURE_BASE = 0x2400; // U+2400〜U+241F: C0 制御文字の記号
const DEL_PICTURE = 0x2421; // U+2421: DEL の記号

/**
 * テキスト表示: 保持バイト全体を連結してから UTF-8 として解釈する。
 * 不正なバイトは置換文字(U+FFFD)になり、改行(LF)とタブ以外の C0 制御文字と DEL は可視記号にする。
 * CR(0x0D)は改行とみなさず可視記号にする。
 */
export function renderText(buf: OutputBuffer): string {
  // ignoreBOM: 先頭の EF BB BF を出力バイトとして残す(除去しない)
  const text = new TextDecoder("utf-8", { ignoreBOM: true }).decode(buf.bytes);
  return visualizeControls(text);
}

/** 16 進表示: 16 バイト/行、行頭は出力全体における絶対バイト位置 */
export function renderHex(buf: OutputBuffer): HexRow[] {
  const rows: HexRow[] = [];
  for (let i = 0; i < buf.bytes.length; i += HEX_BYTES_PER_ROW) {
    const row = buf.bytes.subarray(i, i + HEX_BYTES_PER_ROW);
    rows.push({
      offset: formatOffset(buf.bufferStart + i),
      hex: Array.from(row, formatByte).join(" "),
    });
  }
  return rows;
}

/** 省略表示と総バイト数の表示に使う要約 */
export function summarizeOutput(buf: OutputBuffer): OutputSummary {
  return {
    empty: buf.totalLen === 0,
    totalLen: buf.totalLen,
    truncated: buf.truncated,
    omittedLen: buf.totalLen - buf.bytes.length,
  };
}

function visualizeControls(text: string): string {
  let out = "";
  for (const ch of text) {
    const code = ch.codePointAt(0) ?? 0;
    out += isControlToShow(code) ? String.fromCodePoint(pictureOf(code)) : ch;
  }
  return out;
}

function isControlToShow(code: number): boolean {
  if (code === 0x7f) return true;
  // 改行(LF 0x0A)とタブ(0x09)以外の C0 制御文字
  return code < 0x20 && code !== 0x09 && code !== 0x0a;
}

function pictureOf(code: number): number {
  return code === 0x7f ? DEL_PICTURE : CONTROL_PICTURE_BASE + code;
}

function formatOffset(position: number): string {
  return position.toString(16).toUpperCase().padStart(8, "0");
}

function formatByte(byte: number): string {
  return byte.toString(16).toUpperCase().padStart(2, "0");
}
