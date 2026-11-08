// OS の操作の窓口。PowerShell の補助スクリプト(windowsHost.ps1)を固定の引数で起動し、
// 標準出力の 1 行の JSON を検証して型に写す。解析(parseHostResult)と引数の組み立て(buildHostArgs)は
// 純粋関数で、単体テストは偽の出力だけで行う。Windows 専用(powershell.exe を使う)。
import { execFile } from "node:child_process";
import { WINDOWS_HOST_SCRIPT_PATH } from "./paths.ts";

export type HostAction = "runtime" | "window" | "close" | "keys" | "menu";

export type HostResult =
  | { action: "runtime"; webView2Version: string | null }
  | { action: "window"; found: true; title: string }
  | { action: "close"; delivered: true; reason: null }
  | { action: "keys"; delivered: true; reason: null }
  | { action: "menu"; menuOpen: boolean };

/** 届けられなかった(ウィンドウが無い・前面化できない)結果や形の違う出力。理由を含む */
export class HostActionError extends Error {
  readonly action: HostAction;
  readonly reason: string;

  constructor(action: HostAction, reason: string) {
    super(`OS の操作 ${action} に失敗しました: ${reason}`);
    this.name = "HostActionError";
    this.action = action;
    this.reason = reason;
  }
}

/** 補助スクリプトへの対象(pid と SendKeys の記法)。runtime は何も要らない */
export type HostTarget = { processId?: number; keys?: string };

/** 結果の型を動作の名前で絞り込む */
export type HostResultOf<A extends HostAction> = Extract<HostResult, { action: A }>;

const PROCESS_TIMEOUT_MS = 30_000;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function nonEmptyLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line !== "");
}

function malformed(action: HostAction, detail: string): HostActionError {
  return new HostActionError(action, `補助スクリプトの出力の形が違います(${detail})`);
}

/**
 * 補助スクリプトの標準出力を解析する。出力は結果の 1 行の JSON だけを期待する。
 * `error` の行と、届けられなかった結果(`delivered: false`、`found: false`)は HostActionError にする。
 */
export function parseHostResult(action: HostAction, stdout: string): HostResult {
  const lines = nonEmptyLines(stdout);
  const [line, ...rest] = lines;
  if (line === undefined || rest.length > 0) {
    throw malformed(action, `1 行の JSON ではなく ${lines.length} 行でした`);
  }

  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch {
    throw malformed(action, "JSON として読めません");
  }
  if (!isRecord(value)) {
    throw malformed(action, "オブジェクトではありません");
  }
  if (typeof value.error === "string") {
    throw new HostActionError(action, value.error);
  }
  if (value.action !== action) {
    throw malformed(action, `動作の名前が ${action} ではありません`);
  }

  switch (action) {
    case "runtime": {
      const version = value.webView2Version;
      if (version === null) {
        return { action, webView2Version: null };
      }
      if (typeof version === "string" && version !== "") {
        return { action, webView2Version: version };
      }
      throw malformed(action, "webView2Version の型が違います");
    }
    case "window": {
      if (value.found === false) {
        throw new HostActionError(action, "主ウィンドウが見つかりません");
      }
      if (value.found === true && typeof value.title === "string") {
        return { action, found: true, title: value.title };
      }
      throw malformed(action, "found または title の型が違います");
    }
    case "close":
    case "keys": {
      if (value.delivered === false) {
        const reason = typeof value.reason === "string" && value.reason !== "" ? value.reason : "届けられませんでした";
        throw new HostActionError(action, reason);
      }
      if (value.delivered === true) {
        return { action, delivered: true, reason: null };
      }
      throw malformed(action, "delivered の型が違います");
    }
    case "menu": {
      if (typeof value.menuOpen === "boolean") {
        return { action, menuOpen: value.menuOpen };
      }
      throw malformed(action, "menuOpen の型が違います");
    }
  }
}

/** 補助スクリプトへの固定の引数。利用者の入力は数値の pid と、シナリオが持つキーの記法だけ */
export function buildHostArgs(action: HostAction, target: HostTarget): string[] {
  const args = [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    WINDOWS_HOST_SCRIPT_PATH,
    "-Action",
    action,
  ];
  if (action !== "runtime") {
    const pid = target.processId;
    if (pid === undefined || !Number.isInteger(pid) || pid <= 0) {
      throw new HostActionError(action, "対象のプロセス ID が正の整数ではありません");
    }
    args.push("-ProcessId", String(pid));
  }
  if (action === "keys") {
    if (target.keys === undefined || target.keys === "") {
      throw new HostActionError(action, "キーの記法が指定されていません");
    }
    args.push("-Keys", target.keys);
  }
  return args;
}

/** 出力が 1 行の `{ "error": "..." }` ならその理由を返す(失敗時の理由の取り出し) */
function errorLineReason(stdout: string): string | null {
  const [line, ...rest] = nonEmptyLines(stdout);
  if (line === undefined || rest.length > 0) {
    return null;
  }
  try {
    const value: unknown = JSON.parse(line);
    return isRecord(value) && typeof value.error === "string" ? value.error : null;
  } catch {
    return null;
  }
}

type ExecOutcome = { stdout: string; stderr: string; failure: Error | null };

function runPowerShell(args: string[]): Promise<ExecOutcome> {
  return new Promise((resolve) => {
    execFile(
      "powershell.exe",
      args,
      { encoding: "utf8", windowsHide: true, timeout: PROCESS_TIMEOUT_MS },
      (error, stdout, stderr) => {
        resolve({ stdout: String(stdout), stderr: String(stderr), failure: error });
      },
    );
  });
}

/**
 * 補助スクリプトを 1 回実行し、結果を型に写す。
 * 終了コードが 0 でなければ、出力の `error` の行の理由を優先し、無ければ標準エラーの内容を理由にする。
 */
export async function runHostAction<A extends HostAction>(action: A, target: HostTarget = {}): Promise<HostResultOf<A>> {
  const outcome = await runPowerShell(buildHostArgs(action, target));
  if (outcome.failure !== null) {
    const reason = errorLineReason(outcome.stdout);
    const detail = reason ?? (outcome.stderr.trim() !== "" ? outcome.stderr.trim() : outcome.failure.message);
    throw new HostActionError(action, `補助スクリプトが失敗しました: ${detail}`);
  }
  return parseHostResult(action, outcome.stdout) as HostResultOf<A>;
}

/** WebView2 の版(導入されていなければ null)。準備の確認と注釈に使う */
export async function readWebView2Version(): Promise<string | null> {
  const result = await runHostAction("runtime");
  return result.webView2Version;
}

/** 1 つの実アプリのプロセス(pid)に束ねた OS の窓口 */
export interface BoundWindowsHost {
  /** 主ウィンドウのタイトル。見つからなければ HostActionError(4.1) */
  windowTitle(): Promise<string>;
  /** 主ウィンドウへ OS の閉じる要求(SC_CLOSE)を送る。送れなければ HostActionError(5.5) */
  requestClose(): Promise<void>;
  /** 前面化してから OS のキー入力(SendKeys の記法)を送る。失敗なら HostActionError(6.5) */
  sendKeys(keys: string): Promise<void>;
  /** アプリのプロセスツリーに属する既定のコンテキストメニューが開いているか(6.3) */
  contextMenuOpen(): Promise<boolean>;
}

export function bindWindowsHost(pid: number): BoundWindowsHost {
  return {
    async windowTitle() {
      const result = await runHostAction("window", { processId: pid });
      return result.title;
    },
    async requestClose() {
      await runHostAction("close", { processId: pid });
    },
    async sendKeys(keys: string) {
      await runHostAction("keys", { processId: pid, keys });
    },
    async contextMenuOpen() {
      const result = await runHostAction("menu", { processId: pid });
      return result.menuOpen;
    },
  };
}
