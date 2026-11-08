// スモーク 9(バッチ実行の応答性): 上限なしの無限ループの実行中に、画面が応答し続けることを時間の閾値を使わずに確かめる。
// 時間の閾値を伴う計測(Run から Pause の受け付け、実行数の更新間隔の最大値、Pause 後の表示の追いつき)は
// `@perf` のテストとしてこのファイルだけに置く(要件 10.2〜10.4。通常の実行では除外される)。
import { type RecordedCall, expect, test } from "../support/fixtures.ts";
import type { DebuggerPage } from "../support/pages/DebuggerPage.ts";
import { PERF_THRESHOLD_MS, maxGapBetweenChanges, recordPerf, sample } from "../support/perf.ts";

/** 応答を運ぶ呼び出し(実行・Step の SessionStateDto を含む応答)の最後の状態を返す。無ければ null */
function latestState(calls: readonly RecordedCall[]): StateLike | null {
  let latest: { state: StateLike; endedAt: number } | null = null;
  for (const call of calls) {
    if (call.outcome?.ok !== true || call.endedAt === null) continue;
    const value = call.outcome.value as { state?: unknown } | null;
    const state = (value !== null && typeof value === "object" && "state" in value ? value.state : value) as StateLike | null;
    if (state === null || typeof state !== "object" || !("tapeWindow" in state) || !("output" in state)) continue;
    if (latest === null || call.endedAt >= latest.endedAt) {
      latest = { state, endedAt: call.endedAt };
    }
  }
  return latest?.state ?? null;
}

/** 応答の SessionStateDto のうち、表示の確認に使う項目 */
type StateLike = {
  output: { totalLen: number };
  tapeWindow: { start: number; cells: number[]; pointerOffset: number };
};

/** 出力の総バイト数の表示(「総バイト数: N バイト」)。読めなければ null */
async function displayedOutputTotal(debuggerPage: DebuggerPage): Promise<number | null> {
  const text = await debuggerPage.region("出力").getByText(/^総バイト数: \d+ バイト$/).textContent();
  const match = /(\d+) バイト/.exec(text ?? "");
  return match === null ? null : Number(match[1]);
}

/** Run を押して実行中になるまで待つ(ラベルの変化は ページ内で待つ。Playwright の自動待機の間隔に依らない) */
async function runAndWaitRunning(debuggerPage: DebuggerPage): Promise<void> {
  await debuggerPage.button("Run").click();
  await debuggerPage.waitForStatusLabel("実行中");
}

test.describe("スモーク 9: バッチ実行の応答性", () => {
  test("上限なしの無限ループの実行中に、実行数が増え続け、ホバーとテキストの選択に画面が反応する", async ({
    debuggerPage,
  }) => {
    const { page } = debuggerPage;
    await debuggerPage.setSource("+[]");
    await debuggerPage.unlimited.check();
    await runAndWaitRunning(debuggerPage);

    // 実行数が増え続ける(一度の増加ではなく、読み取った値より後にさらに増えることを重ねて確かめる)
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(0);
    const first = await debuggerPage.statusValue("実行数");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(first);
    const second = await debuggerPage.statusValue("実行数");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(second);

    // ボタンへのホバー: 実行中でも、ホバーの状態(:hover)が画面に反映される
    const pause = debuggerPage.button("Pause");
    await pause.hover();
    await expect.poll(() => pause.evaluate((element) => element.matches(":hover"))).toBe(true);

    // テキストの選択: 状態領域の先頭(実行中の表示)から領域の末尾までをドラッグし、選択された文字列に実行中の表示が含まれる
    const label = await debuggerPage.statusLabel.boundingBox();
    const region = await debuggerPage.region("状態").boundingBox();
    if (label === null || region === null) {
      throw new Error("状態領域の位置を取得できません");
    }
    await page.mouse.move(label.x + 1, label.y + label.height / 2);
    await page.mouse.down();
    await page.mouse.move(region.x + region.width - 2, region.y + region.height - 2, { steps: 5 });
    await page.mouse.up();
    const selected = await page.evaluate(() => window.getSelection()?.toString() ?? "");
    expect(selected).toContain("実行中");

    // 後始末: 一時停止し、ラベルが変わることを確かめる
    await pause.click();
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
  });

  test("@perf Pause の受け付けまで(pause 呼び出しの完了まで)が 1,500 ms 以下であり、実測値を記録する", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    await debuggerPage.setSource("+[]");
    await debuggerPage.unlimited.check();
    await runAndWaitRunning(debuggerPage);

    // 受け付けは pause 呼び出しの完了で定義する(表示のラベルの変化では測らない)。計測は Pause のクリックから
    const before = backend.calls("pause").length;
    const clickedAt = Date.now();
    await debuggerPage.button("Pause").click();
    await expect.poll(() => backend.calls("pause").length).toBeGreaterThan(before);
    const acknowledged = backend.calls("pause").at(-1);
    await expect.poll(() => backend.calls("pause").at(-1)?.outcome ?? null).not.toBeNull();
    expect(acknowledged?.outcome?.ok, "pause 呼び出しが成功で返る").toBe(true);
    const elapsedMs = (backend.calls("pause").at(-1)?.endedAt ?? NaN) - clickedAt;

    await debuggerPage.waitForStatusLabel("一時停止");
    await recordPerf(testInfo, "Pause の受け付けまで", Math.round(elapsedMs), PERF_THRESHOLD_MS);
    expect(elapsedMs).toBeLessThanOrEqual(PERF_THRESHOLD_MS);
  });

  test("@perf 実行中の実行数の表示の更新間隔の最大値が 1,500 ms 以下であり、実測値を記録する", async ({ debuggerPage }, testInfo) => {
    await debuggerPage.setSource("+[]");
    await debuggerPage.unlimited.check();
    await runAndWaitRunning(debuggerPage);

    const samples = await sample(() => debuggerPage.statusValue("実行数"), { intervalMs: 50, durationMs: 3000 });
    await debuggerPage.button("Pause").click();
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    const maxGapMs = Math.round(maxGapBetweenChanges(samples));

    await recordPerf(testInfo, "実行数の更新間隔の最大値", maxGapMs, PERF_THRESHOLD_MS);
    expect(maxGapMs).toBeLessThanOrEqual(PERF_THRESHOLD_MS);
  });

  test("@perf Pause の後に出力とテープの表示が最後の応答に追いつくまでが 1,500 ms 以下であり、実測値を記録する", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    // 出力とテープの両方が変わり続けるプログラム(1 周ごとに 1 バイトを出力し、隣のセルを増やす)
    await debuggerPage.setSource("+[.>+<]");
    await debuggerPage.unlimited.check();
    await runAndWaitRunning(debuggerPage);
    await expect.poll(() => displayedOutputTotal(debuggerPage)).toBeGreaterThan(0);

    await debuggerPage.button("Pause").click();
    await debuggerPage.waitForStatusLabel("一時停止");
    const pausedAt = performance.now();

    // 追いついた状態: 未完了の呼び出しが無く、最後の状態応答の出力の総バイト数と、dp のセルの表示が一致する
    const caughtUp = async (): Promise<boolean> => {
      const calls = backend.calls();
      if (calls.some((call) => call.outcome === null)) return false;
      const last = latestState(calls);
      if (last === null) return false;
      if ((await displayedOutputTotal(debuggerPage)) !== last.output.totalLen) return false;
      const pointer = last.tapeWindow.pointerOffset;
      const value = last.tapeWindow.cells[pointer];
      const index = last.tapeWindow.start + pointer;
      const cell = debuggerPage
        .region("テープ")
        .getByRole("listitem", { name: new RegExp(`^セル ${index}、10 進 ${value}、`) });
      return (await cell.count()) === 1 && (await cell.getAttribute("aria-current")) === "true";
    };
    await expect.poll(caughtUp, { intervals: [10], timeout: 10_000 }).toBe(true);
    const catchUpMs = Math.round(performance.now() - pausedAt);

    await recordPerf(testInfo, "Pause 後の表示の追いつき", catchUpMs, PERF_THRESHOLD_MS);
    expect(catchUpMs).toBeLessThanOrEqual(PERF_THRESHOLD_MS);
  });
});
