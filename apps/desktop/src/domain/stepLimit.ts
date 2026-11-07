// 命令数上限の入力検証: 10 進整数の文字列 → StepLimitDto。React/Tauri に依存しない。
// 「上限なし」は文字列ではなく別の選択(真偽値)として扱い、`resolveStepLimit` で合成する。

import type { StepLimitDto } from "../generated/StepLimitDto";
import { DEFAULT_MAX_STEPS, MAX_STEP_LIMIT } from "./limits";

/** 入力欄の既定値(CLI の既定と同じ 10,000,000) */
export const DEFAULT_STEP_LIMIT_TEXT = String(DEFAULT_MAX_STEPS);

/** 「上限あり」の値。`StepLimitDto` の `limited` 側 */
export type LimitedStepLimit = Extract<StepLimitDto, { kind: "limited" }>;

/**
 * 上限入力の不正理由。
 * - `empty`: 空文字列
 * - `notInteger`: 10 進整数の形(先頭の `-` を除く数字のみ)ではない(小数・指数・空白・非数字・`+` 付きを含む)
 * - `belowMin`: 0 以下
 * - `aboveMax`: 2^53 − 1 を超える
 */
export type StepLimitReason = "empty" | "notInteger" | "belowMin" | "aboveMax";

export type ParseStepLimitResult = { ok: true; limit: LimitedStepLimit } | { ok: false; reason: StepLimitReason };

const INTEGER_SHAPE = /^-?[0-9]+$/;

/**
 * 上限の文字列を検証する。受理するのは 1 以上 2^53 − 1 以下の 10 進整数のみ。
 * 境界判定は `BigInt` で行い、2^53 を超える値も精度を失わずに `aboveMax` と判定する。
 * 前後の空白は許さない(入力欄の値をそのまま検証する)。
 */
export function parseStepLimit(text: string): ParseStepLimitResult {
  if (text === "") {
    return { ok: false, reason: "empty" };
  }
  if (!INTEGER_SHAPE.test(text)) {
    return { ok: false, reason: "notInteger" };
  }
  const n = BigInt(text);
  if (n < 1n) {
    return { ok: false, reason: "belowMin" };
  }
  if (n > BigInt(MAX_STEP_LIMIT)) {
    return { ok: false, reason: "aboveMax" };
  }
  return { ok: true, limit: { kind: "limited", maxSteps: Number(n) } };
}

/**
 * 「上限なし」の選択と入力欄の文字列から、実行に渡す上限を決める。
 * 「上限なし」が選ばれているときは入力欄の内容を見ない。
 */
export function resolveStepLimit(unlimited: boolean, text: string): ParseStepLimitResult | { ok: true; limit: StepLimitDto } {
  if (unlimited) {
    return { ok: true, limit: { kind: "unlimited" } };
  }
  return parseStepLimit(text);
}
