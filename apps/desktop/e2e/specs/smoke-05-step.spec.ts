// スモーク 5(Step): 1 回ごとの pc の進みと状態の更新、ループの内外での命令の順序、終端の「正常終了」と Step の無効(5.1〜5.3)。
// 長いソースで現在命令が編集領域の表示範囲に入ることを、Step とブレークポイント停止の両方で確かめる(5.4)。
import { expect, test } from "../support/fixtures.ts";
import type { DebuggerPage } from "../support/pages/DebuggerPage.ts";

// 命令は pc 0〜8 の 9 個(コメントは命令ではない)
const LOOP_SOURCE = "+++[>+<-]";
const LOOP_CHARS = LOOP_SOURCE.split("");
const END_PC = LOOP_CHARS.length;

// 1 回ごとの Step の後に表示される pc(次に実行する命令)。
// 最初の +++ の 3 回、[ の 1 回、続けて反復ごとに pc 4〜8 の 5 回を 3 回(] が 0 で抜けるまで)
const EXPECTED_PCS_AFTER_STEP = [1, 2, 3, 4, 5, 6, 7, 8, 4, 5, 6, 7, 8, 4, 5, 6, 7, 8, END_PC];

// 長いソース: 1 行目の命令、80 行のコメント、最後の行の命令(pc 1)。コメントに命令の文字を含めない
const COMMENT_LINES = 80;
const LONG_SOURCE = ["+", ...Array.from({ length: COMMENT_LINES }, (_, index) => `コメント行 ${index + 1}`), "+"].join("\n");
const LAST_LINE = COMMENT_LINES + 2;

/** ループのプログラムを終端まで 1 回ずつ Step で進め、各回の pc・実行数・状態ラベル・現在命令を確かめる */
async function stepLoopToEnd(debuggerPage: DebuggerPage): Promise<void> {
  await debuggerPage.setSource(LOOP_SOURCE);
  await expect(debuggerPage.statusLabel).toHaveText("未開始");

  for (const [index, expectedPc] of EXPECTED_PCS_AFTER_STEP.entries()) {
    await debuggerPage.button("Step").click();

    await expect.poll(() => debuggerPage.statusValue("pc")).toBe(expectedPc);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(index + 1);

    if (expectedPc < END_PC) {
      await expect(debuggerPage.statusLabel).toHaveText("ステップ実行後");
      await expect(debuggerPage.currentInstruction).toHaveCount(1);
      await expect(debuggerPage.currentInstruction).toHaveText(LOOP_CHARS[expectedPc]!);
    } else {
      await expect(debuggerPage.statusLabel).toHaveText("正常終了");
      await expect(debuggerPage.currentInstruction).toHaveCount(0);
    }
  }
}

test.describe("スモーク 5: Step", () => {
  test("ループを含むプログラムを 1 回ずつ進めると、pc・実行数・現在命令が命令の順序どおりに変わり、終端で「正常終了」になる", async ({
    debuggerPage,
  }) => {
    await stepLoopToEnd(debuggerPage);

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    expect(await debuggerPage.statusValue("pc")).toBe(END_PC);
    expect(await debuggerPage.statusValue("実行数")).toBe(EXPECTED_PCS_AFTER_STEP.length);
  });

  // 製品の不具合の記録: 終端で状態ラベルは「正常終了」になるが、Step と Run は有効なまま(要件 4.8 は停止状態での Run と Step の無効を求める)。
  // 原因: apps/desktop/src/state/debuggerStore.ts の applyStep は、最後の 1 命令の応答(executed)では phase を
  // idle のまま返し、停止の確定(halted)は次の Step の応答まで反映されない。応答の result.state.snapshot.halted も
  // lastStop === "halted" も見ていない。ラベルは runLabel.ts が lastStop から導くため「正常終了」になり、
  // controls.ts は phase: idle を見るため Run と Step が有効になる。
  // 追跡先: .kiro/specs/desktop-gui-e2e-browser/traceability.md の「見つけた製品の不具合」。
  test.fixme(
    "終端で Step・Run・Pause が無効、Reset が有効になる",
    {
      annotation: {
        type: "issue",
        description:
          "製品の不具合: 最後の命令を Step で実行した直後、状態ラベルは「正常終了」だが Step と Run が有効なまま。" +
          "原因: apps/desktop/src/state/debuggerStore.ts の applyStep が executed の応答で phase を idle のままにする。" +
          "applyStep は executed の応答で result.state.snapshot.halted や lastStop === \"halted\" を見ていない。" +
          "ラベルは runLabel.ts が lastStop から導くため「正常終了」になり、controls.ts は phase: idle を見るため Run と Step が有効になる。" +
          "上流の要件: .kiro/specs/desktop-gui/requirements.md の 4.7・4.8(Step で最後の命令を終えたら正常終了を表示し、停止状態では Run と Step を無効にする)。" +
          "e2e 側の要件: .kiro/specs/desktop-gui-e2e-browser/requirements.md の 5.3。" +
          "追跡先: .kiro/specs/desktop-gui-e2e-browser/traceability.md の「見つけた製品の不具合」",
      },
    },
    async ({ debuggerPage }) => {
      await stepLoopToEnd(debuggerPage);

      await expect(debuggerPage.button("Step")).toBeDisabled();
      await expect(debuggerPage.button("Run")).toBeDisabled();
      await expect(debuggerPage.button("Pause")).toBeDisabled();
      await expect(debuggerPage.button("Reset")).toBeEnabled();
    },
  );

  test("長いソースで Step すると、次の命令が表示範囲の外にあっても現在命令の強調が表示範囲に入る", async ({
    debuggerPage,
  }) => {
    await debuggerPage.setSource(LONG_SOURCE);
    // 先頭へ移動して表示範囲を先頭に合わせる(最後の行は表示範囲の外にある)
    await debuggerPage.editor.focus();
    await debuggerPage.page.keyboard.press("Control+Home");
    await expect(debuggerPage.gutterLine(1)).toBeInViewport();

    await debuggerPage.button("Step").click();

    await expect.poll(() => debuggerPage.statusValue("pc")).toBe(1);
    await expect(debuggerPage.currentInstruction).toHaveCount(1);
    await expect(debuggerPage.currentInstruction).toBeInViewport();
  });

  test("長いソースでブレークポイントで停止すると、停止位置の現在命令が表示範囲に入る", async ({ debuggerPage }) => {
    await debuggerPage.setSource(LONG_SOURCE);
    // 最後の行にブレークポイントを付ける(ガターの行を描画するため末尾へ移動する)
    await debuggerPage.editor.focus();
    await debuggerPage.page.keyboard.press("Control+End");
    await expect(debuggerPage.gutterLine(LAST_LINE)).toBeVisible();
    await debuggerPage.clickBreakpointGutter(LAST_LINE);
    await expect.poll(() => debuggerPage.hasBreakpointAt(LAST_LINE)).toBe(true);

    // 先頭へ戻して表示範囲を先頭に合わせる。Run は最後の行のブレークポイントで止まる
    await debuggerPage.editor.focus();
    await debuggerPage.page.keyboard.press("Control+Home");
    await expect(debuggerPage.gutterLine(1)).toBeInViewport();

    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("ブレークポイントで停止");
    await expect.poll(() => debuggerPage.statusValue("pc")).toBe(1);
    await expect(debuggerPage.currentInstruction).toHaveCount(1);
    await expect(debuggerPage.currentInstruction).toBeInViewport();
  });
});
