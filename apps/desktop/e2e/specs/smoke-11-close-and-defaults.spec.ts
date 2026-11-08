// スモーク 11(閉じる確認とブラウザ既定動作の抑止): 閉じる要求の模擬と確認ダイアログ(8.1〜8.5)、
// F5 の自動リピート・Ctrl+R・Ctrl+F5 の既定動作の抑止とナビゲーションの無さ(8.6)、
// 既定の右クリックメニューの抑止と編集可能な要素での非抑止(8.7)。
// 閉じる要求は backend.requestWindowClose() で模擬する(実ウィンドウの × ボタンは webview の spec が扱う)。
import { mkdirSync } from "node:fs";
import path from "node:path";
import type { Page, TestInfo } from "@playwright/test";
import { type E2eBackend, expect, test } from "../support/fixtures.ts";
import type { DebuggerPage } from "../support/pages/DebuggerPage.ts";
import { installDefaultActionProbe, readProbe } from "../support/probes.ts";

/** テストごとの出力先(保存のテストでファイルを作る場所。無ければ作る) */
function outputDir(testInfo: TestInfo): string {
  const dir = testInfo.outputPath("files");
  mkdirSync(dir, { recursive: true });
  return dir;
}

/**
 * 否定の検査(確定が要求されていない)の観測時間(ms)。確定の要求は、閉じる要求の後の遅れて届く処理として出得るため、
 * 同期点(ダイアログが消えた等)の後にこの時間だけ待ってから「要求されていない」を確かめる。
 * 肯定の検査は待たずに、Playwright の自動リトライするアサーションで行う。
 */
const LATE_CALL_WINDOW_MS = 600;

/** エディタに文字を入れて未保存にし、未保存の旗がブリッジに記録されるまで待つ */
async function makeDirty(backend: E2eBackend, debuggerPage: DebuggerPage): Promise<void> {
  await debuggerPage.editor.click();
  await debuggerPage.page.keyboard.type("+");
  await expect
    .poll(() => backend.calls("set_document_dirty").at(-1)?.outcome ?? null)
    .toEqual({ ok: true, value: null });
  expect(backend.calls("set_document_dirty").at(-1)?.payload).toEqual({ args: { dirty: true } });
}

test.describe("スモーク 11: 閉じる確認", () => {
  test("未保存の変更があると、閉じる要求で確認ダイアログが 1 回だけ表示される(8.1)", async ({ backend, debuggerPage }) => {
    await makeDirty(backend, debuggerPage);

    expect(await backend.requestWindowClose()).toBe("eventEmitted");

    await expect(debuggerPage.confirmDialog).toHaveCount(1);
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await expect(debuggerPage.confirmDialog).toContainText("未保存の変更");
    await expect(debuggerPage.confirmDialog.getByRole("button", { name: "破棄して閉じる", exact: true })).toBeVisible();
    await expect(debuggerPage.confirmDialog.getByRole("button", { name: "取消", exact: true })).toBeVisible();
    expect(backend.calls("confirm_exit")).toEqual([]);
  });

  test("取消で画面は開いたままになり、編集内容が残り、終了の確定は要求されない(8.2)", async ({ backend, debuggerPage }) => {
    await makeDirty(backend, debuggerPage);
    const edited = await debuggerPage.sourceText();
    expect(edited).toContain("+");

    expect(await backend.requestWindowClose()).toBe("eventEmitted");
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await debuggerPage.confirmDialog.getByRole("button", { name: "取消", exact: true }).click();

    await expect(debuggerPage.confirmDialog).toBeHidden();
    await expect(debuggerPage.editor).toBeVisible();
    // 同期点: ダイアログが消えた後の往復(ソースの読み取り)の後に、確定が要求されていないことを確かめる
    expect(await debuggerPage.sourceText()).toBe(edited);
    expect(backend.calls("confirm_exit")).toEqual([]);
    await expect(debuggerPage.documentStatus).toContainText("未保存の変更あり");
  });

  test("「破棄して閉じる」で終了の確定がちょうど 1 回要求される(8.3)", async ({ backend, debuggerPage }) => {
    await makeDirty(backend, debuggerPage);

    expect(await backend.requestWindowClose()).toBe("eventEmitted");
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await debuggerPage.confirmDialog.getByRole("button", { name: "破棄して閉じる", exact: true }).click();

    await expect(debuggerPage.confirmDialog).toBeHidden();
    await expect
      .poll(() => backend.calls("confirm_exit").at(-1)?.outcome ?? null)
      .toEqual({ ok: true, value: null });
    expect(backend.calls("confirm_exit")).toHaveLength(1);
  });

  test("無変更では確認ダイアログを出さずに閉じる扱いになる(8.4)", async ({ backend, debuggerPage }) => {
    expect(await backend.requestWindowClose()).toBe("closedWithoutConfirmation");

    await expect(debuggerPage.confirmDialog).toHaveCount(0);
    expect(backend.calls("set_document_dirty")).toEqual([]);
    await expect(debuggerPage.editor).toBeVisible();
  });

  test("保存済みでは確認ダイアログを出さずに閉じる扱いになる(8.4)", async ({ backend, debuggerPage }, testInfo) => {
    await makeDirty(backend, debuggerPage);
    const target = path.join(outputDir(testInfo), "saved-11.bf");
    backend.dialogs.enqueue("save", { kind: "path", path: target });
    await debuggerPage.page.keyboard.press("ControlOrMeta+S");
    await expect(debuggerPage.documentStatus).toHaveText("saved-11.bf");
    await expect
      .poll(() => backend.calls("set_document_dirty").at(-1)?.payload)
      .toEqual({ args: { dirty: false } });

    expect(await backend.requestWindowClose()).toBe("closedWithoutConfirmation");

    await expect(debuggerPage.confirmDialog).toHaveCount(0);
  });

  test("確認ダイアログの表示中に閉じる要求が重ねて届いても、確認ダイアログは二重に表示されない(8.5)", async ({
    backend,
    debuggerPage,
  }) => {
    await makeDirty(backend, debuggerPage);

    expect(await backend.requestWindowClose()).toBe("eventEmitted");
    await expect(debuggerPage.confirmDialog).toHaveCount(1);

    expect(await backend.requestWindowClose()).toBe("eventEmitted");
    expect(await backend.requestWindowClose()).toBe("eventEmitted");

    await expect(debuggerPage.confirmDialog).toHaveCount(1);
    await expect(debuggerPage.confirmDialog).toBeVisible();
    expect(backend.calls("confirm_exit")).toEqual([]);

    // 同期点: 「取消」で閉じる。重複したダイアログがあれば 1 つ目を閉じても残るため、ダイアログが 0 になるまで待つと二重表示を検出できる
    await debuggerPage.confirmDialog.getByRole("button", { name: "取消", exact: true }).click();
    await expect(debuggerPage.confirmDialog).toHaveCount(0);
    // 同期点の後、遅れて届く確定の要求を観測する時間を置いてから、確定が要求されていないことを確かめる
    await debuggerPage.page.waitForTimeout(LATE_CALL_WINDOW_MS);
    expect(backend.calls("confirm_exit")).toEqual([]);
  });
});

/**
 * 再読み込みの検出の補助。ページ内の文書の移動(framenavigated)を記録する。
 * 注意: Playwright のキー入力は CDP で送られ、既定動作を抑止しなくても URL・ページ内のマーカーが
 * 変わらないことがある。そのため URL とマーカーの検査は再読み込みの検出力を持たない(補強にすぎない)。
 * 実際の再読み込みは desktop-gui-e2e-webview(実ウィンドウ)が検証する。ここでは文書の移動の発火回数で確かめる。
 */
function recordNavigations(page: Page): string[] {
  const navigations: string[] = [];
  page.on("framenavigated", (frame) => {
    if (frame === page.mainFrame()) {
      navigations.push(frame.url());
    }
  });
  return navigations;
}

test.describe("スモーク 11: ブラウザ既定動作の抑止", () => {
  test("F5 の押しっぱなし(自動リピート)で既定動作が抑止され、再読み込みされず編集内容が残る(8.6)", async ({
    backend,
    debuggerPage,
  }) => {
    const { page } = debuggerPage;
    await makeDirty(backend, debuggerPage);
    const edited = await debuggerPage.sourceText();
    await installDefaultActionProbe(page);
    await page.evaluate(() => {
      (window as unknown as { __e2eMarker?: string }).__e2eMarker = "kept";
    });
    const url = page.url();
    const navigations = recordNavigations(page);

    await page.keyboard.down("F5");
    await page.keyboard.down("F5");
    await page.keyboard.up("F5");

    const keys = (await readProbe(page)).filter((record) => record.type === "keydown" && record.key === "F5");
    expect(keys.map((record) => ({ repeat: record.repeat, defaultPrevented: record.defaultPrevented }))).toEqual([
      { repeat: false, defaultPrevented: true },
      { repeat: true, defaultPrevented: true },
    ]);
    // 補強: 文書の移動が起きていないこと(検出力は 'framenavigated' の発火回数で持たせる。上の URL とマーカーは補助)
    expect(navigations, "文書の移動の回数").toEqual([]);
    expect(page.url()).toBe(url);
    expect(await page.evaluate(() => (window as unknown as { __e2eMarker?: string }).__e2eMarker)).toBe("kept");
    expect(await debuggerPage.sourceText()).toBe(edited);
  });

  test("Ctrl+R と Ctrl+F5 で既定動作が抑止され、再読み込みされず編集内容が残る(8.6)", async ({ backend, debuggerPage }) => {
    const { page } = debuggerPage;
    await makeDirty(backend, debuggerPage);
    const edited = await debuggerPage.sourceText();
    await installDefaultActionProbe(page);
    await page.evaluate(() => {
      (window as unknown as { __e2eMarker?: string }).__e2eMarker = "kept";
    });
    const url = page.url();
    const navigations = recordNavigations(page);

    await page.keyboard.press("Control+R");
    await page.keyboard.press("Control+F5");

    const reloadKeys = (await readProbe(page))
      .filter((record) => record.type === "keydown" && record.key !== "Control")
      // Ctrl+R の key は Shift の状態で大小が変わるため、文字は大小を区別せずに比べる
      .map((record) => ({ key: record.key?.toLowerCase(), defaultPrevented: record.defaultPrevented }));
    expect(reloadKeys).toEqual([
      { key: "r", defaultPrevented: true },
      { key: "f5", defaultPrevented: true },
    ]);
    // 補強: 文書の移動が起きていないこと(URL とマーカーは補助。実際の再読み込みは webview の spec が検証する)
    expect(navigations, "文書の移動の回数").toEqual([]);
    expect(page.url()).toBe(url);
    expect(await page.evaluate(() => (window as unknown as { __e2eMarker?: string }).__e2eMarker)).toBe("kept");
    expect(await debuggerPage.sourceText()).toBe(edited);
  });
});

test.describe("スモーク 11: 右クリックの既定メニューの抑止", () => {
  test("編集可能でない要素では既定のメニューが抑止され、エディタと入力欄では抑止されない(8.7)", async ({
    debuggerPage,
  }) => {
    const { page } = debuggerPage;
    await installDefaultActionProbe(page);

    await debuggerPage.region("状態").click({ button: "right" });
    await debuggerPage.region("操作").click({ button: "right" });
    await debuggerPage.editor.click({ button: "right" });
    await debuggerPage.inputBox.click({ button: "right" });

    const menus = (await readProbe(page))
      .filter((record) => record.type === "contextmenu")
      .map((record) => ({ targetEditable: record.targetEditable, defaultPrevented: record.defaultPrevented }));
    expect(menus).toEqual([
      { targetEditable: false, defaultPrevented: true },
      { targetEditable: false, defaultPrevented: true },
      { targetEditable: true, defaultPrevented: false },
      { targetEditable: true, defaultPrevented: false },
    ]);
  });
});
