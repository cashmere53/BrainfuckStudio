// テストごとのフィクスチャ。テストごとにダイアログの待ち行列とブリッジを作り、ページに転送関数を公開して
// シムを 1 つの初期化スクリプトとして登録してから画面を開く。後始末で異常を失敗にする(要件 1.6、2.4〜2.8)。
// 純粋な部分(転送関数の記録・閉じる要求・後始末の判定・終了状態の読み取り)は Playwright に依存せず、
// fixtures.test.ts で偽のクライアントを使って単体テストする。
import { readFileSync } from "node:fs";
import { test as base, expect, type Page } from "@playwright/test";
import { DebuggerPage } from "./pages/DebuggerPage.ts";
import {
  type BridgeClient,
  type BridgeExit,
  BridgeExitedError,
  type InvokeOutcome,
  readyWithinMsFromEnv,
  startBridge,
} from "./bridge/bridgeClient.ts";
import { createDialogQueue, type DialogQueue } from "./bridge/dialogQueue.ts";
import type { DialogKind } from "./bridge/protocol.ts";
import { bridgeBinaryPath, SHIM_BUNDLE_PATH } from "./paths.ts";

/** 呼び出しの記録の 1 件。応答が返るまでは outcome が null */
export type RecordedCall = {
  readonly cmd: string;
  readonly payload: unknown;
  readonly outcome: InvokeOutcome | null;
  readonly startedAt: number;
  readonly endedAt: number | null;
};

export type WindowCloseResult = "eventEmitted" | "closedWithoutConfirmation";

export interface E2eBackend {
  /** テストごとのダイアログの待ち行列(指定しなかった問い合わせは後始末で失敗にする) */
  readonly dialogs: DialogQueue;
  /** 記録された呼び出し。cmd を渡すとそのコマンドだけを返す。制御コマンド(`e2e:` で始まるもの)は含めない */
  calls(cmd?: string): readonly RecordedCall[];
  /** 閉じる要求。確認が必要なときだけ閉じる要求のイベントを発火し、不要なら確認なしで閉じた扱いを返す */
  requestWindowClose(): Promise<WindowCloseResult>;
  /** ページからの呼び出しを中継する(debuggerPage フィクスチャが exposeFunction に渡す。テストからは使わない) */
  forward(cmd: string, payload: unknown): Promise<InvokeOutcome>;
}

type Fixtures = {
  backend: E2eBackend;
  debuggerPage: DebuggerPage;
};

/** 記録の作成中だけ使う可変の形(外へは RecordedCall として渡す) */
type MutableCall = { cmd: string; payload: unknown; outcome: InvokeOutcome | null; startedAt: number; endedAt: number | null };

/** 転送関数の記録に必要な最小のクライアント(BridgeClient の invoke だけ) */
export type InvokeClient = Pick<BridgeClient, "invoke">;

/** 転送関数の記録の組み立て。calls と forward を持つ */
export interface BackendRecorder {
  calls(cmd?: string): readonly RecordedCall[];
  forward(cmd: string, payload: unknown): Promise<InvokeOutcome>;
}

/** 制御コマンドの接頭辞。呼び出しの記録には含めない */
export const CONTROL_PREFIX = "e2e:";

/** 閉じる要求の確認の制御コマンド。ブリッジの文書状態(未保存の旗)を読む */
export const CLOSE_REQUESTED_CMD = "e2e:close_requested";

/** ブリッジが途中で終了したときに、ページへ返す失敗の文言(既存の文言。変えない) */
export const BRIDGE_EXITED_MESSAGE = "bridge exited";

/** 後始末で失敗の理由に使う、ブリッジの標準エラーの添付名 */
export const STDERR_ATTACHMENT = "bridge-stderr";

/** 後始末で終了を読む前に待つ上限(ミリ秒)。終了イベントが処理される前に読んで見逃さないため */
export const EXIT_SETTLE_MS = 50;

const EXIT_POLL_MS = 5;

/** 転送関数の記録を作る。制御コマンドは中継するが記録しない。異常終了は既存の失敗の文言で返す */
export function createBackendRecorder(client: InvokeClient): BackendRecorder {
  const records: RecordedCall[] = [];

  return {
    calls(cmd) {
      return records.filter((call) => cmd === undefined || call.cmd === cmd);
    },

    async forward(cmd, payload) {
      const call: MutableCall = { cmd, payload, outcome: null, startedAt: Date.now(), endedAt: null };
      if (!cmd.startsWith(CONTROL_PREFIX)) records.push(call);
      let outcome: InvokeOutcome;
      try {
        outcome = await client.invoke(cmd, payload);
      } catch (error) {
        // 中継の失敗はページへ封筒の失敗として返す(成功の封筒にはしない)
        const message =
          error instanceof BridgeExitedError
            ? BRIDGE_EXITED_MESSAGE
            : String(error instanceof Error ? error.message : error);
        outcome = { ok: false, error: message };
      }
      call.outcome = outcome;
      call.endedAt = Date.now();
      return outcome;
    },
  };
}

/**
 * 閉じる要求。確認が必要(文書が未保存)なら emit で閉じる要求のイベントを発火し、不要なら発火せず
 * 確認なしで閉じた扱いを返す。確認の問い合わせが失敗したら例外にする。
 */
export async function requestCloseWith(
  client: InvokeClient,
  emit: () => Promise<void>,
): Promise<WindowCloseResult> {
  const outcome = await client.invoke(CLOSE_REQUESTED_CMD, null);
  if (!outcome.ok) {
    throw new Error(`閉じる要求の確認が失敗しました: ${JSON.stringify(outcome.error)}`);
  }
  const required = (outcome.value as { confirmationRequired?: unknown }).confirmationRequired;
  if (required !== true) {
    return "closedWithoutConfirmation";
  }
  await emit();
  return "eventEmitted";
}

/**
 * ブリッジの終了状態を読む。exit は終了イベントの処理の後に入るため、待ちの間に終了が届けば返し、
 * 上限まで届かなければ null を返す(後始末の直前にブリッジが終了した競合を見逃さないため)。
 */
export async function settleExit(client: Pick<BridgeClient, "exit">, waitMs: number = EXIT_SETTLE_MS): Promise<BridgeExit | null> {
  const deadline = Date.now() + waitMs;
  while (client.exit === null && Date.now() < deadline) {
    await new Promise<void>((resolve) => setTimeout(resolve, EXIT_POLL_MS));
  }
  return client.exit;
}

/** 後始末の判定の入力 */
export type TeardownInput = {
  readonly exit: BridgeExit | null;
  readonly stderrTail: string;
  readonly unexpected: readonly DialogKind[];
  readonly remaining: number;
};

/** 後始末の判定の結果。stderrToAttach は問題があり、標準エラーに内容があるときだけ入る */
export type TeardownReport = {
  readonly problems: readonly string[];
  readonly stderrToAttach: string | null;
};

/** 後始末の問題を並べる。テスト中の異常終了、予期しないダイアログ、使われなかったダイアログが対象 */
export function teardownReport(input: TeardownInput): TeardownReport {
  const problems: string[] = [];
  const tail = input.stderrTail.trim();
  if (input.exit !== null) {
    const description = `終了コード ${input.exit.code ?? "なし"}、シグナル ${input.exit.signal ?? "なし"}`;
    problems.push(
      tail === ""
        ? `テスト中にブリッジが異常終了しました(${description})。`
        : `テスト中にブリッジが異常終了しました(${description})。標準エラーの末尾:\n${tail}`,
    );
  }
  if (input.unexpected.length > 0) {
    problems.push(`予期しないダイアログが開かれました: ${input.unexpected.join(", ")}`);
  }
  if (input.remaining > 0) {
    problems.push(`指定したが使われなかったダイアログが ${input.remaining} 件あります`);
  }
  return {
    problems,
    stderrToAttach: problems.length > 0 && tail !== "" ? input.stderrTail : null,
  };
}

/** 閉じる要求のイベントを発火する操作口(シムが window に公開する) */
async function emitCloseRequested(page: Page): Promise<void> {
  await page.evaluate(() => window.__bfE2e.emitCloseRequested());
}

export const test = base.extend<Fixtures>({
  backend: async ({ page }, use, testInfo) => {
    const dialogs = createDialogQueue();
    const client: BridgeClient = await startBridge(
      bridgeBinaryPath(process.env),
      (kind) => dialogs.take(kind),
      { readyWithinMs: readyWithinMsFromEnv(process.env) },
    );
    const recorder = createBackendRecorder(client);

    const backend: E2eBackend = {
      dialogs,
      calls: (cmd) => recorder.calls(cmd),
      requestWindowClose: () => requestCloseWith(client, () => emitCloseRequested(page)),
      forward: (cmd, payload) => recorder.forward(cmd, payload),
    };

    await use(backend);

    // 後始末: ブリッジの異常終了(テスト中に既に終了していた)、予期しないダイアログ、使われなかったダイアログを失敗にする。
    // 終了は閉じる前に読む(閉じる操作の強制終了を異常終了と数えないため)
    const exit = await settleExit(client);
    await client.close();
    const report = teardownReport({
      exit,
      stderrTail: client.stderrTail,
      unexpected: dialogs.unexpected,
      remaining: dialogs.remaining,
    });
    if (report.problems.length > 0) {
      if (report.stderrToAttach !== null) {
        await testInfo.attach(STDERR_ATTACHMENT, { body: report.stderrToAttach, contentType: "text/plain" });
      }
      throw new Error(report.problems.join("\n"));
    }
  },

  debuggerPage: async ({ page, backend }, use) => {
    // 転送関数は初期化スクリプトより前に公開する(シムが mockIPC を組む時点で存在させるため)
    await page.exposeFunction("__bfE2eInvoke", (cmd: string, payload: unknown) => backend.forward(cmd, payload));
    // シムは 1 回の addInitScript で登録する(複数の初期化スクリプトの順序に依存しない)
    await page.addInitScript({ content: readFileSync(SHIM_BUNDLE_PATH, "utf8") });
    const debuggerPage = new DebuggerPage(page);
    await debuggerPage.goto();
    await use(debuggerPage);
  },
});

export { expect };
