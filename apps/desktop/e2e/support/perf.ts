// 時間計測の補助。計測は `perf` プロジェクトだけで行い、通常の検証の結果は変えない。
// 値を一定間隔で標本化し、値が変わった間隔の最大値を求め、実測値を注釈と JSON の添付に残す。
import type { TestInfo } from "@playwright/test";

/** 要件の 1 秒に対する閾値(1 秒 × 1.5)。計測の揺らぎに余裕を持たせる */
export const PERF_THRESHOLD_MS = 1500;

/** ある時点(計測の開始からの ms)の値 */
export type Sample = { atMs: number; value: number };

const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * `read` を `intervalMs` ごとに呼び、`durationMs` の間の標本を返す。
 * 時刻は呼び出しの開始からの経過(ms)で、読み取りの所要時間も含めて記録する(読み取り後の時刻)。
 * 時刻と待機は `deps` で差し替えられる(既定は performance.now と setTimeout)。
 */
export async function sample(
  read: () => Promise<number>,
  options: { intervalMs: number; durationMs: number },
  deps: { now?: () => number; sleep?: (ms: number) => Promise<void> } = {},
): Promise<Sample[]> {
  const now = deps.now ?? (() => performance.now());
  const sleep = deps.sleep ?? delay;
  const samples: Sample[] = [];
  const start = now();
  for (;;) {
    const atMs = now() - start;
    if (atMs >= options.durationMs) break;
    const value = await read();
    samples.push({ atMs: now() - start, value });
    await sleep(options.intervalMs);
  }
  return samples;
}

/**
 * 値が変わってから次に変わるまでの最大の間隔(ms)を返す。
 * 計測の開始から最初の変化まで、各変化の間、最後の変化から最後の標本までの区間を比べ、最も長いものを返す。
 * 値が一度も変わらなければ、最初の標本から最後の標本までの長さになる(途中停止・変化なし)。
 */
export function maxGapBetweenChanges(samples: readonly Sample[]): number {
  const [first, ...rest] = samples;
  if (first === undefined) return 0;
  let lastChangeAt = first.atMs;
  let previous = first.value;
  let lastAt = first.atMs;
  let maxGap = 0;
  for (const current of rest) {
    lastAt = current.atMs;
    if (current.value !== previous) {
      maxGap = Math.max(maxGap, current.atMs - lastChangeAt);
      lastChangeAt = current.atMs;
      previous = current.value;
    }
  }
  return Math.max(maxGap, lastAt - lastChangeAt);
}

/**
 * 実測値を注釈(type: "perf")と JSON の添付に残す。合否の判定は呼び出し側が行う。
 */
export async function recordPerf(
  testInfo: TestInfo,
  name: string,
  valueMs: number,
  thresholdMs: number,
): Promise<void> {
  testInfo.annotations.push({
    type: "perf",
    description: `${name}: ${valueMs} ms(閾値 ${thresholdMs} ms)`,
  });
  await testInfo.attach(`${name}.json`, {
    body: JSON.stringify({ name, valueMs, thresholdMs }),
    contentType: "application/json",
  });
}
