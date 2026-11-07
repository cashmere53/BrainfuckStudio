// 命令位置のスナップ規則。React/Tauri に依存しない。
// 表は design.md「instructionScan」と要件 6.1〜6.4 に対応する。
// 位置はすべて文書先頭からの UTF-16 単位の絶対位置(CodeMirror の位置と同じ)。
// 命令文字以外(コメント)はすべて無視されるため、命令でない文字はスキャンで読み飛ばす。

import type { Text } from "@codemirror/state";

/** Brainfuck の命令文字の集合(これ以外の文字はコメント) */
export const INSTRUCTION_CHARS = "><+-.,[]";

/** `text` の `from` 以降(UTF-16 単位の文字列内位置)で最初の命令文字の位置。無ければ -1 */
function indexOfInstruction(text: string, from: number): number {
  for (let i = from; i < text.length; i++) {
    if (INSTRUCTION_CHARS.includes(text.charAt(i))) {
      return i;
    }
  }
  return -1;
}

/**
 * 1 始まりの行 `lineNo` の中で最初の命令の文書内 UTF-16 位置を返す。
 * 行に命令が無ければ `null`。`lineNo` が範囲外なら `RangeError`。
 * 行の外(次の行など)は探索しない。
 */
export function firstInstructionInLine(doc: Text, lineNo: number): number | null {
  if (!Number.isInteger(lineNo) || lineNo < 1 || lineNo > doc.lines) {
    throw new RangeError(`line number out of range: ${lineNo}`);
  }
  const line = doc.line(lineNo);
  const i = indexOfInstruction(line.text, 0);
  return i < 0 ? null : line.from + i;
}

/**
 * 文書内の位置 `pos` 以降(`pos` を含む)で最初の命令の文書内 UTF-16 位置を返す。
 * 改行をまたいで探索する。見つからなければ `null`。
 * `pos` が `0..doc.length` の外なら `RangeError`。
 */
export function firstInstructionFrom(doc: Text, pos: number): number | null {
  if (!Number.isInteger(pos) || pos < 0 || pos > doc.length) {
    throw new RangeError(`position out of range: ${pos}`);
  }
  const start = doc.lineAt(pos);
  const first = indexOfInstruction(start.text, pos - start.from);
  if (first >= 0) {
    return start.from + first;
  }
  for (let n = start.number + 1; n <= doc.lines; n++) {
    const line = doc.line(n);
    const i = indexOfInstruction(line.text, 0);
    if (i >= 0) {
      return line.from + i;
    }
  }
  return null;
}
