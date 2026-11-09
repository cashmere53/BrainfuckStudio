// スモーク 11(ブラウザ既定動作の抑止)の webview 版。実 WebView2 で、F5 の連打・Ctrl+R・Ctrl+F5 の既定の再読み込みが
// 抑止されてページが再読み込みされず編集内容が残ること(6.1)と、編集可能でない要素の右クリックで WebView2 の既定の
// コンテキストメニューが表示されないこと(6.3)を確かめる。
// 陽性対照(6.2・6.4): アプリの抑止を無効にした同じ操作で、再読み込み(目印が消える)とメニューの表示を検出できることを確かめる。
// キー入力と前面化は OS の窓口(host.sendKeys)で行う。前面化できなければ、そのテストは理由つきで失敗する(6.5)。
// 実行中はウィンドウが前面に出るため、ユーザーがキーボードとマウスを操作しない前提で、直列(1 ワーカー)で実行する。
import { expect, test, type WebviewApp } from "../support/webview/fixtures.ts";
import { bypassAppSuppression, placeNavigationMarker } from "../support/webview/appProbes.ts";
import { installDefaultActionProbe, readProbe } from "../support/probes.ts";
import type { Page } from "@playwright/test";

/** 編集して未保存にする。set_document_dirty の応答の完了を編集の前に確定してから、編集の後に完了を待つ */
async function makeDirty(app: WebviewApp): Promise<void> {
  const pending = await app.ipc.armResponse("set_document_dirty");
  await app.debuggerPage.editor.click();
  await app.page.keyboard.type("+");
  await pending.completed;
  expect(await app.ipc.commands()).toContain("set_document_dirty");
  await expect(app.debuggerPage.documentStatus).toContainText("未保存の変更あり");
}

/** 描画の 2 フレーム分を待つ(時間の閾値を使わない)。抑止されなかった既定動作の開始を待つために使う */
async function waitFrames(page: Page, frames: number): Promise<void> {
  await page.evaluate(
    (count) =>
      new Promise<void>((resolve) => {
        let remaining = count;
        const step = (): void => {
          remaining -= 1;
          if (remaining <= 0) {
            resolve();
          } else {
            requestAnimationFrame(step);
          }
        };
        requestAnimationFrame(step);
      }),
    frames,
  );
}

test.describe("スモーク 11: ブラウザ既定動作の抑止(webview)", () => {
  test("スモーク 11.6 F5 の連打・Ctrl+R・Ctrl+F5 で既定動作が抑止され、再読み込みされず編集内容が残る", async ({ app }) => {
    const { page, debuggerPage, host } = app;
    await makeDirty(app);
    const edited = await debuggerPage.sourceText();
    expect(edited).toContain("+");

    await installDefaultActionProbe(page);
    const marker = await placeNavigationMarker(page);
    const url = page.url();

    // F5 を 10 回送る。各 keydown が既定動作を抑止したこと(defaultPrevented)を記録で確かめてから次へ進む
    await host.sendKeys("{F5 10}");
    await expect
      .poll(async () => (await readProbe(page)).filter((record) => record.key === "F5"), { timeout: 15_000 })
      .toHaveLength(10);
    expect((await readProbe(page)).filter((record) => record.key === "F5").map((record) => record.defaultPrevented)).toEqual(
      Array(10).fill(true),
    );

    // Ctrl+R
    await host.sendKeys("^r");
    await expect
      .poll(async () => (await readProbe(page)).filter((record) => record.key?.toLowerCase() === "r"), { timeout: 15_000 })
      .toHaveLength(1);
    expect((await readProbe(page)).filter((record) => record.key?.toLowerCase() === "r")[0]?.defaultPrevented).toBe(true);

    // Ctrl+F5(F5 の記録が 1 件増え、その既定動作が抑止されている)
    await host.sendKeys("^{F5}");
    await expect
      .poll(async () => (await readProbe(page)).filter((record) => record.key === "F5"), { timeout: 15_000 })
      .toHaveLength(11);
    expect((await readProbe(page)).filter((record) => record.key === "F5").at(-1)?.defaultPrevented).toBe(true);

    // 抑止されなかった既定の再読み込みが始まる余地を、描画の数フレーム分待ってから判定する
    await waitFrames(page, 10);
    expect(marker.navigations, "ページの遷移の回数").toBe(0);
    expect(await marker.isIntact(), "ページ内の目印が残っている").toBe(true);
    expect(page.url()).toBe(url);
    expect(await debuggerPage.sourceText()).toBe(edited);
  });

  test("スモーク 11.6 陽性対照: アプリの抑止を無効にすると、Ctrl+R で再読み込みされて目印が消える", async ({ app }) => {
    const { page, debuggerPage, host } = app;
    await makeDirty(app);

    // 抑止を無効にする。Ctrl+R の keydown がアプリの抑止に届かなくなり、既定の再読み込みが起きるはず
    await bypassAppSuppression(page, ["keydown"]);
    const marker = await placeNavigationMarker(page);

    await host.sendKeys("^r");

    // 再読み込みされたことを検出する(目印が消え、遷移が 1 回以上)。検出できなければ 11.6 の検証は成り立たない
    await expect.poll(() => marker.isIntact(), { timeout: 20_000 }).toBe(false);
    expect(marker.navigations, "ページの遷移の回数").toBeGreaterThanOrEqual(1);
    await expect(debuggerPage.editor).toBeVisible();
  });

  test("スモーク 11.7 編集可能でない要素の右クリックで、WebView2 の既定のコンテキストメニューが表示されない", async ({ app }) => {
    const { page, debuggerPage, host } = app;
    await installDefaultActionProbe(page);

    // 状態の領域は編集可能でない要素。CDP のマウスで右クリックし、アプリの抑止(defaultPrevented)を記録で確かめる
    await debuggerPage.region("状態").click({ button: "right" });
    await expect
      .poll(async () => (await readProbe(page)).filter((record) => record.type === "contextmenu"), { timeout: 15_000 })
      .toHaveLength(1);
    const [menu] = (await readProbe(page)).filter((record) => record.type === "contextmenu");
    expect(menu).toEqual({ type: "contextmenu", key: null, repeat: false, defaultPrevented: true, targetEditable: false });

    // OS の窓口で、既定のコンテキストメニューが表示されていないことを確かめる。メニューの表示は検出時間(陽性対照で約 1.2 秒)より
    // 後に起きうるため、確認を 3 回行い、どの回でも開いていないことを求める(1 回の確認は競合で偽の合格になりうる)
    for (let check = 1; check <= 3; check += 1) {
      await waitFrames(page, 2);
      expect(await host.contextMenuOpen(), `WebView2 の既定のコンテキストメニューの表示(${check} 回目の確認)`).toBe(false);
    }
  });

  test("スモーク 11.7 陽性対照: アプリの抑止を無効にすると、同じ右クリックで既定のコンテキストメニューが検出される", async ({ app }, testInfo) => {
    const { page, debuggerPage, host } = app;

    // 抑止を無効にする。右クリックの既定のメニューが表示されるはず
    await bypassAppSuppression(page, ["contextmenu"]);
    await debuggerPage.region("状態").click({ button: "right" });

    try {
      // 検出できなければ 11.7 の否定の判定は成り立たない
      await expect.poll(() => host.contextMenuOpen(), { timeout: 15_000 }).toBe(true);
    } finally {
      // 後始末: 開いたメニューを Esc で閉じる(検出に失敗しても閉じる。失敗した場合は注釈に残す)
      await host.sendKeys("{ESC}").catch((error: unknown) => {
        testInfo.annotations.push({ type: "warning", description: `後始末の Esc を送れませんでした: ${String(error)}` });
      });
    }
  });
});
