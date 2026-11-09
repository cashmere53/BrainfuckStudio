// 比較画面の固定文言: 実行数の 2 つの定義(凡例)と、時間の免責。React・Tauri に依存しない。

export type LegendEntry = { term: string; description: string };

/** 実行数の 2 つの定義。素朴換算の値と方式固有の単位は比較しない(要件 7.5・7.6) */
export const STEP_COUNT_LEGEND: readonly LegendEntry[] = [
  {
    term: "素朴実装換算の実行数",
    description: "素朴インタプリタが同じ実行で数える命令数。全方式で同じ定義・同じ値になる",
  },
  {
    term: "方式固有の単位",
    description:
      "素朴 = 実行した命令数、最適化 = IR 操作の実行回数(連続命令やループを 1 回と数える)、WASM = 数えない",
  },
];

/** 時間の免責(要件 14.3) */
export const DISCLAIMER_TEXT = "実行時間は観察の対象であり、性能や正しさの保証ではありません";
