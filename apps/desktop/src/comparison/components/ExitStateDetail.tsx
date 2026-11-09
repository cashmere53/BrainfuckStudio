import "./ExitStateDetail.css";
import { useState } from "react";
import type { ExitStateDto } from "../../generated/ExitStateDto";
import type { RuntimeErrorDto } from "../../generated/RuntimeErrorDto";
import { applyChunk, emptyOutput } from "../../domain/outputBuffer";
import type { OutputMode } from "../../state/workspace";
import { OutputView } from "../../components/OutputView";
import { formatExitState } from "../domain/comparisonFormat";
import { TapeStrip } from "./TapeStrip";

export interface ExitStateDetailProps {
  /** 方式の終了状態(観測値の DTO 表現)。読み取りのみ */
  exit: ExitStateDto;
}

/** エラーのフィールド(種類ごとの値。正常停止では空) */
function errorRows(error: RuntimeErrorDto): Array<{ term: string; value: string }> {
  if (error.kind === "pointerOutOfBounds") {
    return [
      { term: "エラーの pc", value: String(error.pc) },
      { term: "エラーの dp", value: String(error.dp) },
      { term: "移動方向", value: error.direction === "left" ? "左" : "右" },
    ];
  }
  return [
    { term: "上限", value: String(error.limit) },
    { term: "エラーの pc", value: String(error.pc) },
  ];
}

/**
 * 終了状態の詳細(要件 10.3)。
 * 終了種別とエラーのフィールド、最終の pc・dp・現在セル・実行数・入力読取位置、テープ全体の指紋を説明一覧で示し、
 * 出力は desktop-gui の出力ビュー(テキスト/16 進)を再利用し、ポインタ周辺のテープは TapeStrip で示す。
 * 出力の表示形式だけをこのコンポーネントの状態として持つ(実行状態には触れない)。
 */
export function ExitStateDetail(props: ExitStateDetailProps) {
  const { exit } = props;
  const [mode, setMode] = useState<OutputMode>("text");
  const output = applyChunk(emptyOutput(), exit.outputTail);
  const errors = exit.termination.kind === "error" ? errorRows(exit.termination.error) : [];

  return (
    <div className="exit-state-detail">
      <dl className="exit-state-detail-fields">
        <div>
          <dt>終了の種類</dt>
          <dd>{formatExitState(exit.termination)}</dd>
        </div>
        {errors.map((row) => (
          <div key={row.term}>
            <dt>{row.term}</dt>
            <dd>{row.value}</dd>
          </div>
        ))}
        <div>
          <dt>最終の pc</dt>
          <dd>{exit.pc}</dd>
        </div>
        <div>
          <dt>最終の dp</dt>
          <dd>{exit.dp}</dd>
        </div>
        <div>
          <dt>現在セル</dt>
          <dd>{exit.currentCell}</dd>
        </div>
        <div>
          <dt>実行数(素朴換算)</dt>
          <dd>{exit.stepsExecuted}</dd>
        </div>
        <div>
          <dt>入力読取位置</dt>
          <dd>{`${exit.inputConsumed} / ${exit.inputTotal}`}</dd>
        </div>
        <div>
          <dt>テープの指紋</dt>
          <dd className="exit-state-detail-digest">{exit.tapeDigest}</dd>
        </div>
      </dl>
      <OutputView output={output} mode={mode} onModeChange={setMode} />
      <TapeStrip window={exit.tapeWindow} />
    </div>
  );
}
