// webview の準備の確認(setup プロジェクト)。シナリオの前に 1 回だけ走り、準備物が揃っていなければ失敗させる。
// 1. Windows であることを先に判定する(他の OS では OS の窓口を使わない)
// 2. OS の窓口で WebView2 の版を取る。PowerShell が使えなければその旨を不足として扱う
// 3. 実行ファイルと dist/ の有無を合わせ、不足があれば一覧と準備のコマンドを示して失敗する(1.4、1.5)
// 4. WebView2 の版を注釈と添付に記録する(1.10)
// 5. 一時的な利用者データのフォルダの根の古いフォルダを掃除する。失敗は警告の注釈にとどめる(1.8)
import { existsSync, lstatSync, readdirSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "@playwright/test";
import { DIST_INDEX_PATH } from "../support/paths.ts";
import { appBinaryPath, userDataRoot } from "../support/webview/paths.ts";
import { missingWebviewPrerequisites, type PrerequisiteFacts } from "../support/webview/prerequisites.ts";
import { selectStaleUserDataEntries, type UserDataEntry } from "../support/webview/userDataCleanup.ts";
import { HostActionError, readWebView2Version } from "../support/webview/windowsHost.ts";

/**
 * 一時的な利用者データのフォルダの根の古い `scenario-<uuid>` を消す。消せなかったものと、掃除そのものの失敗を
 * 理由つきの文字列で返す(投げない。掃除の失敗はテストを失敗にしない)。リンクは辿らない(lstat・recursive の既定)。
 */
function removeStaleUserDataFolders(root: string, nowMs: number): string[] {
  try {
    if (!existsSync(root)) {
      return [];
    }
    const entries: UserDataEntry[] = readdirSync(root).map((name) => {
      const stat = lstatSync(path.join(root, name));
      return { name, isDirectory: stat.isDirectory(), mtimeMs: stat.mtimeMs };
    });
    const failures: string[] = [];
    for (const target of selectStaleUserDataEntries(root, entries, nowMs)) {
      try {
        rmSync(target, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
      } catch (error) {
        failures.push(`古い利用者データのフォルダを削除できませんでした: ${target}(${String(error)})`);
      }
    }
    return failures;
  } catch (error) {
    return [`一時的な利用者データのフォルダの掃除に失敗しました: ${root}(${String(error)})`];
  }
}

test("webview の準備物が揃っている", async ({}, testInfo) => {
  if (process.platform !== "win32") {
    // Windows 以外は OS の窓口を呼ばず、Windows 限定の 1 行だけを示して失敗する(1.4)
    const lines = missingWebviewPrerequisites({
      platform: process.platform,
      appBinaryExists: false,
      appBinaryPath: "",
      hostAvailable: false,
      webView2Version: null,
      distExists: false,
    });
    throw new Error(lines.join("\n"));
  }

  let webView2Version: string | null = null;
  let hostAvailable = true;
  let hostReason = "";
  try {
    webView2Version = await readWebView2Version();
  } catch (error) {
    if (!(error instanceof HostActionError)) {
      throw error;
    }
    hostAvailable = false;
    hostReason = error.reason;
  }

  const binary = appBinaryPath(process.env);
  const facts: PrerequisiteFacts = {
    platform: process.platform,
    appBinaryExists: existsSync(binary),
    appBinaryPath: binary,
    hostAvailable,
    // OS の窓口が使えないときは版を確かめられないため、WebView2 の不足の行を出さない
    webView2Version: hostAvailable ? webView2Version : "(未確認)",
    distExists: existsSync(DIST_INDEX_PATH),
  };

  const recorded = webView2Version ?? "未導入";
  testInfo.annotations.push({ type: "webview2", description: recorded });
  await testInfo.attach("webview2-version", { body: recorded, contentType: "text/plain" });

  const missing = missingWebviewPrerequisites(facts);
  if (missing.length > 0) {
    const lines = hostAvailable ? missing : [...missing, `OS の窓口の失敗理由: ${hostReason}`];
    throw new Error(`webview の準備物が不足しています:\n${lines.map((line) => `- ${line}`).join("\n")}`);
  }

  for (const failure of removeStaleUserDataFolders(userDataRoot(os.tmpdir()), Date.now())) {
    testInfo.annotations.push({ type: "warning", description: failure });
  }
});
