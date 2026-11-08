// ブリッジ(crates/brainfuck-e2e-bridge)のプロセスを管理するクライアント。
// 要求に連番の id を付けて並行に送り、応答を id で対応づける。ダイアログの問い合わせには onDialog の結果で答える。
// プロセスが終了したら応答待ちの要求をすべて BridgeExitedError で失敗させ、標準エラーの末尾を保持する。
// 転送路(BridgeTransport)を受け取る部分は PassThrough などの偽物で単体テストできる。
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { PassThrough, type Readable, type Writable } from "node:stream";
import { createInterface } from "node:readline";
import {
  encodeLine,
  parseLine,
  type DialogKind,
  type DialogResult,
  type FromBridge,
} from "./protocol.ts";

export type InvokeOutcome = { ok: true; value: unknown } | { ok: false; error: unknown };
export type BridgeExit = { code: number | null; signal: NodeJS.Signals | null };

/** 子プロセスの 3 本の流れと終了の Promise。spawnBridge が作り、テストは偽物を渡す */
export interface BridgeTransport {
  stdin: Writable;
  stdout: Readable;
  stderr: Readable;
  exited: Promise<BridgeExit>;
  kill(): void;
}

export interface BridgeClient {
  /** 応答を InvokeOutcome で返す。異常終了後や不正な行の後は BridgeExitedError などで reject する */
  invoke(cmd: string, payload: unknown): Promise<InvokeOutcome>;
  /** 終了していなければ null */
  readonly exit: BridgeExit | null;
  /** 調査用に標準エラーの末尾を保持する */
  readonly stderrTail: string;
  /** 標準入力を閉じ、猶予の後に強制終了する */
  close(options?: { graceMs?: number }): Promise<void>;
}

export class BridgeExitedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "BridgeExitedError";
  }
}

/** 起動確認(e2e:ping)の既定の待ち時間(ミリ秒) */
export const DEFAULT_READY_WITHIN_MS = 30_000;
/** close で標準入力を閉じてから強制終了するまでの既定の猶予(ミリ秒) */
export const DEFAULT_CLOSE_GRACE_MS = 2_000;
/** 保持する標準エラーの末尾の長さ(文字数) */
export const STDERR_TAIL_LIMIT = 4_000;

/** 実行ファイルが無いときの案内。パスと準備のコマンドを示す */
export function missingBridgeMessage(binPath: string): string {
  return `ブリッジの実行ファイルが見つかりません: ${binPath}\n先に npm run e2e:prepare を実行して、ブリッジをビルドしてください。`;
}

/** 起動確認が上限時間までに返らないときの案内。ウイルス対策ソフトと CARGO_TARGET_DIR の対処を示す */
export function readyTimeoutMessage(readyWithinMs: number): string {
  return [
    `ブリッジが ${readyWithinMs} ミリ秒以内に起動確認(e2e:ping)へ応答しませんでした。`,
    "ウイルス対策ソフトが新しい実行ファイルを検査・停止している可能性があります。",
    "ウイルス対策ソフトの除外に追加するか一時停止し、CARGO_TARGET_DIR を新しいフォルダにして npm run e2e:prepare をやり直してください。",
    "待ち時間は環境変数 E2E_BRIDGE_READY_MS(ミリ秒)で変えられます。",
  ].join("\n");
}

/** E2E_BRIDGE_READY_MS を読む。空や未設定は既定値、正の整数でなければ失敗する */
export function readyWithinMsFromEnv(env: NodeJS.ProcessEnv): number {
  const raw = env.E2E_BRIDGE_READY_MS;
  if (raw === undefined || raw === "") return DEFAULT_READY_WITHIN_MS;
  if (!/^\d+$/.test(raw) || Number(raw) <= 0) {
    throw new Error(`E2E_BRIDGE_READY_MS は正の整数(ミリ秒)で指定してください: "${raw}"`);
  }
  return Number(raw);
}

function exitDescription(exit: BridgeExit): string {
  return `終了コード ${exit.code ?? "なし"}、シグナル ${exit.signal ?? "なし"}`;
}

/** 実行ファイルを起動し、転送路を返す。無ければ案内つきの Error を投げる */
export function spawnBridge(binPath: string): BridgeTransport {
  if (!existsSync(binPath)) {
    throw new Error(missingBridgeMessage(binPath));
  }
  const child = spawn(binPath, [], { stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
  // 標準エラーは PassThrough に中継し、起動の失敗(ENOENT など)もその末尾に載せる
  const stderr = new PassThrough();
  child.stderr.pipe(stderr);
  const exited = new Promise<BridgeExit>((resolve) => {
    child.once("error", (error) => {
      stderr.write(`起動に失敗しました: ${error.message}\n`);
      resolve({ code: null, signal: null });
    });
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });
  return {
    stdin: child.stdin,
    stdout: child.stdout,
    stderr,
    exited,
    kill: () => {
      child.kill();
    },
  };
}

/** 転送路の上でクライアントを作る。起動確認は waitForReady が行う */
export function createBridgeClient(
  transport: BridgeTransport,
  onDialog: (kind: DialogKind) => Promise<DialogResult>,
): BridgeClient {
  type Pending = { resolve: (outcome: InvokeOutcome) => void; reject: (error: Error) => void };
  const pending = new Map<number, Pending>();
  let nextId = 1;
  let exitState: BridgeExit | null = null;
  let stderrText = "";
  // 不正な行や想定外の応答など、続けられない失敗。以後の要求はこれで失敗させる
  let fatal: Error | null = null;

  // 終了後に書くと EPIPE などの error が出る。失敗は exited の結果として報告するので、ここでは握りつぶす
  transport.stdin.on("error", () => {});

  transport.stderr.on("data", (chunk: Buffer | string) => {
    stderrText = (stderrText + chunk.toString()).slice(-STDERR_TAIL_LIMIT);
  });

  function writeLine(message: Parameters<typeof encodeLine>[0]): void {
    transport.stdin.write(encodeLine(message));
  }

  function failAll(error: Error): void {
    for (const entry of pending.values()) entry.reject(error);
    pending.clear();
  }

  function fail(error: Error): void {
    fatal ??= error;
    failAll(error);
    transport.kill();
  }

  function handle(message: FromBridge): void {
    if (message.type === "dialog") {
      onDialog(message.kind).then(
        (result) => writeLine({ type: "dialogResult", id: message.id, result }),
        (error: unknown) => fail(error instanceof Error ? error : new Error(String(error))),
      );
      return;
    }
    const entry = pending.get(message.id);
    if (entry === undefined) {
      fail(new Error(`想定外の応答です(id ${message.id} の要求はありません)`));
      return;
    }
    pending.delete(message.id);
    entry.resolve(
      message.ok ? { ok: true, value: message.value } : { ok: false, error: message.error },
    );
  }

  const lines = createInterface({ input: transport.stdout, crlfDelay: Infinity });
  lines.on("line", (line) => {
    if (fatal !== null) return;
    try {
      handle(parseLine(line));
    } catch (error) {
      fail(error instanceof Error ? error : new Error(String(error)));
    }
  });

  transport.exited.then((exit) => {
    exitState = exit;
    const tail = stderrText.trim();
    const detail = tail === "" ? "" : `\n標準エラーの末尾:\n${tail}`;
    failAll(new BridgeExitedError(`ブリッジが終了しました(${exitDescription(exit)})。${detail}`));
  });

  return {
    invoke(cmd, payload) {
      if (fatal !== null) return Promise.reject(fatal);
      if (exitState !== null) {
        return Promise.reject(
          new BridgeExitedError(`ブリッジは既に終了しています(${exitDescription(exitState)})`),
        );
      }
      const id = nextId;
      nextId += 1;
      return new Promise<InvokeOutcome>((resolve, reject) => {
        pending.set(id, { resolve, reject });
        writeLine({ type: "request", id, cmd, payload });
      });
    },

    get exit() {
      return exitState;
    },

    get stderrTail() {
      return stderrText;
    },

    async close(options = {}) {
      const graceMs = options.graceMs ?? DEFAULT_CLOSE_GRACE_MS;
      transport.stdin.end();
      let timer: ReturnType<typeof setTimeout> | undefined;
      const graceful = await Promise.race([
        transport.exited.then(() => true),
        new Promise<false>((resolve) => {
          timer = setTimeout(() => resolve(false), graceMs);
        }),
      ]);
      clearTimeout(timer);
      if (!graceful) transport.kill();
      await transport.exited;
    },
  };
}

/** 起動確認(e2e:ping)が pong で返るまで待つ。上限を超えたら案内つきの Error で失敗する */
export async function waitForReady(client: BridgeClient, readyWithinMs: number): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(readyTimeoutMessage(readyWithinMs))), readyWithinMs);
  });
  try {
    const outcome = await Promise.race([client.invoke("e2e:ping", null), timeout]);
    if (!outcome.ok) {
      throw new Error(`起動確認(e2e:ping)が失敗しました: ${JSON.stringify(outcome.error)}`);
    }
    if (outcome.value !== "pong") {
      throw new Error(`起動確認(e2e:ping)の応答が想定と違います: ${JSON.stringify(outcome.value)}`);
    }
  } finally {
    clearTimeout(timer);
  }
}

/** 起動して起動確認を待つ。失敗したときはプロセスを閉じてから失敗を伝える */
export async function startBridge(
  binPath: string,
  onDialog: (kind: DialogKind) => Promise<DialogResult>,
  options: {
    readyWithinMs: number;
    /** 転送路を作る関数。既定は実際の子プロセスを起動する spawnBridge(テストの差し替え用) */
    spawn?: (binPath: string) => BridgeTransport;
  },
): Promise<BridgeClient> {
  const transport = (options.spawn ?? spawnBridge)(binPath);
  const client = createBridgeClient(transport, onDialog);
  try {
    await waitForReady(client, options.readyWithinMs);
    return client;
  } catch (error) {
    await client.close();
    throw error;
  }
}
