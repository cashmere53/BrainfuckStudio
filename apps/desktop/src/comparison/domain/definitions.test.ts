import { describe, expect, it } from "vitest";
import { DISCLAIMER_TEXT, STEP_COUNT_LEGEND } from "./definitions";

describe("STEP_COUNT_LEGEND: 実行数の 2 つの定義", () => {
  it("素朴実装換算の実行数と方式固有の単位の 2 項目だけを持つ", () => {
    expect(STEP_COUNT_LEGEND.map((entry) => entry.term)).toEqual(["素朴実装換算の実行数", "方式固有の単位"]);
  });

  it("素朴実装換算の実行数は全方式で同じ定義・同じ値になると説明する", () => {
    expect(STEP_COUNT_LEGEND[0]).toEqual({
      term: "素朴実装換算の実行数",
      description: "素朴インタプリタが同じ実行で数える命令数。全方式で同じ定義・同じ値になる",
    });
  });

  it("方式固有の単位は素朴・最適化・WASM のそれぞれの数え方を説明する", () => {
    expect(STEP_COUNT_LEGEND[1]).toEqual({
      term: "方式固有の単位",
      description:
        "素朴 = 実行した命令数、最適化 = IR 操作の実行回数(連続命令やループを 1 回と数える)、WASM = 数えない",
    });
  });
});

describe("DISCLAIMER_TEXT: 時間は観察対象", () => {
  it("実行時間は観察の対象であり、性能や正しさの保証ではないと明記する", () => {
    expect(DISCLAIMER_TEXT).toBe("実行時間は観察の対象であり、性能や正しさの保証ではありません");
  });
});
