// スモーク 11(閉じる確認)の webview 版。実ウィンドウへ OS の閉じる要求(SC_CLOSE。× ボタンと同じ経路)を送り、
// 未保存の変更がある間の確認ダイアログを検証する(11.1・11.2・11.3・11.5)。
// 閉じる要求の受信は、ページ内の close-requested の計数で数える。未保存の旗の伝達(set_document_dirty)は、
// 編集の前に待ち受けを確定し、応答(runCallback)を観測してから閉じる要求を送る(5.6)。
// 各テストは新しいプロセスで行う(フィクスチャ app)。終了のテストの後始末は、フィクスチャがプロセスの終了を 1 回だけ行う。
import { expect, test, type WebviewApp } from "../support/webview/fixtures.ts";
import { installCloseEventCounter } from "../support/webview/appProbes.ts";

/** 編集して未保存にする。set_document_dirty の応答の完了を編集の前に確定してから、編集の後に完了を待つ */
async function makeDirty(app: WebviewApp): Promise<void> {
  const pending = await app.ipc.armResponse("set_document_dirty");
  await app.debuggerPage.editor.click();
  await app.page.keyboard.type("+");
  await pending.completed;
  expect(await app.ipc.commands()).toContain("set_document_dirty");
  await expect(app.debuggerPage.documentStatus).toContainText(
    "未保存の変更あり",
  );
}

test.describe("スモーク 11: 閉じる確認(webview)", () => {
  test("スモーク 11.1 未保存の変更で OS の閉じる要求を送ると、プロセスは終了せず、閉じる要求の受信が 1 回で確認ダイアログが 1 つ表示される", async ({
    app,
  }) => {
    await makeDirty(app);
    const closeCounter = await installCloseEventCounter(app.page);

    await app.host.requestClose();

    await expect.poll(() => closeCounter.count()).toBe(1);
    await expect(app.debuggerPage.confirmDialog).toHaveCount(1);
    await expect(app.debuggerPage.confirmDialog).toBeVisible();
    await expect(app.debuggerPage.confirmDialog).toContainText("未保存の変更");
    await expect(
      app.debuggerPage.confirmDialog.getByRole("button", {
        name: "破棄して閉じる",
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      app.debuggerPage.confirmDialog.getByRole("button", {
        name: "取消",
        exact: true,
      }),
    ).toBeVisible();
    // 確認ダイアログが見えている時点で、プロセスは閉じる要求を止めて動き続けている
    expect(app.process.exit).toBeNull();
    expect(await closeCounter.count()).toBe(1);
  });

  test("スモーク 11.2 確認ダイアログで「取消」を選ぶと、プロセスは動き続け、編集内容が残り、ウィンドウのタイトルが読め、続けて操作できる", async ({
    app,
  }) => {
    await makeDirty(app);
    const edited = await app.debuggerPage.sourceText();
    expect(edited).toContain("+");

    await app.host.requestClose();
    await expect(app.debuggerPage.confirmDialog).toBeVisible();
    await app.debuggerPage.confirmDialog
      .getByRole("button", { name: "取消", exact: true })
      .click();

    await expect(app.debuggerPage.confirmDialog).toHaveCount(0);
    expect(app.process.exit).toBeNull();
    expect(await app.debuggerPage.sourceText()).toBe(edited);
    await expect(app.debuggerPage.documentStatus).toContainText(
      "未保存の変更あり",
    );
    // ウィンドウのタイトルを OS の窓口で読める(ウィンドウが残っている)
    await expect
      .poll(() => app.host.windowTitle())
      .toContain("Brainfuck Studio");

    // 続けて操作できる: Run で実行が正常終了まで進む(編集内容は残ったまま)
    await app.debuggerPage.button("Run").click();
    await expect(app.debuggerPage.statusLabel).toHaveText("正常終了");
    expect(app.process.exit).toBeNull();
    expect(await app.debuggerPage.sourceText()).toBe(edited);
  });

  test("スモーク 11.3 確認ダイアログで「破棄して閉じる」を選ぶと、プロセスは終了コード 0 で終わる", async ({
    app,
  }) => {
    await makeDirty(app);

    await app.host.requestClose();
    await expect(app.debuggerPage.confirmDialog).toBeVisible();
    await app.debuggerPage.confirmDialog
      .getByRole("button", { name: "破棄して閉じる", exact: true })
      .click();

    await expect
      .poll(() => app.process.exit, { timeout: 30_000 })
      .not.toBeNull();
    expect(app.process.exit).toEqual({ code: 0, signal: null });
  });

  test("スモーク 11.5 確認ダイアログの表示中に閉じる要求を重ねても、閉じる要求の受信は 2 回になり、確認ダイアログは 1 つのまま", async ({
    app,
  }) => {
    await makeDirty(app);
    const closeCounter = await installCloseEventCounter(app.page);

    await app.host.requestClose();
    await expect.poll(() => closeCounter.count()).toBe(1);
    await expect(app.debuggerPage.confirmDialog).toHaveCount(1);

    // 重ねた要求がページに届いたことを確かめてから、確認ダイアログの数を判定する
    await app.host.requestClose();
    await expect.poll(() => closeCounter.count()).toBe(2);
    await expect(app.debuggerPage.confirmDialog).toHaveCount(1);
    await expect(app.debuggerPage.confirmDialog).toBeVisible();
    expect(app.process.exit).toBeNull();

    // 後始末: 「取消」で閉じる。ダイアログは 0 になり、プロセスは動き続ける
    await app.debuggerPage.confirmDialog
      .getByRole("button", { name: "取消", exact: true })
      .click();
    await expect(app.debuggerPage.confirmDialog).toHaveCount(0);
    expect(app.process.exit).toBeNull();
  });
});
