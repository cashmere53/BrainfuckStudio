// スモーク 13.3(エディタのフォーカス枠)の webview 版。実 WebView2 の描画で、キーボードの操作でエディタにフォーカスが移ったとき
// エディタの枠が実際に描かれ、他の要素に隠れていないことを確かめる(8.1)。
// 枠の検証は上流の expectFocusRingDrawn(フォーカス時とブラー時の帯の画像を全辺で比べる)を使う。
// 配色は実環境の設定のまま検証し、配色のエミュレーション(colorScheme の上書き)は使わない。
// 検証したときの配色(ライト・ダーク)は matchMedia の結果を注釈として残す(8.2)。
// 陰性対照: 枠を描画の上から覆うと、同じ検証が失敗することを確かめる(枠が隠れていれば検出できることの確認)。
import { expect, test } from "../support/webview/fixtures.ts";
import { expectFocusRingDrawn } from "../support/visual/focusRing.ts";

/** エディタの直前の要素(画面の操作の並びで、ソースの直前のツールバーのボタン) */
const PREVIOUS_ELEMENT = "全ブレークポイント解除";

test.describe("スモーク 13.3: エディタのフォーカス枠", () => {
  test("スモーク 13.3 Tab の移動でエディタにフォーカスが移り、実 WebView2 の描画で枠が描かれ隠れていない", async (
    { app },
    testInfo,
  ) => {
    const { page, debuggerPage } = app;
    const previous = debuggerPage.button(PREVIOUS_ELEMENT);

    // 配色は実環境の設定のまま。検証したときの配色を注釈に残す(配色の切り替えの確認とは扱わない)
    const dark = await page.evaluate(() => window.matchMedia("(prefers-color-scheme: dark)").matches);
    testInfo.annotations.push({ type: "color-scheme", description: dark ? "dark" : "light" });

    await expectFocusRingDrawn(page, {
      focus: async () => {
        await previous.focus();
        await expect(previous, "Tab の始点はエディタの直前の要素").toBeFocused();
        await page.keyboard.press("Tab");
        await expect(debuggerPage.editor, "Tab で「ソース」(エディタ)にフォーカスが移る").toBeFocused();
      },
      ring: debuggerPage.editorFrame,
    });
  });

  test("スモーク 13.3 陰性対照: 枠の上を覆うと、枠の検証は失敗する(隠れた枠を検出する)", async ({ app }) => {
    const { page, debuggerPage } = app;
    const previous = debuggerPage.button(PREVIOUS_ELEMENT);

    await expect(
      expectFocusRingDrawn(page, {
        focus: async () => {
          await previous.focus();
          await page.keyboard.press("Tab");
          await expect(debuggerPage.editor).toBeFocused();
          // エディタの枠の外側まで、背景の色で覆う(枠の画素が変わらないようにする)
          const box = await debuggerPage.editorFrame.boundingBox();
          if (box === null) {
            throw new Error("エディタの位置を取得できません");
          }
          await page.evaluate((rect) => {
            const cover = document.createElement("div");
            cover.setAttribute("data-e2e-negative-control", "covered-focus-ring");
            cover.style.cssText = `position:fixed;z-index:2147483647;pointer-events:none;background:${getComputedStyle(document.body).backgroundColor};left:${rect.x - 24}px;top:${rect.y - 24}px;width:${rect.width + 48}px;height:${rect.height + 48}px;`;
            document.body.appendChild(cover);
          }, box);
        },
        ring: debuggerPage.editorFrame,
      }),
    ).rejects.toThrow(/フォーカス枠が描画されていない辺があります/);
  });
});
