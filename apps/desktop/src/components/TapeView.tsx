import "./TapeView.css";
import { useId } from "react";
import type { TapeWindowDto } from "../generated/TapeWindowDto";
import { MAX_TAPE_RADIUS } from "../domain/limits";
import { initialDisplay } from "../state/debuggerStore";

export interface TapeViewProps {
  /** テープ窓。未開始(セッションなし)のときは null で、全 0・dp 0 の初期窓を表示する */
  tapeWindow: TapeWindowDto | null;
  /** 表示半径(0〜64)。セレクタの現在値 */
  radius: number;
  /** 半径の変更(整数で通知する) */
  onRadiusChange: (radius: number) => void;
}

/** 表示可能な ASCII(0x21〜0x7E)は文字を、空白(0x20)は識別記号を、それ以外は表示なしの記号を返す */
export function tapeCellCharacter(value: number): string {
  if (value === 0x20) {
    return "␠";
  }
  if (value >= 0x21 && value <= 0x7e) {
    return String.fromCharCode(value);
  }
  return "—";
}

/** 半径を 0〜MAX_TAPE_RADIUS の整数に丸める(範囲外・非整数・非有限値を select と窓計算に渡さない) */
function clampRadius(radius: number): number {
  if (!Number.isFinite(radius)) {
    return 0;
  }
  return Math.min(MAX_TAPE_RADIUS, Math.max(0, Math.round(radius)));
}

function hexOf(value: number): string {
  return value.toString(16).toUpperCase().padStart(2, "0");
}

/**
 * テープビュー(要件 8.1〜8.3・8.5・8.7・14.4)。
 * バックエンドの窓(切り詰め済み)のセルを横一列に、添字・10 進・16 進・ASCII 文字つきで表示する。
 * dp のセルは枠・「▼ dp」ラベル・aria-current で色以外にも強調する。
 * 窓の計算は上流が行うため、ここでは受け取った窓を写すだけにする。
 */
export function TapeView(props: TapeViewProps) {
  const radius = clampRadius(props.radius);
  const { onRadiusChange } = props;
  const selectId = useId();
  const tape: TapeWindowDto = props.tapeWindow ?? initialDisplay(radius).tapeWindow;

  const radiusOptions = Array.from({ length: MAX_TAPE_RADIUS + 1 }, (_, i) => i);

  return (
    <section className="tape-view" aria-label="テープ">
      <div className="tape-view-header">
        <label htmlFor={selectId}>表示半径</label>
        <select
          id={selectId}
          value={radius}
          onChange={(event) => onRadiusChange(Number(event.target.value))}
        >
          {radiusOptions.map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </div>
      <ol className="tape-cells">
        {tape.cells.map((value, offset) => {
          const index = tape.start + offset;
          const isPointer = offset === tape.pointerOffset;
          const character = tapeCellCharacter(value);
          const label = `セル ${index}、10 進 ${value}、16 進 ${hexOf(value)}、文字 ${character === "—" ? "表示なし" : character}`;
          return (
            <li
              key={index}
              className={isPointer ? "tape-cell tape-cell-pointer cue-frame" : "tape-cell"}
              aria-label={label}
              aria-current={isPointer ? "true" : undefined}
            >
              <span className="tape-cell-index">{index}</span>
              <span className="tape-cell-dec">{value}</span>
              <span className="tape-cell-hex">{hexOf(value)}</span>
              <span className="tape-cell-char">{character}</span>
              {isPointer ? <span className="tape-cell-pointer-label">▼ dp</span> : null}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
