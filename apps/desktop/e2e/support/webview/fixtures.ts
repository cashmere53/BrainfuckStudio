// webview のフィクスチャ(WebviewFixtures)。シナリオごとに実アプリを起動して CDP で接続し、配信オリジンのページを特定して
// エディタの表示を待ち、ページ・上流のページオブジェクト・pid に束ねた OS の窓口・接続直後からの IPC の観測を渡す。
// 後始末では、失敗ならスクリーンショットと出力の末尾を添付し、WebView2 の版を注釈し、接続を切ってプロセスを終了させる。
// 組み込みの browser(と、それに依存する context・page)は無効化する。Chromium を起動させないため。
import os from "node:os";
import { chromium, expect, test as base, type Browser, type Page, type TestInfo } from "@playwright/test";
import { DebuggerPage } from "../pages/DebuggerPage.ts";
import { appBinaryPath, APP_ORIGIN, userDataRoot } from "./paths.ts";
import { IPC_OBSERVATION_MODE, observeIpc, type IpcObserver } from "./appProbes.ts";
import { AppLaunchError, launchApp, readyWithinMsFromEnv, type LaunchedApp } from "./appProcess.ts";
import { hostCallRefusal, shouldCollectFailureMaterials } from "./fixtureFailure.ts";
import { bindWindowsHost, type BoundWindowsHost } from "./windowsHost.ts";

/**
 * pid に束ねた OS の窓口を、アプリが終了していない間だけ呼べるように包む。
 * 終了した後に呼ばれたら、OS の操作を送らずに理由つきで失敗する(pid の再利用で別のウィンドウへ届くのを防ぐ)。
 */
function guardedHost(launched: LaunchedApp): BoundWindowsHost {
  const host = bindWindowsHost(launched.pid);
  const guard = (operation: string): void => {
    const refusal = hostCallRefusal(launched.exit, operation, launched.pid);
    if (refusal !== null) {
      throw new Error(refusal);
    }
  };
  return {
    async windowTitle() {
      guard("windowTitle");
      return host.windowTitle();
    },
    async requestClose() {
      guard("requestClose");
      return host.requestClose();
    },
    async sendKeys(keys: string) {
      guard("sendKeys");
      return host.sendKeys(keys);
    },
    async contextMenuOpen() {
      guard("contextMenuOpen");
      return host.contextMenuOpen();
    },
  };
}

/** シナリオに渡す実アプリの一式 */
export interface WebviewApp {
  /** 起動したプロセス(pid・CDP の口・終了の状態) */
  readonly process: LaunchedApp;
  /** CDP で得たアプリのページ(URL が APP_ORIGIN で始まる) */
  readonly page: Page;
  /** 上流のデバッガ画面のページオブジェクト */
  readonly debuggerPage: DebuggerPage;
  /** pid に束ねた OS の窓口 */
  readonly host: BoundWindowsHost;
  /** 接続直後から観測する IPC(方式は IPC_OBSERVATION_MODE) */
  readonly ipc: IpcObserver;
}

type WebviewFixtures = { app: WebviewApp };

/** 組み込みの browser を無効化したときの文言。シナリオは app フィクスチャだけを使う */
export const BUILTIN_BROWSER_DISABLED_MESSAGE =
  "webview のシナリオでは組み込みの browser・context・page は使えません(app フィクスチャを使う)";

/** 配信オリジンのページが現れるまで既定のコンテキストを見て待つ。上限を過ぎたら例外 */
export async function waitForAppPage(browser: Browser, timeoutMs: number): Promise<Page> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const context = browser.contexts()[0];
    const found = context?.pages().find((page) => page.url().startsWith(APP_ORIGIN));
    if (found !== undefined) {
      return found;
    }
    if (Date.now() >= deadline) {
      throw new Error(`配信オリジン ${APP_ORIGIN} のページが ${timeoutMs} ms 以内に見つかりませんでした`);
    }
    await new Promise<void>((resolve) => setTimeout(resolve, 250));
  }
}

/** 後始末の 1 件の失敗を注釈の警告にする(後始末は続ける。テストの結果は変えない) */
function warn(testInfo: TestInfo, description: string): void {
  testInfo.annotations.push({ type: "warning", description });
}

export const test = base.extend<WebviewFixtures, { browser: Browser }>({
  // 組み込みの browser を無効化する。page・context は browser に依存するため、これらも使えなくなる
  browser: [
    async ({}) => {
      throw new Error(BUILTIN_BROWSER_DISABLED_MESSAGE);
    },
    { scope: "worker" },
  ],

  app: async ({}, use, testInfo) => {
    const readyWithinMs = readyWithinMsFromEnv(process.env);
    let launched: LaunchedApp;
    try {
      launched = await launchApp({
        exePath: appBinaryPath(process.env),
        userDataRoot: userDataRoot(os.tmpdir()),
        readyWithinMs,
      });
    } catch (error) {
      if (error instanceof AppLaunchError) {
        await testInfo.attach("app-output", { body: error.outputTail, contentType: "text/plain" });
      }
      throw error;
    }

    let browser: Browser | null = null;
    let page: Page | null = null;
    // 準備の例外は、テストの状態が passed のまま伝わるため、状態だけでは見分けられない。別に記録する
    let setupFailed = false;
    try {
      let ipc: IpcObserver;
      let debuggerPage: DebuggerPage;
      try {
        browser = await chromium.connectOverCDP(launched.cdpEndpoint);
        page = await waitForAppPage(browser, readyWithinMs);
        debuggerPage = new DebuggerPage(page);
        await expect(debuggerPage.editor).toBeVisible({ timeout: readyWithinMs });
        ipc = await observeIpc(page, IPC_OBSERVATION_MODE);
      } catch (error) {
        setupFailed = true;
        throw error;
      }

      await use({
        process: launched,
        page,
        debuggerPage,
        host: guardedHost(launched),
        ipc,
      });
    } finally {
      // 失敗のときだけ材料を残す(テストの結果は呼び出し側が決める)。準備の失敗も含める
      if (shouldCollectFailureMaterials(setupFailed, testInfo.status, testInfo.expectedStatus)) {
        if (page !== null) {
          try {
            const screenshot = await page.screenshot();
            await testInfo.attach("failure-screenshot", { body: screenshot, contentType: "image/png" });
          } catch (error) {
            warn(testInfo, `失敗時のスクリーンショットを取れませんでした: ${String(error)}`);
          }
        }
        await testInfo.attach("app-output", { body: launched.outputTail, contentType: "text/plain" });
      }

      testInfo.annotations.push({ type: "webview2", description: launched.runtimeVersion });

      if (browser !== null) {
        await browser.close().catch((error: unknown) => {
          warn(testInfo, `CDP の接続を切れませんでした: ${String(error)}`);
        });
      }
      const { userDataRemoved } = await launched.terminate();
      if (!userDataRemoved) {
        warn(testInfo, `利用者データのフォルダを削除できませんでした: ${launched.userDataDir}`);
      }
    }
  },
});

export { expect } from "@playwright/test";
