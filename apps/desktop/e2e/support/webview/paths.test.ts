// @vitest-environment node
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  APP_EXE_NAME,
  APP_ORIGIN,
  DESKTOP_ROOT,
  IPC_URL_PREFIX,
  WINDOWS_HOST_SCRIPT_PATH,
  appBinaryPath,
  userDataRoot,
} from "./paths.ts";

describe("appBinaryPath(実アプリの実行ファイルの優先順位)", () => {
  it("E2E_APP_BIN が最優先で、CARGO_TARGET_DIR より先に使われる", () => {
    const bin = appBinaryPath({ E2E_APP_BIN: "/custom/app.exe", CARGO_TARGET_DIR: "/target-dir" });
    expect(bin).toBe(path.resolve("/custom/app.exe"));
  });

  it("E2E_APP_BIN が無ければ CARGO_TARGET_DIR の debug 配下の brainfuck-studio.exe を使う", () => {
    const bin = appBinaryPath({ CARGO_TARGET_DIR: "/target-dir" });
    expect(bin).toBe(path.join(path.resolve("/target-dir"), "debug", "brainfuck-studio.exe"));
  });

  it("どちらも無ければ apps/desktop/src-tauri/target/debug の既定の場所を使う", () => {
    const bin = appBinaryPath({});
    expect(bin).toBe(path.join(DESKTOP_ROOT, "src-tauri", "target", "debug", "brainfuck-studio.exe"));
  });

  it("空文字の環境変数は未設定として扱う", () => {
    const bin = appBinaryPath({ E2E_APP_BIN: "", CARGO_TARGET_DIR: "" });
    expect(bin).toBe(path.join(DESKTOP_ROOT, "src-tauri", "target", "debug", APP_EXE_NAME));
  });
});

describe("定数(配信オリジン・IPC の接頭辞・実行ファイル名・補助スクリプト)", () => {
  it("組み込みの画面の配信オリジンと IPC の接頭辞は Tauri 2 の Windows の既定", () => {
    expect(APP_ORIGIN).toBe("http://tauri.localhost");
    expect(IPC_URL_PREFIX).toBe("http://ipc.localhost/");
  });

  it("実行ファイル名は brainfuck-studio.exe", () => {
    expect(APP_EXE_NAME).toBe("brainfuck-studio.exe");
  });

  it("OS の操作の補助スクリプトは webview ディレクトリの windowsHost.ps1 を指す", () => {
    expect(path.basename(WINDOWS_HOST_SCRIPT_PATH)).toBe("windowsHost.ps1");
    expect(path.dirname(WINDOWS_HOST_SCRIPT_PATH)).toBe(fileURLToPath(new URL(".", import.meta.url)).replace(/[\\/]$/, ""));
  });

});

describe("userDataRoot(一時的な利用者データのフォルダの根)", () => {
  it("引数のディレクトリの下の bfstudio-e2e-webview を返す", () => {
    expect(userDataRoot("/tmp-base")).toBe(path.join("/tmp-base", "bfstudio-e2e-webview"));
  });

  it("os.tmpdir() の下に作る場合も同じ名前のフォルダになる", () => {
    expect(path.basename(userDataRoot(os.tmpdir()))).toBe("bfstudio-e2e-webview");
  });
});
