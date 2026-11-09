import { describe, expect, it } from "vitest";
import type { ExitStateDto } from "../../generated/ExitStateDto";
import type { MethodResultDto } from "../../generated/MethodResultDto";
import type { OverallDto } from "../../generated/OverallDto";
import type { TerminationDto } from "../../generated/TerminationDto";
import type { UnitDto } from "../../generated/UnitDto";
import type { VerdictDto } from "../../generated/VerdictDto";
import type { FailureKindDto } from "../../generated/FailureKindDto";
import type { UnsupportedReasonDto } from "../../generated/UnsupportedReasonDto";
import type { MismatchFieldDto } from "../../generated/MismatchFieldDto";
import {
  formatExitState,
  formatFailureKind,
  formatInvalidReason,
  formatMicros,
  formatMismatchField,
  formatMethodVerdict,
  formatOverall,
  formatPrepareMicros,
  formatUnit,
  formatUnsupportedReason,
  formatVerdict,
} from "./comparisonFormat";
import { DISCLAIMER_TEXT, STEP_COUNT_LEGEND } from "./definitions";

/** 順位・優劣を表す文言。比較結果の表示にこれらを含めない(要件 10.4) */
const RANKING_WORDS = /順位|最速|最遅|高速|低速|速い|遅い|速度差|優劣|優れ|劣っ|勝|負け|ランキング|ベスト|最も速|最も遅|一番/;

describe("formatMicros: 時間の自動単位換算", () => {
  it.each([
    [0, "0 μs"],
    [1, "1 μs"],
    [999, "999 μs"],
  ])("%i µs は μs 単位 %s", (micros, expected) => {
    expect(formatMicros(micros)).toBe(expected);
  });

  it("1000 µs は 1.00 ms(ms への境界)", () => {
    expect(formatMicros(1000)).toBe("1.00 ms");
  });

  it("1234 µs は 1.23 ms", () => {
    expect(formatMicros(1234)).toBe("1.23 ms");
  });

  it("999000 µs は 999.00 ms(ms の上限側)", () => {
    expect(formatMicros(999_000)).toBe("999.00 ms");
  });

  it("999999 µs は 1.00 s(表示の丸めで 1000.00 ms にせず s へ換算する境界)", () => {
    expect(formatMicros(999_999)).toBe("1.00 s");
  });

  it("1000000 µs は 1.00 s(s への境界)", () => {
    expect(formatMicros(1_000_000)).toBe("1.00 s");
  });

  it("1234567 µs は 1.23 s", () => {
    expect(formatMicros(1_234_567)).toBe("1.23 s");
  });
});

describe("formatPrepareMicros: 準備時間", () => {
  it("準備のない方式(null)は「準備なし」", () => {
    expect(formatPrepareMicros(null)).toBe("準備なし");
  });

  it("準備時間の値は formatMicros と同じ表示", () => {
    expect(formatPrepareMicros(1000)).toBe("1.00 ms");
  });
});

describe("formatUnit: 方式の単位 3 種", () => {
  it("素朴の単位は実行命令数と回数", () => {
    const unit: UnitDto = { kind: "naiveInstructions", count: 42 };
    expect(formatUnit(unit)).toBe("実行命令数 42");
  });

  it("最適化の単位は IR 操作回数と回数", () => {
    const unit: UnitDto = { kind: "irOperations", count: 7 };
    expect(formatUnit(unit)).toBe("IR 操作回数 7");
  });

  it("単位なし(WASM)は「なし」を明示する", () => {
    const unit: UnitDto = { kind: "none" };
    expect(formatUnit(unit)).toBe("なし");
  });
});

describe("formatExitState: 終了状態 3 種", () => {
  it("正常停止", () => {
    const termination: TerminationDto = { kind: "halted" };
    expect(formatExitState(termination)).toBe("正常停止");
  });

  it("ポインタ範囲外は pc・dp・方向を含む(左)", () => {
    const termination: TerminationDto = {
      kind: "error",
      error: { kind: "pointerOutOfBounds", pc: 3, dp: 0, direction: "left" },
    };
    expect(formatExitState(termination)).toBe("ポインタ範囲外(pc: 3, dp: 0, 左へ移動)");
  });

  it("ポインタ範囲外は方向が右のとき右へ移動と表示する", () => {
    const termination: TerminationDto = {
      kind: "error",
      error: { kind: "pointerOutOfBounds", pc: 12, dp: 65535, direction: "right" },
    };
    expect(formatExitState(termination)).toBe("ポインタ範囲外(pc: 12, dp: 65535, 右へ移動)");
  });

  it("命令数上限超過は上限と pc を含む", () => {
    const termination: TerminationDto = {
      kind: "error",
      error: { kind: "stepLimitExceeded", limit: 10, pc: 4 },
    };
    expect(formatExitState(termination)).toBe("命令数上限超過(上限: 10, pc: 4)");
  });
});

describe("formatVerdict: 判定", () => {
  it("基準", () => {
    const verdict: VerdictDto = { kind: "oracle" };
    expect(formatVerdict(verdict)).toBe("基準");
  });

  it("一致は記号と文言", () => {
    const verdict: VerdictDto = { kind: "match" };
    expect(formatVerdict(verdict)).toBe("一致 ✓");
  });

  it("不一致は記号と文言と項目名を並べる", () => {
    const verdict: VerdictDto = { kind: "mismatch", fields: ["pc", "output"] };
    expect(formatVerdict(verdict)).toBe("不一致 ✗(pc、出力)");
  });

  it("不一致の項目は 1 件でも項目名を含む", () => {
    const verdict: VerdictDto = { kind: "mismatch", fields: ["tape"] };
    expect(formatVerdict(verdict)).toBe("不一致 ✗(テープ)");
  });
});

describe("formatMismatchField: 不一致項目の日本語名(9 項目)", () => {
  it.each<[MismatchFieldDto, string]>([
    ["termination", "終了の種類"],
    ["errorValue", "エラー値"],
    ["pc", "pc"],
    ["dataPointer", "dp"],
    ["currentCell", "現在セル"],
    ["stepsExecuted", "実行数"],
    ["inputPosition", "入力位置"],
    ["output", "出力"],
    ["tape", "テープ"],
  ])("%s は %s", (field, expected) => {
    expect(formatMismatchField(field)).toBe(expected);
  });
});

describe("formatMethodVerdict / 未対応・失敗の理由", () => {
  it("完了した方式は判定の文言", () => {
    const exit: ExitStateDto = {
      termination: { kind: "halted" },
      pc: 0,
      dp: 0,
      currentCell: 0,
      stepsExecuted: 0,
      inputConsumed: 0,
      inputTotal: 0,
      outputTail: { dataStart: 0, totalLen: 0, dataBase64: "" },
      tapeWindow: { start: 0, cells: [], pointerOffset: 0 },
      tapeDigest: "0000000000000000",
    };
    const result: MethodResultDto = {
      kind: "completed",
      method: "optimized",
      verdict: { kind: "match" },
      exit,
      metrics: {
        prepareMicros: null,
        runMicros: 0,
        repetitions: 1,
        unit: { kind: "none" },
        artifactBytes: null,
        replayedSteps: null,
      },
    };
    expect(formatMethodVerdict(result)).toBe("一致 ✓");
  });

  it("未対応の方式は「未対応」", () => {
    const result: MethodResultDto = { kind: "unsupported", method: "wasm", reason: "moduleTooLarge" };
    expect(formatMethodVerdict(result)).toBe("未対応");
  });

  it("失敗の方式は「失敗」", () => {
    const result: MethodResultDto = { kind: "failed", method: "wasm", failure: "engineError" };
    expect(formatMethodVerdict(result)).toBe("失敗");
  });

  it.each<[UnsupportedReasonDto, string]>([
    ["moduleTooLarge", "モジュールが大きすぎるため未対応"],
    ["nestingTooDeep", "ループの入れ子が深すぎるため未対応"],
  ])("未対応の理由 %s は %s", (reason, expected) => {
    expect(formatUnsupportedReason(reason)).toBe(expected);
  });

  it.each<[FailureKindDto, string]>([
    ["watchdogTripped", "監視の上限を超えたため打ち切り"],
    ["outputLimitExceeded", "出力の上限を超えたため打ち切り"],
    ["engineError", "実行エンジンのエラー"],
    ["policyRejected", "安全方針により拒否"],
    ["inconsistentRepetitions", "繰り返しの結果が一致しなかった"],
  ])("失敗の理由 %s は %s", (failure, expected) => {
    expect(formatFailureKind(failure)).toBe(expected);
  });
});

describe("formatOverall: 総合結果", () => {
  it.each<[OverallDto, string]>([
    ["allMatch", "すべて一致"],
    ["mismatch", "不一致あり"],
    ["incomplete", "未対応または失敗あり"],
  ])("%s は %s", (overall, expected) => {
    expect(formatOverall(overall)).toBe(expected);
  });
});

describe("formatInvalidReason: 無効理由の文言", () => {
  it("上限なし", () => {
    expect(formatInvalidReason({ kind: "unlimited" })).toBe("上限なしでは比較できません。上限を指定してください");
  });

  it("上限の不正", () => {
    expect(formatInvalidReason({ kind: "invalidLimit" })).toBe("命令数の上限は 1 以上の整数で指定してください");
  });

  it("上限の最大超過は最大値を含む", () => {
    expect(formatInvalidReason({ kind: "limitTooLarge", max: 50_000_000 })).toBe(
      "命令数の上限は 50000000 以下で指定してください",
    );
  });

  it("入力の不正", () => {
    expect(formatInvalidReason({ kind: "invalidInput" })).toBe("入力が正しくありません。16 進の表記を確認してください");
  });

  it("ソースの超過", () => {
    expect(formatInvalidReason({ kind: "sourceTooLarge" })).toBe("ソースが 1 MiB を超えているため比較できません");
  });

  it("入力の超過", () => {
    expect(formatInvalidReason({ kind: "inputTooLarge" })).toBe("入力が 1 MiB を超えているため比較できません");
  });

  it("繰り返し回数の範囲外は最大値を含む", () => {
    expect(formatInvalidReason({ kind: "invalidRepetitions", max: 5 })).toBe("繰り返し回数は 1 以上 5 以下で指定してください");
  });
});

describe("表示文言の共通の性質", () => {
  it("判定の文言は記号と文言の両方を持つ(色に依存しない)", () => {
    expect(formatVerdict({ kind: "match" })).toMatch(/^一致 ✓$/);
    expect(formatVerdict({ kind: "mismatch", fields: ["pc"] })).toMatch(/^不一致 ✗\(/);
  });

  it("全ての表示文言・凡例・免責に順位・優劣を表す語を含まない", () => {
    const texts: string[] = [
      formatMicros(0),
      formatMicros(999),
      formatMicros(1000),
      formatMicros(1_000_000),
      formatPrepareMicros(null),
      formatUnit({ kind: "naiveInstructions", count: 1 }),
      formatUnit({ kind: "irOperations", count: 1 }),
      formatUnit({ kind: "none" }),
      formatExitState({ kind: "halted" }),
      formatExitState({ kind: "error", error: { kind: "stepLimitExceeded", limit: 1, pc: 0 } }),
      formatExitState({ kind: "error", error: { kind: "pointerOutOfBounds", pc: 0, dp: 0, direction: "left" } }),
      formatVerdict({ kind: "oracle" }),
      formatVerdict({ kind: "match" }),
      formatVerdict({ kind: "mismatch", fields: ["termination", "errorValue", "pc", "dataPointer", "currentCell", "stepsExecuted", "inputPosition", "output", "tape"] }),
      formatMethodVerdict({ kind: "unsupported", method: "wasm", reason: "nestingTooDeep" }),
      formatMethodVerdict({ kind: "failed", method: "wasm", failure: "watchdogTripped" }),
      formatUnsupportedReason("moduleTooLarge"),
      formatUnsupportedReason("nestingTooDeep"),
      formatFailureKind("watchdogTripped"),
      formatFailureKind("outputLimitExceeded"),
      formatFailureKind("engineError"),
      formatFailureKind("policyRejected"),
      formatFailureKind("inconsistentRepetitions"),
      formatOverall("allMatch"),
      formatOverall("mismatch"),
      formatOverall("incomplete"),
      formatInvalidReason({ kind: "unlimited" }),
      formatInvalidReason({ kind: "invalidLimit" }),
      formatInvalidReason({ kind: "limitTooLarge", max: 1 }),
      formatInvalidReason({ kind: "invalidInput" }),
      formatInvalidReason({ kind: "sourceTooLarge" }),
      formatInvalidReason({ kind: "inputTooLarge" }),
      formatInvalidReason({ kind: "invalidRepetitions", max: 5 }),
      DISCLAIMER_TEXT,
      ...STEP_COUNT_LEGEND.map((entry) => `${entry.term} ${entry.description}`),
    ];
    for (const text of texts) {
      expect(text, text).not.toMatch(RANKING_WORDS);
    }
  });
});
