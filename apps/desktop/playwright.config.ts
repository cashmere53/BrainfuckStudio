// E2E(ブラウザ)の Playwright 設定。
// 画面は `npm run build` の成果物を `vite preview` で配信し、バックエンドはテストごとのブリッジで独立させる。
// 準備物の確認は webServer の起動より前(この設定の読み込み時)に行う。
import { defineConfig, devices } from "@playwright/test";
import { assertPrerequisites } from "./e2e/support/prerequisites.ts";
import { e2ePort } from "./e2e/support/paths.ts";
import { webviewProjects } from "./e2e/support/webview/projects.ts";

// `E2E_SKIP_PREREQ=1` のときは確認を省く(--list・show-report・エディタ拡張・下流の webview 用)
assertPrerequisites(process.env);

const PORT = e2ePort(process.env);
const CI = !!process.env.CI;

/** 1280×800 で揃える(幅 1040 の確認は各シナリオで別に指定する) */
const VIEWPORT = { width: 1280, height: 800 };

export default defineConfig({
  testDir: "e2e/specs",
  outputDir: "test-results",
  // `@perf` を含むシナリオは計測の専用プロジェクトだけで走らせる(10.4)
  projects: [
    {
      name: "chromium",
      grepInvert: /@perf/,
      use: { ...devices["Desktop Chrome"], viewport: VIEWPORT },
    },
    {
      name: "perf",
      grep: /@perf/,
      use: { ...devices["Desktop Chrome"], viewport: VIEWPORT },
    },
    // webview(実アプリ・WebView2)は `E2E_WEBVIEW=1` のときだけ追加される(本 spec の 1.3)
    ...webviewProjects(process.env),
  ],
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    headless: true,
    locale: "ja-JP",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: `npx vite preview --host 127.0.0.1 --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: false,
  },
  reporter: CI
    ? [["list"], ["html", { open: "never" }], ["github"]]
    : [["list"], ["html", { open: "never" }]],
  // 起動直後の page.goto の遅れ(並列の負荷)に備え、CI とローカルの両方で 1 回だけ再試行する。
  // 再試行で通ったものは flaky として報告に残る(README の「並列度と製品の不具合の記録」)
  retries: 1,
  forbidOnly: CI,
});
