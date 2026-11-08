// スモーク 15(実行エラーの表示): 命令数上限を超えた実行エラー(5.6)、括弧の不整合での解析エラーの強調(5.7)、
// それぞれの後にソースを直して Reset・Run すると正常に終わること(5.8)。
// 表示は画面の表示で確かめ、実行の結果は run_batch・load_source の応答(呼び出しの記録)と突き合わせる。
import { expect, test } from "../support/fixtures.ts";

/** run_batch の応答のうち、このシナリオが読む部分(apps/desktop/src/generated/BatchResultDto.ts と一致) */
type BatchResult = {
  stop: { kind: string; error?: { kind: string; limit?: number; pc?: number } };
  executed: number;
  state: { output: { dataBase64: string } };
};

/** 上限に達するプログラム(無限ループ) */
const LOOP_SOURCE = "+[]";

/** 直したあとのプログラム: セル 1 に 65 を入れて「A」を出力する(約 100 命令で終わる) */
const FIXED_SOURCE = "++++++++[>++++++++<-]>+.";

/** 括弧が対応しないプログラム(開き括弧が余る)。余った「[」は 2 文字目(行 1・列 2)、命令の添字は 1 */
const UNMATCHED_OPENING_SOURCE = "+[+";

/** 括弧が対応しないプログラム(閉じ括弧が余る)。余った「]」は 2 文字目(行 1・列 2)、命令の添字は 1 */
const UNMATCHED_CLOSING_SOURCE = "+]";

/** 直したあとのプログラム(空のループで必ず止まる) */
const BALANCED_SOURCE = "+[-]";

test.describe("スモーク 15: 実行エラーの表示", () => {
  test("5.6 命令数上限を超えると実行エラーの見出しと上限の理由が表示され、ソースを直して Reset・Run すると正常終了し、表示が消える", async ({
    backend,
    debuggerPage,
  }) => {
    const { stepLimit } = debuggerPage;
    await debuggerPage.setSource(LOOP_SOURCE);
    await stepLimit.fill("3");
    await debuggerPage.button("Run").click();

    // 上限到達の実行エラー: 状態ラベル、エラーの見出し、上限の理由、実行数が表示される
    await expect(debuggerPage.statusLabel).toHaveText("実行エラー");
    await expect(debuggerPage.errorBanner).toContainText("実行エラー");
    await expect(debuggerPage.errorBanner).toContainText("命令数の上限(3 命令)を超えました");
    await expect(debuggerPage.errorBanner.filter({ hasText: "解析エラー" })).toHaveCount(0);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(3);

    // 実行の応答が上限到達の実行エラーであることを確かめる(画面の表示と一致する)
    const batches = backend.calls("run_batch");
    expect(batches.length).toBeGreaterThan(0);
    const last = batches.at(-1)?.outcome;
    await expect.poll(() => last?.ok ?? null).toBe(true);
    const failed = (last as { ok: true; value: BatchResult }).value;
    expect(failed.stop.kind).toBe("error");
    expect(failed.stop.error).toEqual({ kind: "stepLimitExceeded", limit: 3, pc: expect.any(Number) });
    expect(failed.executed).toBe(3);

    // ソースを直して Reset し、上限を直して Run する(直したソースが読み込まれ、正常に終わる)
    await debuggerPage.button("Reset").click();
    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    // Reset で実行エラーの表示と強調が消える(前の結果を残さない)
    await expect(debuggerPage.errorBanner).toHaveCount(0);
    await expect(debuggerPage.runtimeErrors).toHaveCount(0);
    await expect(debuggerPage.parseErrors).toHaveCount(0);
    await debuggerPage.setSource(FIXED_SOURCE);
    await stepLimit.fill("1000");
    await expect(stepLimit).toHaveAttribute("aria-invalid", "false");
    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    await expect(debuggerPage.outputText).toHaveText("A");
    await expect(debuggerPage.errorBanner).toHaveCount(0);

    // 直したソースが読み込まれたことを、読み込みの要求と応答で確かめる
    const loads = backend.calls("load_source");
    expect(loads.length).toBeGreaterThan(0);
    expect(loads.at(-1)?.payload).toMatchObject({ args: { source: FIXED_SOURCE } });
    await expect.poll(() => loads.at(-1)?.outcome?.ok ?? null).toBe(true);
    const finalBatch = backend.calls("run_batch").at(-1)?.outcome;
    await expect.poll(() => finalBatch?.ok ?? null).toBe(true);
    expect(((finalBatch as { ok: true; value: BatchResult }).value).stop.kind).toBe("halted");
  });

  test("5.7 開き括弧が余る解析エラーは見出し・種類・位置・命令添字が表示され、その括弧だけが解析エラーの強調になる", async ({
    backend,
    debuggerPage,
  }) => {
    await debuggerPage.setSource(UNMATCHED_OPENING_SOURCE);
    await debuggerPage.button("Run").click();

    // 解析エラーは実行を始めない: 状態は未開始のままで、セッションは作られない
    await expect(debuggerPage.errorBanner).toContainText("解析エラー");
    await expect(debuggerPage.errorBanner).toContainText("種類: 対応する閉じ括弧がない開き括弧");
    await expect(debuggerPage.errorBanner).toContainText("命令添字: 1");
    await expect(debuggerPage.errorBanner).toContainText("位置: 行 1・列 2");
    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    expect(backend.calls("start_session")).toEqual([]);

    // 強調は対応しない括弧 1 文字だけ(エラーの位置)
    await expect(debuggerPage.parseErrors).toHaveText("[");
    await expect(debuggerPage.currentInstruction).toHaveCount(0);

    // 応答: 読み込みは解析エラーで返り、プログラムは登録されない
    const loads = backend.calls("load_source");
    expect(loads.length).toBeGreaterThan(0);
    await expect.poll(() => loads.at(-1)?.outcome?.ok ?? null).toBe(true);
    expect((loads.at(-1)?.outcome as { ok: true; value: { kind: string } }).value.kind).toBe("parseError");
  });

  test("5.7 閉じ括弧が余る解析エラーは見出し・種類・位置・命令添字が表示され、その括弧だけが解析エラーの強調になる", async ({
    backend,
    debuggerPage,
  }) => {
    await debuggerPage.setSource(UNMATCHED_CLOSING_SOURCE);
    await debuggerPage.button("Run").click();

    await expect(debuggerPage.errorBanner).toContainText("解析エラー");
    await expect(debuggerPage.errorBanner).toContainText("種類: 対応する開き括弧がない閉じ括弧");
    await expect(debuggerPage.errorBanner).toContainText("命令添字: 1");
    await expect(debuggerPage.errorBanner).toContainText("位置: 行 1・列 2");
    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    expect(backend.calls("start_session")).toEqual([]);

    await expect(debuggerPage.parseErrors).toHaveText("]");
    const loads = backend.calls("load_source");
    await expect.poll(() => loads.at(-1)?.outcome?.ok ?? null).toBe(true);
    expect((loads.at(-1)?.outcome as { ok: true; value: { kind: string } }).value.kind).toBe("parseError");
  });

  test("5.8 解析エラーの後にソースを直すと解析エラーの表示と強調が消え、Run で正常終了する", async ({
    backend,
    debuggerPage,
  }) => {
    await debuggerPage.setSource(UNMATCHED_OPENING_SOURCE);
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.parseErrors).toHaveText("[");
    await expect(debuggerPage.errorBanner).toContainText("解析エラー");

    // 編集で解析エラーの表示は消え、Run で正常に終わる。解析エラーの後はセッションが無く Reset は無効のため、Reset は押さない
    await debuggerPage.setSource(BALANCED_SOURCE);
    await expect(debuggerPage.errorBanner).toHaveCount(0);
    await expect(debuggerPage.parseErrors).toHaveCount(0);
    await expect(debuggerPage.button("Reset")).toBeDisabled();

    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    await expect(debuggerPage.errorBanner).toHaveCount(0);
    await expect(debuggerPage.parseErrors).toHaveCount(0);
    await expect(debuggerPage.errorBanner.filter({ hasText: "解析エラー" })).toHaveCount(0);

    const loads = backend.calls("load_source");
    expect(loads.at(-1)?.payload).toMatchObject({ args: { source: BALANCED_SOURCE } });
    await expect.poll(() => loads.at(-1)?.outcome ?? null).toMatchObject({ ok: true, value: { kind: "ok" } });
  });
});
