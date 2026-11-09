import "./TapeStrip.css";
import type { TapeWindowDto } from "../../generated/TapeWindowDto";

export interface TapeStripProps {
  /** テープ窓(バックエンドが計算済みの窓。ここでは写すだけにする) */
  window: TapeWindowDto;
}

/**
 * 文字の表示規則: 可視 ASCII(0x21〜0x7E)は文字、空白(0x20)は記号、それ以外は表示なしの記号。
 * デバッガのテープ表示と同じ規則。
 */
export function tapeStripCharacter(value: number): string {
  if (value === 0x20) {
    return "␠";
  }
  if (value >= 0x21 && value <= 0x7e) {
    return String.fromCharCode(value);
  }
  return "—";
}

function hexOf(value: number): string {
  return value.toString(16).toUpperCase().padStart(2, "0");
}

/**
 * ポインタ周辺のテープの横並び(要件 10.3)。
 * 各セルに添字・10 進・16 進・文字を示し、dp のセルは枠・「▼ dp」・aria-current で色以外にも強調する。
 * 窓が横に長くなる場合は領域内でスクロールする。
 */
export function TapeStrip(props: TapeStripProps) {
  const { window } = props;
  return (
    <ol className="tape-strip" aria-label="テープ(ポインタ周辺)" tabIndex={0}>
      {window.cells.map((value, offset) => {
        const index = window.start + offset;
        const isPointer = offset === window.pointerOffset;
        const character = tapeStripCharacter(value);
        const label = `セル ${index}、10 進 ${value}、16 進 ${hexOf(value)}、文字 ${character === "—" ? "表示なし" : character}`;
        return (
          <li
            key={index}
            className={isPointer ? "tape-strip-cell tape-strip-cell-pointer cue-frame" : "tape-strip-cell"}
            aria-label={label}
            aria-current={isPointer ? "true" : undefined}
          >
            <span className="tape-strip-cell-index">{index}</span>
            <span className="tape-strip-cell-dec">{value}</span>
            <span className="tape-strip-cell-hex">{hexOf(value)}</span>
            <span className="tape-strip-cell-char">{character}</span>
            {isPointer ? <span className="tape-strip-cell-pointer-label">▼ dp</span> : null}
          </li>
        );
      })}
    </ol>
  );
}
