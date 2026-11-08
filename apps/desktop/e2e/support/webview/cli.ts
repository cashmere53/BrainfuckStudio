// webview(実アプリ・WebView2)のランチャ。`npm run e2e:webview:run -- <引数>` から Node の型の除去で直接実行する。
// 1. Windows であることと dist/ の有無を Playwright の起動より前に確かめ、不足なら一覧を示して終了コード 1
// 2. E2E_WEBVIEW=1・E2E_SKIP_PREREQ=1・E2E_PORT(未指定なら空きポート)を付けて、webview のプロジェクトを 1 ワーカーで起動
//
// 型の除去の制約: この file が推移的に import するファイルは、消去可能な構文だけを使い、相対 import に .ts を付ける。
// 値の import は本ディレクトリの prerequisites.ts と Node の組み込みモジュールだけにする(上流の paths.ts は import しない)。
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { missingLaunchPrerequisites } from "./prerequisites.ts";

/** apps/desktop(このファイルは apps/desktop/e2e/support/webview にある) */
const DESKTOP_ROOT = path.resolve(import.meta.dirname, "../../..");

export type WebviewRun = {
  readonly command: string;
  readonly args: string[];
  readonly env: NodeJS.ProcessEnv;
};

/** 画面のビルド成果物 dist/index.html(上流の DIST_INDEX_PATH と同じ場所。import せず自分で求める) */
export function webviewDistIndexPath(): string {
  return path.join(DESKTOP_ROOT, "dist", "index.html");
}

/**
 * Playwright の起動の指定(純粋な組み立て)。現在の Node で Playwright の CLI を起動し、
 * 利用者の引数は後ろにそのまま渡す。E2E_PORT が空でなければそれを使い、無ければ空きポートを使う。
 */
export function buildWebviewRun(
  argv: readonly string[],
  env: NodeJS.ProcessEnv,
  freePort: number,
): WebviewRun {
  const playwrightCli = path.join(DESKTOP_ROOT, "node_modules", "@playwright", "test", "cli.js");
  const port = env.E2E_PORT !== undefined && env.E2E_PORT !== "" ? env.E2E_PORT : String(freePort);
  return {
    command: process.execPath,
    args: [playwrightCli, "test", "--project=webview", "--workers=1", ...argv],
    env: {
      ...env,
      E2E_WEBVIEW: "1",
      E2E_SKIP_PREREQ: "1",
      E2E_PORT: port,
    },
  };
}

/** 127.0.0.1 の空きポートを 1 つ取る(取得後に閉じるため、競合の余地は残る) */
function freeLocalPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        if (address === null || typeof address === "string") {
          reject(new Error("空きポートを取得できませんでした"));
          return;
        }
        resolve(address.port);
      });
    });
  });
}

/** ランチャの外部への依存(テストでは差し替える) */
export type LauncherDeps = {
  readonly platform: NodeJS.Platform;
  /** dist/index.html の有無 */
  readonly distExists: boolean;
  readonly env: NodeJS.ProcessEnv;
  /** 空きポート(取得に失敗したら reject) */
  readonly freePort: () => Promise<number>;
  /** Playwright の起動(同期)。結果は spawnSync の形と同じ */
  readonly spawn: (run: WebviewRun) => { readonly error?: Error; readonly status: number | null };
};

/** 実際の環境に対する既定の依存(直接実行されたときの入口が使う) */
export function defaultLauncherDeps(): LauncherDeps {
  return {
    platform: process.platform,
    distExists: existsSync(webviewDistIndexPath()),
    env: process.env,
    freePort: freeLocalPort,
    spawn: (run) =>
      spawnSync(run.command, run.args, { cwd: DESKTOP_ROOT, env: run.env, stdio: "inherit" }),
  };
}

/**
 * ランチャの本体。準備の確認 → 空きポート → Playwright の起動の順。
 * 準備が不足なら起動せず 1 を返す。Playwright の終了コードをそのまま返す(起動自体に失敗したら 1)。
 */
export async function runLauncher(argv: readonly string[], deps: LauncherDeps): Promise<number> {
  const missing = missingLaunchPrerequisites({
    platform: deps.platform,
    distExists: deps.distExists,
  });
  if (missing.length > 0) {
    for (const line of missing) {
      console.error(line);
    }
    return 1;
  }
  const port = await deps.freePort();
  const run = buildWebviewRun(argv, deps.env, port);
  const result = deps.spawn(run);
  if (result.error !== undefined) {
    console.error(`Playwright の起動に失敗しました: ${result.error.message}`);
    return 1;
  }
  return result.status ?? 1;
}

// 直接実行されたときだけ動く(テストの import では動かない)
const entry = process.argv[1];
const isEntry =
  entry !== undefined &&
  path.resolve(entry).toLowerCase() === fileURLToPath(import.meta.url).toLowerCase();
if (isEntry) {
  runLauncher(process.argv.slice(2), defaultLauncherDeps()).then((code) => {
    process.exitCode = code;
  });
}
