// 比較結果の日本語表示: 時間・単位・終了状態・判定・不一致項目・未対応/失敗の理由・総合結果・無効理由。
// 順位・優劣を表す文言は作らない(要件 10.4)。判定は記号と文言の両方を持つ(要件 10.6)。React・Tauri に依存しない。

import type { FailureKindDto } from "../../generated/FailureKindDto";
import type { MethodResultDto } from "../../generated/MethodResultDto";
import type { MismatchFieldDto } from "../../generated/MismatchFieldDto";
import type { OverallDto } from "../../generated/OverallDto";
import type { TerminationDto } from "../../generated/TerminationDto";
import type { UnitDto } from "../../generated/UnitDto";
import type { UnsupportedReasonDto } from "../../generated/UnsupportedReasonDto";
import type { VerdictDto } from "../../generated/VerdictDto";
import type { InvalidReason } from "./comparisonRequest";

/**
 * 時間の自動単位換算。入力はマイクロ秒(整数)。
 * - 1000 未満は μs(整数)
 * - 1 ms 以上で、小数 2 桁に丸めても 1000 ms 未満なら ms(小数 2 桁)
 * - それ以外は s(小数 2 桁)。丸めで「1000.00 ms」にならないよう、判定は丸めた値で行う
 */
export function formatMicros(micros: number): string {
  if (micros < 1000) {
    return `${micros} μs`;
  }
  const ms = (micros / 1000).toFixed(2);
  if (Number(ms) < 1000) {
    return `${ms} ms`;
  }
  return `${(micros / 1_000_000).toFixed(2)} s`;
}

/** 準備時間。準備のない方式(素朴)は null で「準備なし」と表示する */
export function formatPrepareMicros(micros: number | null): string {
  return micros === null ? "準備なし" : formatMicros(micros);
}

/** 方式の単位。単位なし(WASM)は「なし」を明示する(要件 10.5) */
export function formatUnit(unit: UnitDto): string {
  switch (unit.kind) {
    case "naiveInstructions":
      return `実行命令数 ${unit.count}`;
    case "irOperations":
      return `IR 操作回数 ${unit.count}`;
    case "none":
      return "なし";
    default:
      return assertNever(unit);
  }
}

/** 終了状態 3 種。エラーの場合は pc・dp・方向または上限を含む */
export function formatExitState(termination: TerminationDto): string {
  if (termination.kind === "halted") {
    return "正常停止";
  }
  const error = termination.error;
  switch (error.kind) {
    case "pointerOutOfBounds":
      return `ポインタ範囲外(pc: ${error.pc}, dp: ${error.dp}, ${error.direction === "left" ? "左" : "右"}へ移動)`;
    case "stepLimitExceeded":
      return `命令数上限超過(上限: ${error.limit}, pc: ${error.pc})`;
    default:
      return assertNever(error);
  }
}

/** 判定。基準・一致(✓)・不一致(✗ と不一致項目)。未対応・失敗は方式の表示で扱う */
export function formatVerdict(verdict: VerdictDto): string {
  switch (verdict.kind) {
    case "oracle":
      return "基準";
    case "match":
      return "一致 ✓";
    case "mismatch":
      return `不一致 ✗(${verdict.fields.map(formatMismatchField).join("、")})`;
    default:
      return assertNever(verdict);
  }
}

/** 不一致となった観測項目の日本語名 */
export function formatMismatchField(field: MismatchFieldDto): string {
  switch (field) {
    case "termination":
      return "終了の種類";
    case "errorValue":
      return "エラー値";
    case "pc":
      return "pc";
    case "dataPointer":
      return "dp";
    case "currentCell":
      return "現在セル";
    case "stepsExecuted":
      return "実行数";
    case "inputPosition":
      return "入力位置";
    case "output":
      return "出力";
    case "tape":
      return "テープ";
    default:
      return assertNever(field);
  }
}

/** 方式ごとの判定欄の文言。完了は判定、未対応・失敗は別の語にする(一致・不一致とは区別する) */
export function formatMethodVerdict(result: MethodResultDto): string {
  switch (result.kind) {
    case "completed":
      return formatVerdict(result.verdict);
    case "unsupported":
      return "未対応";
    case "failed":
      return "失敗";
    default:
      return assertNever(result);
  }
}

/** 未対応の理由 */
export function formatUnsupportedReason(reason: UnsupportedReasonDto): string {
  switch (reason) {
    case "moduleTooLarge":
      return "モジュールが大きすぎるため未対応";
    case "nestingTooDeep":
      return "ループの入れ子が深すぎるため未対応";
    default:
      return assertNever(reason);
  }
}

/** 失敗の理由 */
export function formatFailureKind(failure: FailureKindDto): string {
  switch (failure) {
    case "watchdogTripped":
      return "監視の上限を超えたため打ち切り";
    case "outputLimitExceeded":
      return "出力の上限を超えたため打ち切り";
    case "engineError":
      return "実行エンジンのエラー";
    case "policyRejected":
      return "安全方針により拒否";
    case "inconsistentRepetitions":
      return "繰り返しの結果が一致しなかった";
    default:
      return assertNever(failure);
  }
}

/** 総合結果の帯の文言 */
export function formatOverall(overall: OverallDto): string {
  switch (overall) {
    case "allMatch":
      return "すべて一致";
    case "mismatch":
      return "不一致あり";
    case "incomplete":
      return "未対応または失敗あり";
    default:
      return assertNever(overall);
  }
}

/** 要求を作れない理由の文言(入力欄の近くに関連付けて表示する) */
export function formatInvalidReason(reason: InvalidReason): string {
  switch (reason.kind) {
    case "unlimited":
      return "上限なしでは比較できません。上限を指定してください";
    case "invalidLimit":
      return "命令数の上限は 1 以上の整数で指定してください";
    case "limitTooLarge":
      return `命令数の上限は ${reason.max} 以下で指定してください`;
    case "invalidInput":
      return "入力が正しくありません。16 進の表記を確認してください";
    case "sourceTooLarge":
      return "ソースが 1 MiB を超えているため比較できません";
    case "inputTooLarge":
      return "入力が 1 MiB を超えているため比較できません";
    case "invalidRepetitions":
      return `繰り返し回数は 1 以上 ${reason.max} 以下で指定してください`;
    default:
      return assertNever(reason);
  }
}

function assertNever(value: never): never {
  throw new Error(`unexpected value: ${JSON.stringify(value)}`);
}
