import "./ResultTable.css";
import type { ComparisonMethodDto } from "../../generated/ComparisonMethodDto";
import type { MethodResultDto } from "../../generated/MethodResultDto";
import {
  formatExitState,
  formatFailureKind,
  formatMethodVerdict,
  formatMicros,
  formatPrepareMicros,
  formatUnit,
  formatUnsupportedReason,
} from "../domain/comparisonFormat";
import { ExitStateDetail } from "./ExitStateDetail";

export interface ResultTableProps {
  /** 方式ごとの結果。先頭は常に素朴(基準)。読み取りのみで、並べ替えや順位づけはしない(要件 10.11) */
  methods: MethodResultDto[];
}

const METHOD_LABELS: Record<ComparisonMethodDto, string> = {
  naive: "素朴インタプリタ",
  optimized: "最適化インタプリタ",
  wasm: "WASM コンパイル実行",
};

/** 完了以外の行で数値・単位の欄に入れる記号。空欄にしない */
const NOT_APPLICABLE = "—";

/**
 * 方式ごとの結果表(要件 10.2・10.5〜10.7・10.11)。
 * 1 方式 1 行で、方式・判定・準備時間・実行時間(繰り返し回数つき)・換算実行数・方式固有の単位・終了状態・成果物を示す。
 * 完了した行は、直後の行で終了状態の詳細を展開できる(`details`)。未対応・失敗の行は分類だけを示す。
 * 判定の文言は domain の整形関数で作り、一致・不一致・未対応・失敗を記号と文言で区別する。
 */
export function ResultTable(props: ResultTableProps) {
  const { methods } = props;
  return (
    <table className="result-table">
      <caption>方式ごとの結果</caption>
      <thead>
        <tr>
          <th scope="col">方式</th>
          <th scope="col">判定</th>
          <th scope="col">準備時間</th>
          <th scope="col">実行時間</th>
          <th scope="col">換算実行数</th>
          <th scope="col">方式固有の単位</th>
          <th scope="col">終了状態</th>
          <th scope="col">成果物</th>
        </tr>
      </thead>
      <tbody>
        {methods.map((result) => (
          <MethodRows key={result.method} result={result} />
        ))}
      </tbody>
    </table>
  );
}

function MethodRows({ result }: { result: MethodResultDto }) {
  const label = METHOD_LABELS[result.method];
  if (result.kind !== "completed") {
    return (
      <tr>
        <th scope="row">{label}</th>
        <td className="result-table-verdict">
          {formatMethodVerdict(result)}
          {result.kind === "unsupported" ? (
            <span className="result-table-reason">{formatUnsupportedReason(result.reason)}</span>
          ) : (
            <span className="result-table-reason">{formatFailureKind(result.failure)}</span>
          )}
        </td>
        <td>{NOT_APPLICABLE}</td>
        <td>{NOT_APPLICABLE}</td>
        <td>{NOT_APPLICABLE}</td>
        <td>{NOT_APPLICABLE}</td>
        <td>{NOT_APPLICABLE}</td>
        <td>{NOT_APPLICABLE}</td>
      </tr>
    );
  }

  const { metrics, exit } = result;
  return (
    <>
      <tr>
        <th scope="row">{label}</th>
        <td className="result-table-verdict">{formatMethodVerdict(result)}</td>
        <td>{formatPrepareMicros(metrics.prepareMicros)}</td>
        <td>{`${formatMicros(metrics.runMicros)}(${metrics.repetitions} 回の中央値)`}</td>
        <td>{exit.stepsExecuted}</td>
        <td>{formatUnit(metrics.unit)}</td>
        <td>{formatExitState(exit.termination)}</td>
        <td>
          {metrics.artifactBytes === null || metrics.replayedSteps === null
            ? NOT_APPLICABLE
            : `モジュール ${metrics.artifactBytes} バイト、ホスト再生 ${metrics.replayedSteps} 命令`}
        </td>
      </tr>
      <tr className="result-table-detail-row">
        <td colSpan={8}>
          <details>
            <summary>{`${label}の終了状態の詳細`}</summary>
            <ExitStateDetail exit={exit} />
          </details>
        </td>
      </tr>
    </>
  );
}
