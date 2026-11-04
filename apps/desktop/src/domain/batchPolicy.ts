// 適応バッチ幅の純粋関数。直前のバッチの往復時間から次のバッチ命令数を決める。
// React/Tauri に依存しない。

import { MAX_BATCH_SIZE, MIN_BATCH_SIZE } from "./limits";

/** この往復時間(ms)未満なら倍加する */
const FAST_BELOW_MS = 15;
/** この往復時間(ms)超なら半減する */
const SLOW_ABOVE_MS = 60;

/**
 * 次のバッチ命令数。
 * - `elapsedMs < 15` なら 2 倍(上限 1,000,000)
 * - `elapsedMs > 60` なら半分(下限 1,000)
 * - それ以外(15 以上 60 以下)は据え置き
 * 結果は常に 1,000..1,000,000 の範囲に収める。
 */
export function nextBatchSize(prev: number, elapsedMs: number): number {
  let next = prev;
  if (elapsedMs < FAST_BELOW_MS) {
    next = prev * 2;
  } else if (elapsedMs > SLOW_ABOVE_MS) {
    next = Math.floor(prev / 2);
  }
  return Math.min(MAX_BATCH_SIZE, Math.max(MIN_BATCH_SIZE, next));
}
