// スモーク 4(上限なしの実行の Pause と再開)の webview 版。上限なしで `+[]` を Run し、警告・実行中のボタンの有効・無効、
// 実行数が連続 3 回の読み取りで増え続けること(9.3)、Pause で「一時停止」になり実行数が止まること、Run で再び増えることを確かめる。
// 時間の閾値は使わない。停止の確認は、連続する 2 回の読み取りが同じ値になるまで自動リトライする方法で行う。
// IPC は fixtures の ipc(postMessage の方式)で観測する。pause と run_batch の要求・完了は armResponse を操作の前に確定してから待つ。
import { expect, test } from "../support/webview/fixtures.ts";
import type { DebuggerPage } from "../support/pages/DebuggerPage.ts";

/**
 * 実行中の実行数が増え続けることを確かめる。読み取りを繰り返し、前回より大きい値を続けて 3 回得られたら成立する。
 * 実行数の表示は batch ごとに更新されるため、読み取りの間に値が変わらないことがある。その場合は次の読み取りを待つ(時間の閾値は使わない)。
 */
async function expectExecutionCountKeepsIncreasing(debuggerPage: DebuggerPage, readings = 3): Promise<void> {
  const samples: number[] = [];
  await expect
    .poll(
      async () => {
        const value = await debuggerPage.statusValue("実行数");
        const last = samples.at(-1);
        if (last === undefined || value > last) {
          samples.push(value);
        }
        return samples.length >= readings;
      },
      { timeout: 15_000 },
    )
    .toBe(true);
}

/**
 * 実行数が停止したことを確かめ、停止した値を返す。
 * 実行中は実行数の表示が描画のフレームごとに 1〜3 回ずつ変わるため、2 回の読み取りでは偶然同じ値になりうる
 * (実測で実行中でも約 4 回に 1 回)。そこで、描画の 2 フレームごとの読み取りが STABLE_READINGS 回続けて同じ値になるまで
 * 自動リトライし、その値を停止時点とする。停止の直後は Pause の前に送られた run_batch の応答が表示に反映されることがあるため、
 * 途中で値が変わればやり直す。時間の閾値は使わず、フレームの待ちと読み取りの回数で確かめる。
 */
const STABLE_READINGS = 8;

async function waitForStoppedStatus(debuggerPage: DebuggerPage): Promise<number> {
  let stopped = -1;
  await expect
    .poll(async () => {
      const first = await debuggerPage.statusValue("実行数");
      for (let reading = 1; reading < STABLE_READINGS; reading += 1) {
        await debuggerPage.page.evaluate(
          () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))),
        );
        if ((await debuggerPage.statusValue("実行数")) !== first) {
          return false;
        }
      }
      stopped = first;
      return true;
    })
    .toBe(true);
  return stopped;
}

test.describe("スモーク 4: 上限なしの実行の Pause と再開", () => {
  test("上限なしの警告、実行中のボタンの有効・無効、実行数の増加、Pause での停止、Run での再開", async ({
    app,
  }) => {
    const { debuggerPage, ipc } = app;

    // 上限なしにする前は警告が出ていない(不在の確認)
    await debuggerPage.setSource("+[]");
    await expect(debuggerPage.stepLimitWarning).toHaveCount(0);

    // 上限なしにすると警告が出て、命令数上限の入力欄は無効になる
    await debuggerPage.unlimited.check();
    await expect(debuggerPage.stepLimitWarning).toBeVisible();
    await expect(debuggerPage.stepLimit).toBeDisabled();

    // Run: run_batch の要求の完了を確定してから押す。実行中になり、実行中のボタンと編集が止まる
    const firstBatch = await ipc.armResponse("run_batch");
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect(debuggerPage.stepLimitWarning).toBeVisible();
    await expect(debuggerPage.button("Run")).toBeDisabled();
    await expect(debuggerPage.button("Pause")).toBeEnabled();
    await expect(debuggerPage.button("Step")).toBeDisabled();
    await expect(debuggerPage.button("Reset")).toBeDisabled();
    await expect(debuggerPage.unlimited).toBeDisabled();
    await firstBatch.completed;
    expect(await ipc.commands()).toContain("run_batch");

    // 実行数が連続 3 回の読み取りで増え続ける(時間の閾値は使わず、読み取りの値の増加で確かめる)
    await expectExecutionCountKeepsIncreasing(debuggerPage);

    // Pause: pause の要求の完了を確定してから押す。一時停止になり、停止時のボタンの状態になる
    const pauseDone = await ipc.armResponse("pause");
    await debuggerPage.button("Pause").click();
    await pauseDone.completed;
    expect(await ipc.commands()).toContain("pause");
    const runBatchAtPause = (await ipc.commands()).filter((command) => command === "run_batch").length;
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    await expect(debuggerPage.button("Pause")).toBeDisabled();
    await expect(debuggerPage.button("Run")).toBeEnabled();
    await expect(debuggerPage.button("Step")).toBeEnabled();
    await expect(debuggerPage.button("Reset")).toBeEnabled();
    await expect(debuggerPage.stepLimitWarning).toBeVisible();

    // 停止の確認: 連続する 2 回の読み取りが同じ値で、その値が停止時点として固定される
    const stoppedAt = await waitForStoppedStatus(debuggerPage);
    expect(stoppedAt).toBeGreaterThan(0);
    expect(await debuggerPage.statusValue("実行数")).toBe(stoppedAt);
    expect(await debuggerPage.statusValue("実行数")).toBe(stoppedAt);
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    // 停止の後は run_batch の要求が増えていない(実行が続いていないことの IPC による確認)
    expect((await ipc.commands()).filter((command) => command === "run_batch").length).toBe(runBatchAtPause);

    // Run での再開: 実行中に戻り、run_batch の要求が増え、実行数が停止時点より増える
    const runBatchBeforeResume = (await ipc.commands()).filter((command) => command === "run_batch").length;
    const resumedBatch = await ipc.armResponse("run_batch");
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect(debuggerPage.button("Run")).toBeDisabled();
    await expect(debuggerPage.button("Pause")).toBeEnabled();
    await expect(debuggerPage.button("Step")).toBeDisabled();
    await expect(debuggerPage.button("Reset")).toBeDisabled();
    await resumedBatch.completed;
    await expect
      .poll(async () => (await ipc.commands()).filter((command) => command === "run_batch").length)
      .toBeGreaterThan(runBatchBeforeResume);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(stoppedAt);

    // 後始末として再び Pause し、一時停止で止まることを確かめる
    const secondPause = await ipc.armResponse("pause");
    await debuggerPage.button("Pause").click();
    await secondPause.completed;
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    await expect(debuggerPage.button("Run")).toBeEnabled();
    await expect(debuggerPage.stepLimitWarning).toBeVisible();
  });
});
