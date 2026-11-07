// 出力バッファ: バックエンドの差分チャンク(OutputChunkDto)を末尾 65,536 バイトだけ保持する。
// React/Tauri に依存しない。表示変換は outputFormat.ts が担う。

import type { OutputChunkDto } from "../generated/OutputChunkDto";
import { decodeBase64 } from "./base64";
import { OUTPUT_TAIL_LIMIT } from "./limits";

/**
 * 表示用の出力バッファ。
 * - `bytes`: 出力全体のうち末尾側の保持バイト列(バイト単位で無加工)
 * - `bufferStart`: `bytes[0]` が出力全体の何バイト目か
 * - `end`: `bufferStart + bytes.length`(次の連続チャンクの `dataStart` と一致させる位置)
 * - `totalLen`: 出力全体の総バイト数(省略分を含む)
 * - `truncated`: 先頭が省略されているか(`bufferStart > 0`)
 */
export type OutputBuffer = {
  readonly bytes: Uint8Array;
  readonly bufferStart: number;
  readonly end: number;
  readonly totalLen: number;
  readonly truncated: boolean;
};

/** 出力がまだない状態(セッション開始直後・リセット後) */
export function emptyOutput(): OutputBuffer {
  return makeBuffer(new Uint8Array(0), 0, 0);
}

/** リセット: 保持バイトと総数をすべて捨てる */
export function resetOutput(): OutputBuffer {
  return emptyOutput();
}

/**
 * 差分チャンクを適用する。
 * - `chunk.dataStart` が現在の `end` と一致する場合は連結する(連続)
 * - 一致しない場合は、チャンクの内容でバッファを置き換える(不連続)
 * 結果は末尾 65,536 バイトに切り詰め、`totalLen` は `chunk.totalLen` をそのまま採る。
 * `dataBase64` が Base64 として不正な場合は契約違反として TypeError を投げる。
 */
export function applyChunk(buf: OutputBuffer, chunk: OutputChunkDto): OutputBuffer {
  const decoded = decodeBase64(chunk.dataBase64);
  if (!decoded.ok) {
    throw new TypeError("出力チャンクの dataBase64 が Base64 として不正です");
  }
  if (chunk.dataStart === buf.end) {
    return keepTail(concat(buf.bytes, decoded.bytes), buf.bufferStart, chunk.totalLen);
  }
  return keepTail(decoded.bytes, chunk.dataStart, chunk.totalLen);
}

/** `bytes` の先頭が `start` バイト目であるとして、末尾 65,536 バイトだけを残す */
function keepTail(bytes: Uint8Array, start: number, totalLen: number): OutputBuffer {
  const excess = Math.max(0, bytes.length - OUTPUT_TAIL_LIMIT);
  // slice で複製し、切り捨てた先頭を参照し続けない
  return makeBuffer(bytes.slice(excess), start + excess, totalLen);
}

function makeBuffer(bytes: Uint8Array, bufferStart: number, totalLen: number): OutputBuffer {
  return {
    bytes,
    bufferStart,
    end: bufferStart + bytes.length,
    totalLen,
    truncated: bufferStart > 0,
  };
}

function concat(head: Uint8Array, tail: Uint8Array): Uint8Array {
  const joined = new Uint8Array(head.length + tail.length);
  joined.set(head, 0);
  joined.set(tail, head.length);
  return joined;
}
