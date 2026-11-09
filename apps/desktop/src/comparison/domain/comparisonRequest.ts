// 比較の要求導出: 共有値(ソース・入力・上限)と画面の選択から RunComparisonRequest を作る純粋関数。
// 無効な場合は理由を返し、要求を作らない(要件 11.5〜11.7)。React・Tauri に依存しない。

import type { ComparisonInfoDto } from "../../generated/ComparisonInfoDto";
import type { ComparisonMethodDto } from "../../generated/ComparisonMethodDto";
import type { RunComparisonRequest } from "../../generated/RunComparisonRequest";
import type { WorkspaceValues } from "../../state/workspace";
import { encodeBase64 } from "../../domain/base64";
import { parseInput } from "../../domain/inputBytes";
import { MAX_SOURCE_BYTES } from "../../domain/limits";
import { parseStepLimit } from "../../domain/stepLimit";

/** 画面で選んだ方式(素朴は常に実行されるため選択に含めない)と繰り返し回数 */
export type Selection = { optimized: boolean; wasm: boolean; repetitions: number };

/** 要求を作れない理由 */
export type InvalidReason =
  | { kind: "unlimited" }
  | { kind: "invalidLimit" }
  | { kind: "limitTooLarge"; max: number }
  | { kind: "invalidInput" }
  | { kind: "sourceTooLarge" }
  | { kind: "inputTooLarge" }
  | { kind: "invalidRepetitions"; max: number };

export type DeriveRequestResult = { ok: true; request: RunComparisonRequest } | { ok: false; reason: InvalidReason };

/**
 * 共有値と選択から比較の要求を導出する。判定の順序は 上限 → 入力 → ソース → 繰り返し。
 * - 上限: 「上限なし」は unlimited。不正な文字列は invalidLimit。比較用の最大値を超える値(2^53 − 1 以下を含む)は limitTooLarge
 * - 入力: 16 進の不正は invalidInput、1 MiB 超は inputTooLarge。受理時は UTF-8 のバイト列を Base64 にする
 * - ソース: UTF-8 のバイト数が 1 MiB 超なら sourceTooLarge(文字数ではなくバイト数で判定する)
 * - 繰り返し: 1 以上 最大値以下の整数のみ。範囲外は invalidRepetitions
 */
export function deriveRequest(
  values: WorkspaceValues,
  selection: Selection,
  info: ComparisonInfoDto,
): DeriveRequestResult {
  if (values.limit.unlimited) {
    return invalid({ kind: "unlimited" });
  }
  const limit = parseStepLimit(values.limit.text);
  if (!limit.ok) {
    return invalid(limit.reason === "aboveMax" ? { kind: "limitTooLarge", max: info.maxSteps } : { kind: "invalidLimit" });
  }
  if (limit.limit.maxSteps > info.maxSteps) {
    return invalid({ kind: "limitTooLarge", max: info.maxSteps });
  }

  const input = parseInput(values.input);
  if (!input.ok) {
    return invalid(input.error.kind === "tooLarge" ? { kind: "inputTooLarge" } : { kind: "invalidInput" });
  }

  // UTF-8 のバイト数は UTF-16 のコード単位数以上のため、文字数が上限を超えれば符号化せずに超過と判定できる
  if (values.source.length > MAX_SOURCE_BYTES || new TextEncoder().encode(values.source).length > MAX_SOURCE_BYTES) {
    return invalid({ kind: "sourceTooLarge" });
  }

  const { repetitions } = selection;
  if (!Number.isInteger(repetitions) || repetitions < 1 || repetitions > info.maxRepetitions) {
    return invalid({ kind: "invalidRepetitions", max: info.maxRepetitions });
  }

  return {
    ok: true,
    request: {
      source: values.source,
      inputBase64: encodeBase64(input.bytes),
      maxSteps: limit.limit.maxSteps,
      methods: selectedMethods(selection),
      repetitions,
    },
  };
}

/** 追加で実行する方式。最適化・WASM の順。素朴は含めない */
function selectedMethods(selection: Selection): ComparisonMethodDto[] {
  const methods: ComparisonMethodDto[] = [];
  if (selection.optimized) {
    methods.push("optimized");
  }
  if (selection.wasm) {
    methods.push("wasm");
  }
  return methods;
}

function invalid(reason: InvalidReason): DeriveRequestResult {
  return { ok: false, reason };
}
