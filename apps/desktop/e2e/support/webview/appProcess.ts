// 実アプリのプロセスの管理(AppLauncher)。シナリオごとに空きポートと一時的な利用者データのフォルダを割り当て、
// WebView2 の環境変数つきでシェルを介さずに起動し、CDP の口が応答するまで待つ。終了はプロセスツリーごと行う。
// 純粋関数(buildAppEnv、readyWithinMsFromEnv、waitForCdpEndpoint)は偽の取得関数で単体テストする。
import { execFile, spawn, type SpawnOptions } from "node:child_process";
import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { SCENARIO_FOLDER_PREFIX } from "./userDataCleanup.ts";

export type AppExit = { code: number | null; signal: NodeJS.Signals | null };
export type CdpVersion = { browser: string; webSocketDebuggerUrl: string };

export interface LaunchedApp {
  readonly pid: number;
  readonly cdpEndpoint: string; // http://127.0.0.1:<port>
  readonly userDataDir: string;
  readonly runtimeVersion: string; // /json/version の Browser(例: Edg/1xx.x)
  readonly exited: Promise<AppExit>;
  readonly exit: AppExit | null; // 動いていれば null
  readonly outputTail: string; // 標準出力と標準エラーの末尾(最大 64 KiB)
  /** 動いていればプロセスツリーを終了し、終了を待ってからフォルダを削除する。削除の失敗は結果で返す(投げない) */
  terminate(): Promise<{ userDataRemoved: boolean }>;
}

export type LaunchOptions = { exePath: string; userDataRoot: string; readyWithinMs: number };

/** 起動の失敗の種類。理由と出力の末尾を持つ */
export class AppLaunchError extends Error {
  readonly reason: "notFound" | "exitedEarly" | "notReady";
  readonly outputTail: string;

  constructor(reason: AppLaunchError["reason"], message: string, outputTail: string) {
    super(message);
    this.name = "AppLaunchError";
    this.reason = reason;
    this.outputTail = outputTail;
  }
}

/** 上限時間の既定(ミリ秒)。E2E_APP_READY_MS で変更する */
export const DEFAULT_READY_WITHIN_MS = 60_000;

/** 出力の末尾として保持する上限(文字数の目安) */
const OUTPUT_TAIL_LIMIT = 64 * 1024;

/** CDP の口の再試行の間隔 */
const POLL_INTERVAL_MS = 250;

/** 1 回の /json/version の取得の上限 */
const FETCH_TIMEOUT_MS = 2_000;

/** taskkill の完了を待つ上限 */
const KILL_WAIT_MS = 15_000;

/** WebView2 の既定の環境変数の接頭辞。大文字・小文字を区別せず除く(Windows の環境変数の名前は大文字・小文字を区別しない) */
const WEBVIEW2_PREFIX = "WEBVIEW2_";

/** 上限時間を環境変数 E2E_APP_READY_MS から読む。未指定・正の整数でなければ既定 */
export function readyWithinMsFromEnv(env: NodeJS.ProcessEnv): number {
  const raw = env.E2E_APP_READY_MS;
  if (raw === undefined || raw === "") {
    return DEFAULT_READY_WITHIN_MS;
  }
  const value = Number(raw);
  return Number.isInteger(value) && value > 0 ? value : DEFAULT_READY_WITHIN_MS;
}

/** 127.0.0.1 の空きポートを 0 番で bind して得る */
export function findFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (address === null || typeof address === "string") {
        server.close();
        reject(new Error("空きポートを取得できませんでした"));
        return;
      }
      const port = address.port;
      server.close(() => resolve(port));
    });
  });
}

/**
 * 継承した WEBVIEW2_* を除き、デバッグ口(追加の引数)と利用者データのフォルダの 2 つだけを設定した環境(純粋関数)。
 * アプリの既定の追加引数は写さない。環境変数はアプリの指定に追加される(research.md)。
 */
export function buildAppEnv(base: NodeJS.ProcessEnv, port: number, userDataDir: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {};
  for (const [key, value] of Object.entries(base)) {
    if (key.toUpperCase().startsWith(WEBVIEW2_PREFIX)) {
      continue;
    }
    env[key] = value;
  }
  env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = `--remote-debugging-port=${port}`;
  env.WEBVIEW2_USER_DATA_FOLDER = userDataDir;
  return env;
}

/** 既定の取得関数: 127.0.0.1 の /json/version を読む。応答しない・不正なら null */
export async function fetchCdpVersion(port: number): Promise<CdpVersion | null> {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json/version`, {
      signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
    });
    if (!response.ok) {
      return null;
    }
    const body: unknown = await response.json();
    if (typeof body !== "object" || body === null) {
      return null;
    }
    const record = body as Record<string, unknown>;
    if (typeof record.Browser !== "string" || typeof record.webSocketDebuggerUrl !== "string") {
      return null;
    }
    return { browser: record.Browser, webSocketDebuggerUrl: record.webSocketDebuggerUrl };
  } catch {
    return null;
  }
}

function notReadyMessage(readyWithinMs: number): string {
  return [
    `アプリの CDP の口が ${readyWithinMs} ms 以内に応答しませんでした(E2E_APP_READY_MS で上限を変更できます)。`,
    "次を確かめてください:",
    "- ウイルス対策ソフト(Avast など)が新しい exe を止めていないか。除外に登録するか、一時停止してから再実行してください",
    "- 新しい CARGO_TARGET_DIR で再ビルドしてください(exe が新しく作られた直後に止められることがあります)",
    "- 固まったアプリのプロセスが残っていれば、再起動(タスクマネージャーでの終了)で消えます",
  ].join("\n");
}

/**
 * CDP の口のバージョン情報が応答するまで再試行する。
 * プロセスが終了したら早期終了として、上限時間を超えたら案内つきで AppLaunchError を投げる。
 * 取得関数・終了の判定・出力の末尾は注入できる(単体テストで偽の値を使う)。
 */
export async function waitForCdpEndpoint(
  port: number,
  deps: {
    readyWithinMs: number;
    fetchVersion: (port: number) => Promise<CdpVersion | null>;
    hasExited: () => boolean;
    outputTail?: () => string;
    intervalMs?: number;
  },
): Promise<CdpVersion> {
  const interval = deps.intervalMs ?? POLL_INTERVAL_MS;
  const deadline = Date.now() + deps.readyWithinMs;
  for (;;) {
    const version = await deps.fetchVersion(port);
    if (version !== null) {
      return version;
    }
    const tail = deps.outputTail?.() ?? "";
    if (deps.hasExited()) {
      throw new AppLaunchError(
        "exitedEarly",
        `アプリが CDP の口の応答の前に早期に終了しました(ポート ${port})。出力の末尾を確かめてください`,
        tail,
      );
    }
    if (Date.now() >= deadline) {
      throw new AppLaunchError("notReady", notReadyMessage(deps.readyWithinMs), tail);
    }
    await new Promise((resolve) => setTimeout(resolve, interval));
  }
}

/** 出力の末尾だけを残す(上限を超えた分は先頭から落とす) */
export function appendTail(current: string, chunk: string): string {
  const next = current + chunk;
  return next.length > OUTPUT_TAIL_LIMIT ? next.slice(next.length - OUTPUT_TAIL_LIMIT) : next;
}

/** シナリオごとの利用者データのフォルダ(根の直下の `scenario-<uuid>`)。根は絶対パスに解決する */
export function scenarioUserDataDir(root: string): string {
  return path.join(path.resolve(root), `${SCENARIO_FOLDER_PREFIX}${randomUUID()}`);
}

/** taskkill の引数(純粋関数)。/T で子プロセス(WebView2 のプロセス)を含め、/F で強制終了する */
export function taskkillArgs(pid: number): string[] {
  return ["/PID", String(pid), "/T", "/F"];
}

/** プロセスツリーを taskkill で終わらせる(WebView2 の子プロセスを含む)。終了を待つ */
function killTree(pid: number): Promise<void> {
  return new Promise((resolve) => {
    execFile("taskkill", taskkillArgs(pid), { windowsHide: true, timeout: KILL_WAIT_MS }, () => {
      resolve();
    });
  });
}

/** 削除の I/O の型(rm と同じ形。単体テストで差し替える) */
export type RemoveFn = (dir: string, options: { recursive: boolean; force: boolean; maxRetries: number; retryDelay: number }) => Promise<void>;

/** 利用者データのフォルダを再試行つきで削除する。成否を返す(投げない) */
export async function removeUserDataDir(dir: string, rmImpl: RemoveFn = rm): Promise<boolean> {
  try {
    await rmImpl(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
    return true;
  } catch {
    return false;
  }
}

/**
 * 実アプリの spawn の引数(純粋関数)。シェルを介さず(shell を指定しない)、コンソールの窓は隠さず、
 * 標準入力は無視し、標準出力と標準エラーは受け取る。
 */
export function appSpawnOptions(env: NodeJS.ProcessEnv): SpawnOptions {
  return {
    env,
    windowsHide: false,
    stdio: ["ignore", "pipe", "pipe"],
  };
}

/**
 * 実アプリを起動して CDP の口が応答するまで待つ。シナリオごとに空きポートと一時フォルダを割り当てる。
 * 起動はシェルを介さず、標準出力と標準エラーの末尾を保持する。
 */
export async function launchApp(options: LaunchOptions): Promise<LaunchedApp> {
  if (!existsSync(options.exePath)) {
    throw new AppLaunchError("notFound", `実行ファイルが見つかりません: ${options.exePath}`, "");
  }

  const port = await findFreePort();
  const userDataDir = scenarioUserDataDir(options.userDataRoot);
  const child = spawn(options.exePath, [], appSpawnOptions(buildAppEnv(process.env, port, userDataDir)));

  let tail = "";
  const onOutput = (chunk: string): void => {
    tail = appendTail(tail, chunk);
  };
  // UTF-8 のデコーダを流に持たせ、複数バイトの文字がチャンクの境目で化けないようにする
  child.stdout?.setEncoding("utf8");
  child.stderr?.setEncoding("utf8");
  child.stdout?.on("data", onOutput);
  child.stderr?.on("data", onOutput);

  let exitState: AppExit | null = null;
  const exited = new Promise<AppExit>((resolve) => {
    const settle = (code: number | null, signal: NodeJS.Signals | null): void => {
      exitState = { code, signal };
      resolve(exitState);
    };
    child.once("exit", settle);
    child.once("error", () => settle(null, null));
  });

  const pid = child.pid;
  if (pid === undefined) {
    await removeUserDataDir(userDataDir);
    throw new AppLaunchError("notFound", `アプリを起動できませんでした: ${options.exePath}`, tail);
  }

  const terminate = async (): Promise<{ userDataRemoved: boolean }> => {
    if (exitState === null) {
      await killTree(pid);
      await Promise.race([exited, new Promise((resolve) => setTimeout(resolve, KILL_WAIT_MS))]);
    }
    const userDataRemoved = await removeUserDataDir(userDataDir);
    return { userDataRemoved };
  };

  let version: CdpVersion;
  try {
    version = await waitForCdpEndpoint(port, {
      readyWithinMs: options.readyWithinMs,
      fetchVersion: fetchCdpVersion,
      hasExited: () => exitState !== null,
      outputTail: () => tail,
    });
  } catch (error) {
    await terminate();
    throw error;
  }

  return {
    pid,
    cdpEndpoint: `http://127.0.0.1:${port}`,
    userDataDir,
    runtimeVersion: version.browser,
    exited,
    get exit() {
      return exitState;
    },
    get outputTail() {
      return tail;
    },
    terminate,
  };
}
