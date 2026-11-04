// 入力設定の純粋ロジック: テキスト/16 進 → 入力バイト列、モード切替、総バイト数。
// React/Tauri に依存しない。

import { MAX_INPUT_BYTES } from "./limits";

export type InputMode = "text" | "hex";

/** 入力設定モデル。`text` はモードに応じて平文または 16 進表記の文字列を保持する */
export type InputModel = { mode: InputMode; text: string };

/**
 * 入力エラー。`index` は元の文字列上の UTF-16 位置(invalidChar のみ)。
 * 16 進数字の総数が奇数の場合は位置を持たない。
 */
export type InputError =
  | { kind: "invalidChar"; index: number }
  | { kind: "oddDigits" }
  | { kind: "tooLarge" };

export type ParseInputResult = { ok: true; bytes: Uint8Array } | { ok: false; error: InputError };

/** モード切替の失敗理由。入力エラーの種類か、UTF-8 として不正であることを表す */
export type SwitchModeResult =
  | { ok: true; model: InputModel }
  | { ok: false; reason: InputError["kind"] | "notUtf8" };

const WHITESPACE = new Set([" ", "\t", "\n", "\r"]);

export function parseInput(model: InputModel): ParseInputResult {
  return model.mode === "text" ? parseText(model.text) : parseHex(model.text);
}

/** 入力バイト列の総数。解析できない場合は null */
export function byteCount(model: InputModel): number | null {
  const result = parseInput(model);
  return result.ok ? result.bytes.length : null;
}

/**
 * モードを切り替える。
 * - テキスト → 16 進: 常に成功(同じバイト列を 16 進表記にする。上限超過は 16 進側のエラーとして残す)
 * - 16 進 → テキスト: 入力バイト列が UTF-8 として正しい場合のみ成功。失敗時はモードを変えず理由を返す
 */
export function switchMode(model: InputModel, to: InputMode): SwitchModeResult {
  if (model.mode === to) {
    return { ok: true, model };
  }
  if (to === "hex") {
    const bytes = new TextEncoder().encode(model.text);
    return { ok: true, model: { mode: "hex", text: bytesToHex(bytes) } };
  }
  const parsed = parseHex(model.text);
  if (!parsed.ok) {
    return { ok: false, reason: parsed.error.kind };
  }
  try {
    // BOM(EF BB BF)を消さないよう ignoreBOM を有効にし、バイト列をそのまま保つ
    const text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(parsed.bytes);
    return { ok: true, model: { mode: "text", text } };
  } catch {
    return { ok: false, reason: "notUtf8" };
  }
}

function parseText(value: string): ParseInputResult {
  // UTF-8 の 1 バイト以上を各 UTF-16 単位が占めるため、文字数が上限を超えれば必ず超過する。
  // 巨大な文字列を符号化する前に拒否する。
  if (value.length > MAX_INPUT_BYTES) {
    return { ok: false, error: { kind: "tooLarge" } };
  }
  // 実行環境によっては TextEncoder が別 realm の Uint8Array を返すため、このモジュールの Uint8Array へ写す
  const bytes = new Uint8Array(new TextEncoder().encode(value));
  if (bytes.length > MAX_INPUT_BYTES) {
    return { ok: false, error: { kind: "tooLarge" } };
  }
  return { ok: true, bytes };
}

function parseHex(value: string): ParseInputResult {
  const nibbles = new Uint8Array(value.length);
  let count = 0;
  for (let i = 0; i < value.length; i++) {
    if (WHITESPACE.has(value.charAt(i))) {
      continue;
    }
    const nibble = hexNibble(value.charCodeAt(i));
    if (nibble === null) {
      return { ok: false, error: { kind: "invalidChar", index: i } };
    }
    nibbles[count++] = nibble;
  }
  if (count % 2 !== 0) {
    return { ok: false, error: { kind: "oddDigits" } };
  }
  const size = count / 2;
  if (size > MAX_INPUT_BYTES) {
    return { ok: false, error: { kind: "tooLarge" } };
  }
  const bytes = new Uint8Array(size);
  for (let i = 0; i < size; i++) {
    bytes[i] = ((nibbles[2 * i] ?? 0) << 4) | (nibbles[2 * i + 1] ?? 0);
  }
  return { ok: true, bytes };
}

function hexNibble(code: number): number | null {
  if (code >= 0x30 && code <= 0x39) return code - 0x30; // 0-9
  if (code >= 0x41 && code <= 0x46) return code - 0x41 + 10; // A-F
  if (code >= 0x61 && code <= 0x66) return code - 0x61 + 10; // a-f
  return null;
}

/** 2 桁の大文字 16 進を空白区切りで並べる(例: "48 69") */
function bytesToHex(bytes: Uint8Array): string {
  const parts: string[] = [];
  for (const byte of bytes) {
    parts.push(byte.toString(16).toUpperCase().padStart(2, "0"));
  }
  return parts.join(" ");
}
