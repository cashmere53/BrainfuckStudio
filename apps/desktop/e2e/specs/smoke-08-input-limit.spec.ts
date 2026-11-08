// スモーク 8(入力と命令数上限): 入力の形式の切り替えで入力バイト列が変わらないこと(6.4)、
// 不正な 16 進で理由が出て Run が無効になること(6.5)、不正な上限で理由が出て直前の有効な上限で実行されること(6.6)、
// 上限に達するプログラムが上限到達の実行エラーで止まること(5.6)。
// 入力バイト列は start_session の要求(inputBase64)と突き合わせ、上限は要求の maxSteps と突き合わせる。
import { expect, test } from "../support/fixtures.ts";

/** start_session の要求のうち、このシナリオが読む部分(apps/desktop/src/generated/StartSessionRequest.ts と一致) */
type StartSessionArgs = {
  args: { inputBase64: string; limit: { kind: string; maxSteps?: number } };
};

/** run_batch の応答のうち、このシナリオが読む部分(apps/desktop/src/generated/BatchResultDto.ts と一致) */
type BatchResult = {
  stop: { kind: string; error?: { kind: string; limit?: number; pc?: number } };
  executed: number;
  state: { output: { dataBase64: string } };
};

/** 入力を読んでそのまま出力する(`,` で 1 バイト読み、`.` で書く) */
const ECHO_SOURCE = ",.,.,.,.";

/** 上限に達するプログラム(無限ループ) */
const LOOP_SOURCE = "+[]";

/** 「あA」の UTF-8 のバイト列(E3 81 82 41)の Base64 */
const KANA_A_BASE64 = Buffer.from([0xe3, 0x81, 0x82, 0x41]).toString("base64");

/** start_session の要求の一覧(入力のバイト列と上限を読む) */
function startArgs(calls: ReadonlyArray<{ payload: unknown }>): StartSessionArgs[] {
  return calls.map((call) => call.payload as StartSessionArgs);
}

test.describe("スモーク 8: 入力と命令数上限", () => {
  test("6.4 テキストと 16 進の切り替えで表示が対応し、プログラムが読む入力バイト列は切り替えの前後で変わらない", async ({
    backend,
    debuggerPage,
  }) => {
    const { inputBox } = debuggerPage;
    await debuggerPage.setSource(ECHO_SOURCE);
    // 1 文字(3 バイト)と ASCII(1 バイト)を混ぜ、バイト列の違いが表示と実行に出るようにする
    await inputBox.fill("あA");

    await debuggerPage.inputMode("16 進").click();
    await expect(inputBox).toHaveValue("E3 81 82 41");
    await debuggerPage.inputMode("テキスト").click();
    await expect(inputBox).toHaveValue("あA");

    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    await expect(debuggerPage.outputText).toHaveText("あA");
    const starts = startArgs(backend.calls("start_session"));
    expect(starts).toHaveLength(1);
    expect(starts[0]?.args.inputBase64).toBe(KANA_A_BASE64);
  });

  test("6.4 0x10 未満のバイトも 2 桁の大文字 16 進で表示され、プログラムが読む入力バイト列と一致する", async ({
    backend,
    debuggerPage,
  }) => {
    const { inputBox } = debuggerPage;
    await debuggerPage.setSource(ECHO_SOURCE);
    // あ(E3 81 82)・タブ(09)・A(41)。タブは 0x10 未満のため 1 桁にせず 2 桁で表示される
    await inputBox.fill("あ\tA");

    await debuggerPage.inputMode("16 進").click();
    await expect(inputBox).toHaveValue("E3 81 82 09 41");

    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    const starts = startArgs(backend.calls("start_session"));
    expect(starts).toHaveLength(1);
    expect(starts[0]?.args.inputBase64).toBe(Buffer.from([0xe3, 0x81, 0x82, 0x09, 0x41]).toString("base64"));
  });

  test("6.4 16 進で入力した内容をテキストへ切り替えても、プログラムが読む入力バイト列は変わらない", async ({
    backend,
    debuggerPage,
  }) => {
    const { inputBox } = debuggerPage;
    await debuggerPage.setSource(ECHO_SOURCE);

    await debuggerPage.inputMode("16 進").click();
    await inputBox.fill("E3 81 82 41");
    await debuggerPage.inputMode("テキスト").click();
    await expect(inputBox).toHaveValue("あA");
    await debuggerPage.inputMode("16 進").click();
    await expect(inputBox).toHaveValue("E3 81 82 41");

    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    await expect(debuggerPage.outputText).toHaveText("あA");
    const starts = startArgs(backend.calls("start_session"));
    expect(starts).toHaveLength(1);
    expect(starts[0]?.args.inputBase64).toBe(KANA_A_BASE64);
  });

  test("6.5 奇数桁・16 進でない文字の入力で理由が表示され、Run と Step が無効になり、正しい入力で有効に戻る", async ({
    backend,
    debuggerPage,
  }) => {
    const { inputBox, page } = debuggerPage;
    await debuggerPage.setSource(ECHO_SOURCE);
    await debuggerPage.inputMode("16 進").click();

    // 奇数桁: 理由は桁数の奇偶。入力欄は不正として示される
    await inputBox.fill("4");
    await expect(inputBox).toHaveAttribute("aria-invalid", "true");
    await expect(
      page.getByRole("alert").filter({ hasText: "入力エラー: 16 進数字の総数が奇数です" }),
    ).toBeVisible();
    await expect(debuggerPage.button("Run")).toBeDisabled();
    await expect(debuggerPage.button("Step")).toBeDisabled();

    // 16 進でない文字: 理由は文字の種類と位置(2 文字目)。大文字・小文字の g(16 進の範囲外)と、空白以外の非 ASCII 文字
    for (const text of ["4G", "4g", "4あ"]) {
      await inputBox.fill(text);
      await expect(inputBox).toHaveAttribute("aria-invalid", "true");
      await expect(
        page.getByRole("alert").filter({ hasText: "入力エラー: 16 進数字・空白以外の文字が含まれています(2 文字目)" }),
      ).toBeVisible();
      await expect(debuggerPage.button("Run")).toBeDisabled();
      await expect(debuggerPage.button("Step")).toBeDisabled();
    }

    // 不正な入力の間は開始要求を送らない
    expect(backend.calls("start_session")).toEqual([]);

    // 正しい入力に直すと理由が消え、Run と Step が有効になる
    await inputBox.fill("41");
    await expect(inputBox).toHaveAttribute("aria-invalid", "false");
    await expect(page.getByRole("alert").filter({ hasText: "入力エラー" })).toHaveCount(0);
    await expect(debuggerPage.button("Run")).toBeEnabled();
    await expect(debuggerPage.button("Step")).toBeEnabled();
  });

  test("6.6 不正な上限(0・負数・小数・空・数字以外)で理由が表示される", async ({ debuggerPage }) => {
    const { page, stepLimit } = debuggerPage;
    const cases: Array<[string, string]> = [
      ["0", "命令数上限は 1 以上で入力してください"],
      ["-5", "命令数上限は 1 以上で入力してください"],
      ["1.5", "命令数上限は 10 進整数で入力してください"],
      ["", "命令数上限を入力してください"],
      ["abc", "命令数上限は 10 進整数で入力してください"],
    ];

    for (const [text, reason] of cases) {
      await stepLimit.fill(text);
      await expect(stepLimit).toHaveAttribute("aria-invalid", "true");
      await expect(page.getByRole("alert").filter({ hasText: `入力エラー: ${reason}` })).toBeVisible();
    }
  });

  test("6.6 不正な上限のときは直前の有効な上限(5)で実行され、上限到達の実行エラーで止まる", async ({
    backend,
    debuggerPage,
  }) => {
    const { page, stepLimit } = debuggerPage;
    await debuggerPage.setSource(LOOP_SOURCE);
    await stepLimit.fill("5");
    await expect(stepLimit).toHaveAttribute("aria-invalid", "false");

    for (const text of ["0", "-5", "1.5", "", "abc"]) {
      await stepLimit.fill(text);
      await expect(stepLimit).toHaveAttribute("aria-invalid", "true");

      await debuggerPage.button("Run").click();

      await expect(debuggerPage.statusLabel).toHaveText("実行エラー");
      await expect(debuggerPage.errorBanner).toContainText("命令数の上限(5 命令)を超えました");
      // 不正な欄の理由は、実行の後も残る
      await expect(stepLimit).toHaveAttribute("aria-invalid", "true");
      await expect(page.getByRole("alert").filter({ hasText: "入力エラー" }).first()).toBeVisible();

      await debuggerPage.button("Reset").click();
      await expect(debuggerPage.statusLabel).toHaveText("未開始");
    }

    // 開始要求は直前の有効な上限 5 を指定している(既定の 10,000,000 や不正な値は使われない)。
    // Reset はセッションを残すため、以降の実行は同じセッションの上限 5 で行われる
    const starts = startArgs(backend.calls("start_session"));
    expect(starts).toHaveLength(1);
    expect(starts[0]?.args.limit).toEqual({ kind: "limited", maxSteps: 5 });
    const batches = backend.calls("run_batch");
    expect(batches).toHaveLength(5);
    for (const batch of batches) {
      await expect.poll(() => batch.outcome?.ok ?? null).toBe(true);
      const result = (batch.outcome as { ok: true; value: BatchResult }).value;
      expect(result.stop.error).toEqual({ kind: "stepLimitExceeded", limit: 5, pc: expect.any(Number) });
    }
  });

  test("5.6 上限に達するプログラムは上限到達の実行エラーで止まり、実行数は上限と一致する", async ({
    backend,
    debuggerPage,
  }) => {
    await debuggerPage.setSource(LOOP_SOURCE);
    await debuggerPage.stepLimit.fill("3");
    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("実行エラー");
    await expect(debuggerPage.errorBanner).toContainText("実行エラー");
    await expect(debuggerPage.errorBanner).toContainText("命令数の上限(3 命令)を超えました");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(3);

    // 実行の応答: 停止はエラーの「上限到達」で、上限と停止時の pc が返る
    const batches = backend.calls("run_batch");
    expect(batches.length).toBeGreaterThan(0);
    const last = batches.at(-1)?.outcome;
    await expect.poll(() => last?.ok ?? null).toBe(true);
    const result = (last as { ok: true; value: BatchResult }).value;
    expect(result.stop.kind).toBe("error");
    // 停止の理由は上限到達で、上限の値が返る(停止時の pc は実行の位置なので値は見ず、数であることだけを見る)
    expect(result.stop.error).toEqual({ kind: "stepLimitExceeded", limit: 3, pc: expect.any(Number) });
    expect(result.executed).toBe(3);
  });
});
