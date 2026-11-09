// webview(実アプリ・WebView2)の opt-in のプロジェクト定義。
// `E2E_WEBVIEW=1` のときだけ 2 件を返す。既定の `npm run e2e` の対象(chromium・perf)は変わらない。
// playwright.config.ts の projects の末尾に展開する(10.4、1.3)。
import type { Project } from "@playwright/test";

export const WEBVIEW_ENV_FLAG = "E2E_WEBVIEW";

/**
 * E2E_WEBVIEW=1 のとき [webview-setup, webview] を返す。それ以外は空配列。
 * webview は setup に依存し、1 ワーカー・直列で走る。上流の規約に合わせ @perf は除く。
 */
export function webviewProjects(env: NodeJS.ProcessEnv): Project[] {
  if (env[WEBVIEW_ENV_FLAG] !== "1") {
    return [];
  }
  return [
    {
      name: "webview-setup",
      testDir: "e2e/webview",
      testMatch: /prerequisites\.setup\.ts$/,
    },
    {
      name: "webview",
      testDir: "e2e/webview",
      testMatch: /\.webview\.spec\.ts$/,
      dependencies: ["webview-setup"],
      grepInvert: /@perf/,
      fullyParallel: false,
      workers: 1,
      timeout: 120_000,
    },
  ];
}
