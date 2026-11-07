import "./OutputView.css";
import { useId, useMemo } from "react";
import { OUTPUT_TAIL_LIMIT } from "../domain/limits";
import type { OutputBuffer } from "../domain/outputBuffer";
import { renderHex, renderText, summarizeOutput } from "../domain/outputFormat";
import type { OutputMode } from "../state/workspace";

export interface OutputViewProps {
  /** 表示する出力バッファ(末尾 65,536 バイトの保持分と総バイト数)。読み取りのみで、変更しない */
  output: OutputBuffer;
  /** 表示形式。親(Workspace.outputMode)が保持し、ここでは写すだけにする */
  mode: OutputMode;
  /** 表示形式の変更を親へ通知する(選んだモードを渡す) */
  onModeChange: (mode: OutputMode) => void;
}

const MODE_LABELS: Record<OutputMode, string> = { text: "テキスト", hex: "16 進" };

/**
 * 出力ビュー(要件 9.1・9.2・9.7〜9.9)。
 * テキスト/16 進の表示切替、総バイト数、先頭省略の表示、空表示を行う。
 * 表示形式は props で受け取り、切替は `onModeChange` で親へ通知する(表示のみに作用し、実行状態には触れない)。
 * 表示変換は `domain/outputFormat` に委譲し、選択中の形式に必要な変換だけを行う。
 */
export function OutputView(props: OutputViewProps) {
  const { output, mode, onModeChange } = props;
  const groupName = useId();
  const summary = summarizeOutput(output);
  const text = useMemo(
    () => (mode === "text" && !summary.empty ? renderText(output) : ""),
    [output, mode, summary.empty],
  );
  const hexRows = useMemo(
    () => (mode === "hex" && !summary.empty ? renderHex(output) : []),
    [output, mode, summary.empty],
  );

  return (
    <section className="output-view" aria-label="出力">
      <div className="output-view-header">
        <div className="output-view-modes" role="radiogroup" aria-label="出力の表示形式">
          {(Object.keys(MODE_LABELS) as OutputMode[]).map((value) => (
            <label key={value} className="output-view-mode">
              <input
                type="radio"
                name={groupName}
                value={value}
                checked={mode === value}
                onChange={() => onModeChange(value)}
              />
              {MODE_LABELS[value]}
            </label>
          ))}
        </div>
        <p className="output-view-total">総バイト数: {summary.totalLen} バイト</p>
      </div>
      {summary.truncated ? (
        <p className="output-view-omitted">
          先頭 {summary.omittedLen} バイトを省略しています。末尾 {OUTPUT_TAIL_LIMIT} バイトを表示しています。
        </p>
      ) : null}
      {summary.empty ? (
        <p className="output-view-empty">出力はまだありません</p>
      ) : mode === "text" ? (
        <pre className="output-view-text" aria-label="出力の内容(テキスト)" tabIndex={0}>
          {text}
        </pre>
      ) : (
        <ol className="output-view-hex" aria-label="出力の内容(16 進)" tabIndex={0}>
          {hexRows.map((row) => (
            <li key={row.offset} className="output-view-hex-row">
              <span className="output-view-offset">{row.offset}</span>
              <span className="output-view-bytes">{row.hex}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
