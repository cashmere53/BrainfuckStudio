// スモーク 4(上限なしの実行の Pause と再開): 上限なしの警告、実行中のボタンの有効・無効、
// Pause 後の実行数の停止、Run での再開を、時間の閾値を使わず実行数の値と呼び出しの記録の変化で確かめる(時間計測は smoke-09 の @perf)。
import { expect, test } from "../support/fixtures.ts";

/** run_batch の応答のうち、このシナリオが読む部分 */
type BatchReply = {
  executed: number;
  stop: { kind: string };
  state: { snapshot: { stepsExecuted: number } };
};

/** 呼び出しの応答が run_batch の成功応答なら、その応答を返す(未応答や失敗なら null) */
function batchReplyOf(outcome: { ok: boolean; value?: unknown } | null): BatchReply | null {
  return outcome?.ok ? (outcome.value as BatchReply) : null;
}

test.describe("スモーク 4: 上限なしの実行の Pause と再開", () => {
  test("上限なしの警告、実行中のボタンの有効・無効、Pause 後の実行数の停止、Run での再開", async ({
    backend,
    debuggerPage,
  }) => {
    // 上限なしにする前は警告が出ていない(不在の確認)
    await debuggerPage.setSource("+[]");
    await expect(debuggerPage.stepLimitWarning).toHaveCount(0);

    // 上限なしにして無限ループを入れる。警告が出て、入力欄は無効になる
    await debuggerPage.unlimited.check();
    await expect(debuggerPage.stepLimitWarning).toBeVisible();
    await expect(debuggerPage.stepLimit).toBeDisabled();

    // Run: 実行中になり、実行数が増え、実行中のボタンと編集が止まる
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect(debuggerPage.stepLimitWarning).toBeVisible();
    await expect(debuggerPage.button("Run")).toBeDisabled();
    await expect(debuggerPage.button("Pause")).toBeEnabled();
    await expect(debuggerPage.button("Step")).toBeDisabled();
    await expect(debuggerPage.button("Reset")).toBeDisabled();
    await expect(debuggerPage.unlimited).toBeDisabled();
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(0);

    // Pause: pause の呼び出しが成功し、一時停止になる
    await debuggerPage.button("Pause").click();
    await expect.poll(() => backend.calls("pause")[0]?.outcome ?? null).toEqual({ ok: true, value: null });
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    await expect(debuggerPage.button("Pause")).toBeDisabled();
    await expect(debuggerPage.button("Run")).toBeEnabled();
    await expect(debuggerPage.button("Step")).toBeEnabled();
    await expect(debuggerPage.button("Reset")).toBeEnabled();

    // 実行中の run_batch がすべて応答を持つまで待つ。表示の実行数は run_batch の応答の executed の合計、
    // および最後の応答の stepsExecuted と一致する。最後の応答の停止理由は、Pause が batch の途中に届いたら
    // cancelled、batch の間に届いたら batchReached になる(どちらも一時停止の結果として正しい)
    await expect.poll(() => backend.calls("run_batch").every((call) => call.outcome !== null)).toBe(true);
    const replies = backend.calls("run_batch").map((call) => batchReplyOf(call.outcome));
    expect(replies.every((reply) => reply !== null)).toBe(true);
    const executedTotal = replies.reduce((sum, reply) => sum + (reply?.executed ?? 0), 0);
    const lastReply = replies.at(-1);
    expect(executedTotal).toBeGreaterThan(0);
    expect(["cancelled", "batchReached"]).toContain(lastReply?.stop.kind);
    expect(lastReply?.state.snapshot.stepsExecuted).toBe(executedTotal);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(executedTotal);

    // 停止の確認: 画面の読み取りの前後で run_batch の呼び出しが増えず、読み取った値も変わらない(時間の閾値は使わない)
    const callsBeforeReads = backend.calls("run_batch").length;
    const firstRead = await debuggerPage.statusValue("実行数");
    const secondRead = await debuggerPage.statusValue("実行数");
    expect(backend.calls("run_batch").length).toBe(callsBeforeReads);
    expect(firstRead).toBe(executedTotal);
    expect(secondRead).toBe(executedTotal);
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");

    // Run での再開: 実行中に戻り、run_batch の呼び出しが増え、実行数が停止時点より増える
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect(debuggerPage.button("Run")).toBeDisabled();
    await expect.poll(() => backend.calls("run_batch").length).toBeGreaterThan(callsBeforeReads);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(executedTotal);

    // 後始末として再び Pause し、一時停止で止まることを確かめる
    await debuggerPage.button("Pause").click();
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    await expect(debuggerPage.button("Run")).toBeEnabled();
    await expect(debuggerPage.stepLimitWarning).toBeVisible();
  });
});
