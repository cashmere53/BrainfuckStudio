// webview(実アプリ・WebView2)の E2E が使うパスと定数。環境変数は呼び出し側から渡す。
// 上流(ブラウザ)の paths.ts は import しない(cli.ts が Node の型の除去で直接読み込むため)。
import path from "node:path";
import { fileURLToPath } from "node:url";

/** apps/desktop(このファイルは apps/desktop/e2e/support/webview にある) */
export const DESKTOP_ROOT = fileURLToPath(new URL("../../..", import.meta.url));

/** 画面のビルド成果物(`npm run build` の出力。上流の DIST_INDEX_PATH と同じ) */
export const DIST_INDEX_PATH = path.join(DESKTOP_ROOT, "dist", "index.html");

/** 組み込みの画面の配信オリジン(Windows の Tauri 2 の既定) */
export const APP_ORIGIN = "http://tauri.localhost";

/** 実アプリの IPC の要求の URL の接頭辞(コマンド名がパスに入る) */
export const IPC_URL_PREFIX = "http://ipc.localhost/";

/** 実アプリの実行ファイル名(tauri build --debug --no-bundle の出力) */
export const APP_EXE_NAME = "brainfuck-studio.exe";

/** OS の操作の補助スクリプト(本ディレクトリの windowsHost.ps1) */
export const WINDOWS_HOST_SCRIPT_PATH = fileURLToPath(new URL("./windowsHost.ps1", import.meta.url));

/** 空文字の環境変数は未設定として扱う */
function nonEmpty(value: string | undefined): string | undefined {
  return value === undefined || value === "" ? undefined : value;
}

/**
 * 実アプリの実行ファイルのパスを決める。優先順位は
 * `E2E_APP_BIN`(明示の指定) > `CARGO_TARGET_DIR`(配下の debug) > `apps/desktop/src-tauri/target`(既定の debug)。
 * 相対パスは実行時のカレントディレクトリ(npm スクリプトでは apps/desktop)を基準にする。
 */
export function appBinaryPath(env: NodeJS.ProcessEnv): string {
  const explicit = nonEmpty(env.E2E_APP_BIN);
  if (explicit !== undefined) {
    return path.resolve(explicit);
  }
  const targetDir = nonEmpty(env.CARGO_TARGET_DIR);
  if (targetDir !== undefined) {
    return path.join(path.resolve(targetDir), "debug", APP_EXE_NAME);
  }
  return path.join(DESKTOP_ROOT, "src-tauri", "target", "debug", APP_EXE_NAME);
}

/** 一時的な利用者データのフォルダの根(引数のディレクトリの下の bfstudio-e2e-webview) */
export function userDataRoot(tmpDir: string): string {
  return path.join(tmpDir, "bfstudio-e2e-webview");
}
