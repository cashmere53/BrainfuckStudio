// @vitest-environment node
import { describe, expect, it, vi } from "vitest";
import { PERF_THRESHOLD_MS, maxGapBetweenChanges, recordPerf, sample } from "./perf.ts";
import type { Sample } from "./perf.ts";

function series(points: Array<[number, number]>): Sample[] {
  return points.map(([atMs, value]) => ({ atMs, value }));
}

describe("PERF_THRESHOLD_MS(時間計測の閾値)", () => {
  it("要件の 1 秒の 1.5 倍(1,500 ms)である", () => {
    expect(PERF_THRESHOLD_MS).toBe(1500);
  });
});

describe("maxGapBetweenChanges(値の変化の最大間隔)", () => {
  it("標本が無いときは 0 を返す", () => {
    expect(maxGapBetweenChanges([])).toBe(0);
  });

  it("標本が 1 つだけのときは 0 を返す", () => {
    expect(maxGapBetweenChanges(series([[0, 5]]))).toBe(0);
  });

  it("値が一度も変わらないときは、計測の全体の長さを返す(変化なしの停止)", () => {
    const samples = series([
      [0, 7],
      [100, 7],
      [200, 7],
      [300, 7],
    ]);
    expect(maxGapBetweenChanges(samples)).toBe(300);
  });

  it("毎回増える(単調増加)ときは、変化の間隔の最大値を返す", () => {
    const samples = series([
      [0, 0],
      [100, 1],
      [200, 2],
      [300, 3],
    ]);
    expect(maxGapBetweenChanges(samples)).toBe(100);
  });

  it("途中で止まったときは、止まった区間の長さを返す", () => {
    const samples = series([
      [0, 0],
      [100, 1],
      [200, 2],
      [700, 2],
      [800, 3],
    ]);
    // 値 2 は 200 ms から 800 ms まで 600 ms 変わらない
    expect(maxGapBetweenChanges(samples)).toBe(600);
  });

  it("最後まで止まったままのときは、最後の変化からの経過を返す(末尾の停止)", () => {
    const samples = series([
      [0, 0],
      [100, 1],
      [400, 1],
      [900, 1],
    ]);
    expect(maxGapBetweenChanges(samples)).toBe(800);
  });

  it("変化の間隔が不揃いでも、最も長い間隔を返す", () => {
    const samples = series([
      [0, 0],
      [50, 1],
      [250, 2],
      [300, 3],
    ]);
    expect(maxGapBetweenChanges(samples)).toBe(200);
  });
});

describe("maxGapBetweenChanges(追加の境界)", () => {
  it("先頭の変化なしの区間も数える(先頭停止)", () => {
    expect(maxGapBetweenChanges(series([[0, 0], [600, 1], [700, 2]]))).toBe(600);
  });

  it("起点は最初の標本の時刻であり 0 ではない", () => {
    expect(maxGapBetweenChanges(series([[50, 0], [150, 1]]))).toBe(100);
  });

  it("値の減少も変化とみなす", () => {
    // 0→5 の後、100 で 3 に減り、400 で 9 に増える。変化は 100 と 400 で、最大は 400 - 100 = 300
    expect(maxGapBetweenChanges(series([[0, 5], [100, 3], [300, 3], [400, 9]]))).toBe(300);
  });
});

/** 時刻を手で進められるフェイク時計。sleep は時刻を進めるだけで、待ちが多すぎれば例外にして無限ループを防ぐ */
function fakeClock(startAt = 0) {
  let t = startAt;
  let sleeps = 0;
  return {
    now: () => t,
    advance: (ms: number) => {
      t += ms;
    },
    sleep: vi.fn(async (ms: number) => {
      sleeps += 1;
      if (sleeps > 50) throw new Error("sleep が多すぎる(durationMs が効いていない可能性)");
      t += ms;
    }),
  };
}

describe("sample(値の標本化)", () => {
  it("指定した間隔で read を呼び、時刻つきの標本を返す", async () => {
    let n = 0;
    const read = vi.fn(async () => ++n);
    const samples = await sample(read, { intervalMs: 5, durationMs: 40 });
    expect(samples.length).toBeGreaterThanOrEqual(2);
    expect(read).toHaveBeenCalledTimes(samples.length);
    expect(samples.map((s) => s.value)).toEqual(samples.map((_, i) => i + 1));
    samples.slice(1).forEach((s, i) => {
      expect(s.atMs).toBeGreaterThanOrEqual(samples[i]?.atMs ?? Infinity);
    });
  });

  it("フェイク時計で intervalMs と durationMs どおりに標本化する", async () => {
    const clock = fakeClock(1000);
    const read = vi.fn(async () => 0);
    const samples = await sample(read, { intervalMs: 100, durationMs: 350 }, { now: clock.now, sleep: clock.sleep });
    expect(samples.map((s) => s.atMs)).toEqual([0, 100, 200, 300]);
    expect(clock.sleep).toHaveBeenCalledWith(100);
    expect(clock.sleep.mock.calls.every((call) => call[0] === 100)).toBe(true);
  });

  it("標本の時刻は読み取りの後の時刻である(読み取りの所要時間を含む)", async () => {
    const clock = fakeClock(0);
    const read = vi.fn(async () => {
      clock.advance(30);
      return 0;
    });
    const samples = await sample(read, { intervalMs: 100, durationMs: 350 }, { now: clock.now, sleep: clock.sleep });
    expect(samples.map((s) => s.atMs)).toEqual([30, 160, 290]);
  });
});

describe("recordPerf(計測値の注釈と添付)", () => {
  it("計測名・実測値・閾値を注釈と JSON の添付に残す", async () => {
    const annotations: Array<{ type: string; description?: string }> = [];
    const attach = vi.fn(async () => undefined);
    const testInfo = { annotations, attach } as unknown as Parameters<typeof recordPerf>[0];

    await recordPerf(testInfo, "Pause の反映", 812, PERF_THRESHOLD_MS);

    expect(annotations).toHaveLength(1);
    const [annotation] = annotations;
    expect(annotation).toMatchObject({ type: "perf" });
    const description = annotation?.description ?? "";
    expect(description).toContain("Pause の反映");
    expect(description).toContain("812");
    expect(description).toContain("1500");

    expect(attach).toHaveBeenCalledTimes(1);
    const [name, options] = attach.mock.calls[0] as unknown as [string, { body: string; contentType: string }];
    expect(name).toContain("Pause の反映");
    expect(options.contentType).toBe("application/json");
    expect(JSON.parse(options.body)).toEqual({
      name: "Pause の反映",
      valueMs: 812,
      thresholdMs: 1500,
    });
  });
});
