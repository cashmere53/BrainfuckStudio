// E2E が使う実行ファイルと成果物のパスの解決。純粋な関数(resolveBridgeBinary・e2ePort)と、
// このリポジトリの場所に基づく定数に分けている。環境変数は呼び出し側から渡す。
import path from "node:path";
import { fileURLToPath } from "node:url";

/** 既定の配信ポート。開発サーバの 1420 と分ける(要件 1.5) */
export const DEFAULT_E2E_PORT = 14273;

/** apps/desktop(このファイルは apps/desktop/e2e/support にある) */
export const DESKTOP_ROOT = fileURLToPath(new URL("../..", import.meta.url));

/** リポジトリのルート(apps/desktop の 2 つ上) */
export const REPO_ROOT = fileURLToPath(new URL("../../../..", import.meta.url));

/** 画面のビルド成果物(`npm run build` の出力) */
export const DIST_INDEX_PATH = path.join(DESKTOP_ROOT, "dist", "index.html");

/** IPC シムのビルド成果物(`npm run e2e:shim` の出力) */
export const SHIM_BUNDLE_PATH = path.join(DESKTOP_ROOT, "e2e", ".cache", "ipc-shim.js");

export type BridgeBinaryOptions = {
  /** Cargo のワークスペースのルート(既定の target の基準) */
  readonly repoRoot: string;
  /** 実行ファイルの拡張子を決める OS(既定は実行中の OS) */
  readonly platform: NodeJS.Platform;
};

/** 空文字の環境変数は未設定として扱う */
function nonEmpty(value: string | undefined): string | undefined {
  return value === undefined || value === "" ? undefined : value;
}

/**
 * ブリッジの実行ファイルのパスを決める。優先順位は
 * `E2E_BRIDGE_BIN` > `CARGO_TARGET_DIR`(配下の debug) > `<repoRoot>/target`(配下の debug)。
 * 相対パスは Cargo と同じく実行時のカレントディレクトリ(npm スクリプトでは apps/desktop)を基準にする。
 */
export function resolveBridgeBinary(env: NodeJS.ProcessEnv, options: BridgeBinaryOptions): string {
  const exe = options.platform === "win32" ? "brainfuck-e2e-bridge.exe" : "brainfuck-e2e-bridge";
  const explicit = nonEmpty(env.E2E_BRIDGE_BIN);
  if (explicit !== undefined) {
    return path.resolve(explicit);
  }
  const targetDir = nonEmpty(env.CARGO_TARGET_DIR);
  const base = targetDir !== undefined ? path.resolve(targetDir) : path.join(options.repoRoot, "target");
  return path.join(base, "debug", exe);
}

/** 実行中の環境でのブリッジの実行ファイルのパス */
export function bridgeBinaryPath(env: NodeJS.ProcessEnv): string {
  return resolveBridgeBinary(env, { repoRoot: REPO_ROOT, platform: process.platform });
}

/** `E2E_PORT` を解釈する。未設定なら既定値、数字以外や範囲外は例外 */
export function e2ePort(env: NodeJS.ProcessEnv): number {
  const raw = nonEmpty(env.E2E_PORT);
  if (raw === undefined) {
    return DEFAULT_E2E_PORT;
  }
  if (!/^\d+$/.test(raw)) {
    throw new Error(`E2E_PORT は 1〜65535 の整数で指定してください(指定値: ${raw})`);
  }
  const port = Number(raw);
  if (port < 1 || port > 65535) {
    throw new Error(`E2E_PORT は 1〜65535 の整数で指定してください(指定値: ${raw})`);
  }
  return port;
}
