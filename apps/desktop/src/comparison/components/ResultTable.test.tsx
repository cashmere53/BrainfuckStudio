import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { encodeBase64 } from "../../domain/base64";
import type { ExitStateDto } from "../../generated/ExitStateDto";
import type { MethodResultDto } from "../../generated/MethodResultDto";
import { ResultTable } from "./ResultTable";

const encoder = new TextEncoder();

function exit(overrides: Partial<ExitStateDto> = {}): ExitStateDto {
  return {
    termination: { kind: "halted" },
    pc: 10,
    dp: 2,
    currentCell: 65,
    stepsExecuted: 906,
    inputConsumed: 0,
    inputTotal: 0,
    outputTail: { dataStart: 0, totalLen: 3, dataBase64: encodeBase64(encoder.encode("Hi\n")) },
    tapeWindow: { start: 0, cells: [0, 0, 65], pointerOffset: 2 },
    tapeDigest: "00000000deadbeef",
    ...overrides,
  };
}

const naive: MethodResultDto = {
  kind: "completed",
  method: "naive",
  verdict: { kind: "oracle" },
  exit: exit(),
  metrics: {
    prepareMicros: null,
    runMicros: 1500,
    repetitions: 3,
    unit: { kind: "naiveInstructions", count: 906 },
    artifactBytes: null,
    replayedSteps: null,
  },
};

const optimized: MethodResultDto = {
  kind: "completed",
  method: "optimized",
  verdict: { kind: "match" },
  exit: exit(),
  metrics: {
    prepareMicros: 120,
    runMicros: 800,
    repetitions: 3,
    unit: { kind: "irOperations", count: 300 },
    artifactBytes: null,
    replayedSteps: null,
  },
};

const wasm: MethodResultDto = {
  kind: "completed",
  method: "wasm",
  verdict: { kind: "match" },
  exit: exit(),
  metrics: {
    prepareMicros: 2500,
    runMicros: 1200,
    repetitions: 3,
    unit: { kind: "none" },
    artifactBytes: 512,
    replayedSteps: 0,
  },
};

function rowOf(label: string): HTMLTableRowElement {
  const row = screen.getByRole("rowheader", { name: label }).closest("tr");
  if (!row) {
    throw new Error(`row not found: ${label}`);
  }
  return row;
}

/** 方式の主行の各セル(0 方式、1 判定、2 準備時間、3 実行時間、4 換算実行数、5 単位、6 終了状態、7 成果物) */
function cellsOf(label: string): string[] {
  return Array.from(rowOf(label).cells).map((cell) => cell.textContent ?? "");
}

describe("ResultTable(要件 10.2・10.5〜10.7・10.11)", () => {
  it("表を名前つきで出し、列見出しは要件の表示項目を順に持つ", () => {
    render(<ResultTable methods={[naive, optimized, wasm]} />);

    const table = screen.getByRole("table", { name: "方式ごとの結果" });
    const headers = within(table)
      .getAllByRole("columnheader")
      .map((header) => header.textContent);
    expect(headers).toEqual(["方式", "判定", "準備時間", "実行時間", "換算実行数", "方式固有の単位", "終了状態", "成果物"]);
  });

  it("基準は「基準」、一致は「一致 ✓」と記号つきで示し、準備のない方式は「準備なし」とする", () => {
    render(<ResultTable methods={[naive, optimized]} />);

    const naiveCells = cellsOf("素朴インタプリタ");
    expect(naiveCells[1]).toBe("基準");
    expect(naiveCells[2]).toBe("準備なし");
    expect(naiveCells[3]).toBe("1.50 ms(3 回の中央値)");
    expect(naiveCells[4]).toBe("906");
    expect(naiveCells[5]).toBe("実行命令数 906");
    expect(naiveCells[6]).toBe("正常停止");
    expect(naiveCells[7]).toBe("—");

    const optimizedCells = cellsOf("最適化インタプリタ");
    expect(optimizedCells[1]).toBe("一致 ✓");
    expect(optimizedCells[2]).toBe("120 μs");
    expect(optimizedCells[3]).toBe("800 μs(3 回の中央値)");
    expect(optimizedCells[5]).toBe("IR 操作回数 300");
  });

  it("方式固有の単位がない WASM は「なし」と明示し、空欄にしない。成果物とホスト再生を示す", () => {
    render(<ResultTable methods={[naive, wasm]} />);

    const wasmCells = cellsOf("WASM コンパイル実行");
    expect(wasmCells[5]).toBe("なし");
    expect(wasmCells[7]).toBe("モジュール 512 バイト、ホスト再生 0 命令");
  });

  it("不一致は記号・文言・異なる項目名で示す", () => {
    const mismatched: MethodResultDto = {
      ...optimized,
      verdict: { kind: "mismatch", fields: ["output", "tape"] },
    };
    render(<ResultTable methods={[naive, mismatched]} />);

    const cells = cellsOf("最適化インタプリタ");
    expect(cells[1]).toBe("不一致 ✗(出力、テープ)");
    expect(cells[1]).not.toContain("一致 ✓");
  });

  it("未対応は一致・不一致とは別の語で示し、理由の分類を表示する。数値の欄は空欄にしない", () => {
    const unsupported: MethodResultDto = { kind: "unsupported", method: "wasm", reason: "nestingTooDeep" };
    render(<ResultTable methods={[naive, unsupported]} />);

    const cells = cellsOf("WASM コンパイル実行");
    expect(cells[1]).toContain("未対応");
    expect(cells[1]).toContain("ループの入れ子が深すぎるため未対応");
    expect(cells[1]).not.toContain("一致");
    expect(cells[3]).toBe("—");
    expect(cells[5]).toBe("—");
    expect(cells[6]).toBe("—");
  });

  it("未対応の理由「モジュールが大きすぎる」を表示する", () => {
    const unsupported: MethodResultDto = { kind: "unsupported", method: "wasm", reason: "moduleTooLarge" };
    render(<ResultTable methods={[naive, unsupported]} />);
    expect(cellsOf("WASM コンパイル実行")[1]).toContain("モジュールが大きすぎるため未対応");
    expect(cellsOf("WASM コンパイル実行")[1]).not.toContain("一致");
  });

  it("失敗は一致・不一致・未対応とは別の語で示し、失敗の理由を表示する", () => {
    const failed: MethodResultDto = { kind: "failed", method: "optimized", failure: "watchdogTripped" };
    render(<ResultTable methods={[naive, failed]} />);

    const cells = cellsOf("最適化インタプリタ");
    expect(cells[1]).toContain("失敗");
    expect(cells[1]).toContain("監視の上限を超えたため打ち切り");
    expect(cells[1]).not.toContain("一致");
    expect(cells[1]).not.toContain("未対応");
  });

  it("完了した方式ごとに終了状態の詳細を展開できる(未対応・失敗の行には詳細を出さない)", () => {
    const failed: MethodResultDto = { kind: "failed", method: "optimized", failure: "engineError" };
    render(<ResultTable methods={[naive, failed, wasm]} />);

    const table = screen.getByRole("table", { name: "方式ごとの結果" });
    const details = table.querySelectorAll("details");
    expect(details).toHaveLength(2);

    const naiveDetails = within(table).getByText("素朴インタプリタの終了状態の詳細").closest("details");
    expect(naiveDetails?.open).toBe(false);
    fireEvent.click(within(table).getByText("素朴インタプリタの終了状態の詳細"));
    expect(naiveDetails?.open).toBe(true);
    expect(naiveDetails?.textContent).toContain("正常停止");
  });

  it("時間の大小による順位・優劣・並び替えを表示しない(要件 10.11)", () => {
    const { container } = render(<ResultTable methods={[naive, optimized, wasm]} />);
    expect(container.textContent).not.toMatch(/順位|最速|最遅|速い|遅い|優劣|ランキング/);
  });
});
