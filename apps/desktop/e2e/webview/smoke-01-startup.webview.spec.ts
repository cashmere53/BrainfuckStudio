// スモーク 1(起動)の webview 版。実アプリを起動して CDP で接続し、画面の初期表示を OS の窓口と合わせて確かめる。
// あわせて、実アプリで観測の補助(閉じる要求の計数・ナビゲーションの目印・IPC の観測)が働くことを確かめる(2.1、2.3)。
// 起動時の呼び出しは無い(load_source は Run の開始時、set_document_dirty は旗が変わったときだけ呼ばれる)。
import { readFileSync } from "node:fs";
import path from "node:path";
import { expect, test } from "../support/webview/fixtures.ts";
import { installCloseEventCounter, placeNavigationMarker } from "../support/webview/appProbes.ts";
import { REPO_ROOT } from "../support/paths.ts";

const HELLO_SOURCE = readFileSync(path.join(REPO_ROOT, "examples", "hello.bf"), "utf8").trimEnd();

test.describe("スモーク 1: 起動", () => {
  test("スモーク 1 起動の表示: OS のタイトル、エディタの内容、無題、未開始、Run・Step の有効と Pause・Reset の無効", async ({
    app,
  }) => {
    await expect.poll(() => app.host.windowTitle(), { timeout: 30_000 }).toBe("Brainfuck Studio");
    expect((await app.debuggerPage.sourceText()).trimEnd()).toBe(HELLO_SOURCE);
    await expect(app.debuggerPage.documentStatus).toHaveText("無題");
    await expect(app.debuggerPage.statusLabel).toHaveText("未開始");
    await expect(app.debuggerPage.button("Run")).toBeEnabled();
    await expect(app.debuggerPage.button("Step")).toBeEnabled();
    await expect(app.debuggerPage.button("Pause")).toBeDisabled();
    await expect(app.debuggerPage.button("Reset")).toBeDisabled();
  });

  test("スモーク 1 観測の補助: 閉じる要求の計数が設置でき、ナビゲーションの目印が残っていると読め、遷移は 0 回", async ({
    app,
  }) => {
    const closeCounter = await installCloseEventCounter(app.page);
    const marker = await placeNavigationMarker(app.page);

    expect(await closeCounter.count()).toBe(0);
    expect(await marker.isIntact()).toBe(true);
    expect(marker.navigations).toBe(0);
  });

  test("スモーク 1 観測の補助: 接続後にエディタを編集すると set_document_dirty の要求と応答が観測される", async ({
    app,
  }) => {
    const pending = await app.ipc.armResponse("set_document_dirty");

    await app.debuggerPage.editor.click();
    await app.page.keyboard.type("+");

    await pending.completed;
    expect(await app.ipc.commands()).toContain("set_document_dirty");
  });
});
