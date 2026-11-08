// スモーク 6(Reset): 一時停止・停止の後の Reset で初期状態へ戻ること、ブレークポイントと入力が保持されること、
// 実行中は Reset のボタンと Shift+F5 が効かないこと(要件 5.5)。
import { expect, test } from "../support/fixtures.ts";
import type { DebuggerPage } from "../support/pages/DebuggerPage.ts";

// 「#」を 1 回出力してから無限ループする(上限なしで Run し、Pause で止める)
const PRINT_THEN_LOOP = "+++++[>+++++++<-]>.+[]";
// 2 行目(無限ループの先頭)にブレークポイントを置くと、「#」の出力の後で停止する
const PRINT_THEN_BREAK_LOOP = "+++++[>+++++++<-]>.\n+[]";

/**
 * 現在命令の強調が、指定の行の中にあることを確かめる(強調の中心が行の高さに入るかで判定する)。
 * 強調の文字は行ごとに同じ命令の場合があるため、文字ではなく位置で区別する。
 */
async function expectHighlightOnLine(debuggerPage: DebuggerPage, line: number): Promise<void> {
  await expect(debuggerPage.currentInstruction).toHaveCount(1);
  await expect
    .poll(async () => {
      const row = await debuggerPage.gutterLine(line).boundingBox();
      const mark = await debuggerPage.currentInstruction.boundingBox();
      if (row === null || mark === null) {
        return false;
      }
      const center = mark.y + mark.height / 2;
      return center >= row.y && center <= row.y + row.height;
    })
    .toBe(true);
}

/**
 * Reset の後の初期状態を確かめる: pc と dp が 0、現在命令の強調が先頭の命令(1 行目の先頭)にある、
 * dp のセルが番地 0 で値 0。停止中は強調が 2 行目にあり dp が 1 のため、戻ったことが区別できる。
 */
async function expectInitialPositionAndTape(debuggerPage: DebuggerPage): Promise<void> {
  await expect.poll(() => debuggerPage.statusValue("pc")).toBe(0);
  await expect.poll(() => debuggerPage.statusValue("dp")).toBe(0);
  await expectHighlightOnLine(debuggerPage, 1);
  await expect(debuggerPage.currentInstruction).toHaveText("+");
  const pointer = debuggerPage.region("テープ").locator('[aria-current="true"]');
  await expect(pointer).toHaveCount(1);
  await expect(pointer).toHaveAttribute("aria-label", /^セル 0、10 進 0、/);
}

test.describe("スモーク 6: Reset", () => {
  test("一時停止の後の Shift+F5 で「未開始」に戻り、出力と実行数がクリアされる", async ({ backend, debuggerPage }) => {
    // 2 行目の無限ループで Pause する(停止位置の強調が 2 行目に来るため、Reset で 1 行目へ戻ることを確かめられる)
    await debuggerPage.setSource(PRINT_THEN_BREAK_LOOP);
    await debuggerPage.unlimited.check();
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(0);
    await debuggerPage.button("Pause").click();
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    await expect(debuggerPage.outputText).toHaveText("#");
    // 停止中は pc が 0 でなく、dp が 1(1 番のセル)にあり、強調が 2 行目にある
    await expect.poll(() => debuggerPage.statusValue("dp")).toBe(1);
    expect(await debuggerPage.statusValue("pc")).not.toBe(0);
    await expectHighlightOnLine(debuggerPage, 2);

    await debuggerPage.page.keyboard.press("Shift+F5");

    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    await expect(debuggerPage.outputText).toHaveCount(0);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(0);
    await expectInitialPositionAndTape(debuggerPage);
    await expect.poll(() => backend.calls("reset")[0]?.outcome ?? null).toEqual({ ok: true, value: expect.anything() });
    expect(backend.calls("reset")).toHaveLength(1);
  });

  test("ブレークポイントで停止した後の Reset で「未開始」に戻り、出力がクリアされる", async ({ backend, debuggerPage }) => {
    await debuggerPage.setSource(PRINT_THEN_BREAK_LOOP);
    await debuggerPage.clickBreakpointGutter(2);
    await expect.poll(() => debuggerPage.hasBreakpointAt(2)).toBe(true);
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("ブレークポイントで停止");
    await expect(debuggerPage.outputText).toHaveText("#");
    await expect(debuggerPage.button("Reset")).toBeEnabled();
    // 停止中は pc が 0 でなく、dp が 1 にある(Reset の後と区別する)
    await expect.poll(() => debuggerPage.statusValue("dp")).toBe(1);
    expect(await debuggerPage.statusValue("pc")).not.toBe(0);
    await expectHighlightOnLine(debuggerPage, 2);

    await debuggerPage.page.keyboard.press("Shift+F5");

    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    await expect(debuggerPage.outputText).toHaveCount(0);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(0);
    await expectInitialPositionAndTape(debuggerPage);
    expect(backend.calls("reset")).toHaveLength(1);
  });

  test("Reset の後もテキスト形式の入力が保持される", async ({ debuggerPage }) => {
    await debuggerPage.setSource(PRINT_THEN_BREAK_LOOP);
    await debuggerPage.clickBreakpointGutter(2);
    await expect.poll(() => debuggerPage.hasBreakpointAt(2)).toBe(true);

    await debuggerPage.inputMode("テキスト").check();
    await debuggerPage.inputBox.fill("AB");
    await expect(debuggerPage.inputBox).toHaveValue("AB");

    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("ブレークポイントで停止");

    await debuggerPage.page.keyboard.press("Shift+F5");
    await expect(debuggerPage.statusLabel).toHaveText("未開始");

    await expect(debuggerPage.inputMode("テキスト")).toBeChecked();
    await expect(debuggerPage.inputBox).toHaveValue("AB");
  });

  test("Reset の後もブレークポイントと入力(16 進)が保持され、再び Run すると同じ位置で停止する", async ({
    debuggerPage,
  }) => {
    await debuggerPage.setSource(PRINT_THEN_BREAK_LOOP);
    await debuggerPage.clickBreakpointGutter(2);
    await expect.poll(() => debuggerPage.hasBreakpointAt(2)).toBe(true);
    await expect(debuggerPage.breakpointMarkers).toHaveCount(1);

    await debuggerPage.inputMode("16 進").check();
    await debuggerPage.inputBox.fill("4142");
    await expect(debuggerPage.inputBox).toHaveValue("4142");

    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("ブレークポイントで停止");

    await debuggerPage.page.keyboard.press("Shift+F5");
    await expect(debuggerPage.statusLabel).toHaveText("未開始");

    await expect(debuggerPage.breakpointMarkers).toHaveCount(1);
    await expect.poll(() => debuggerPage.hasBreakpointAt(2)).toBe(true);
    await expect(debuggerPage.inputMode("16 進")).toBeChecked();
    await expect(debuggerPage.inputBox).toHaveValue("4142");

    // 保持されたブレークポイントで、再び同じ位置で停止する
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("ブレークポイントで停止");
    await expect(debuggerPage.outputText).toHaveText("#");
  });

  test("実行中は Reset のボタンが無効で、Shift+F5 を押しても状態と出力は変わらない", async ({ backend, debuggerPage }) => {
    await debuggerPage.setSource(PRINT_THEN_LOOP);
    await debuggerPage.unlimited.check();
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect(debuggerPage.button("Reset")).toBeDisabled();

    const before = await debuggerPage.statusValue("実行数");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(before);

    await debuggerPage.page.keyboard.press("Shift+F5");

    // Shift+F5 が効いていれば「未開始」に戻り実行数が 0 になるため、その後も増え続けることで判定する
    const afterPress = await debuggerPage.statusValue("実行数");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(afterPress);
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect(debuggerPage.outputText).toHaveText("#");
    expect(backend.calls("reset")).toHaveLength(0);

    await debuggerPage.button("Pause").click();
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    await expect(debuggerPage.button("Reset")).toBeEnabled();
  });
});
