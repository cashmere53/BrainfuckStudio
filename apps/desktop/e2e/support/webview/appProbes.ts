// webview の E2E の実アプリのページの観測の補助(AppProbes)。製品のコードは変えず、ページの外から観測する。
// - IPC の観測(方式は IPC_OBSERVATION_MODE で選ぶ。名前は既存の観測との互換のため postMessage のまま)
// - close-requested の受信の計数、ページ内の目印によるナビゲーションの検出、抑止の無効化(陽性対照)
// ページ内で実行する関数(page.evaluate に渡すもの)は自己完結させ、モジュールのスコープの値を参照しない。
//
// IPC の観測の根拠(実アプリで確認済み): 製品の CSP は connect-src 'self' ipc: http://ipc.localhost を許すため、
// Tauri は fetch 方式の IPC(http://ipc.localhost/<cmd>)で要求する。この経路は window.fetch の包み込みで観測する。
// CSP が拒否されたときに Tauri が切り替える window.chrome.webview.postMessage の経路も、同じ記録に残す
// (要求は postMessage の包み込み、完了は callbacks の get の包み込みで観測する)。
import { randomUUID } from "node:crypto";
import type { Page } from "@playwright/test";
import { CLOSE_REQUESTED_EVENT } from "../../../src/ipc/backend.ts";

/** IPC の観測の方式。現在は 1 つ(実アプリで、要求と完了が fetch 方式の記録に入ることを確認済み) */
export type IpcObservationMode = "postMessage";

/** フィクスチャが使う方式 */
export const IPC_OBSERVATION_MODE: IpcObservationMode = "postMessage";

export interface IpcObserver {
  readonly mode: IpcObservationMode;
  /** 観測した要求のコマンド名(順序どおり) */
  commands(): Promise<readonly string[]>;
  /**
   * 待ち受けを確定してから返る。completed は、確定以降に完了したそのコマンドの応答で解決し、
   * 応答の成否(callback なら ok: true、error なら ok: false)を返す(エラーでも解決する)
   */
  armResponse(command: string): Promise<{ readonly completed: Promise<{ readonly ok: boolean }> }>;
}

/** ページ内に残す 1 件の要求。done は完了した時点の連番(0 は未完了)。ok は完了の成否(未完了は null) */
type IpcEntry = { id: number; cmd: string; done: number; ok: boolean | null };

/** callback または error の id に対応する要求と、その id が成功(callback)か失敗(error)かの区分 */
type IpcCallbackRef = { entry: IpcEntry; ok: boolean };

/** ページ内に残す観測の記録。seq は単調な連番(要求と完了の両方で進む) */
type IpcRecord = {
  seq: number;
  entries: IpcEntry[];
  byCallback: Map<number, IpcCallbackRef>;
};

/** ページ内の窓口(型だけに使う。ページ内の関数の本体は値を外から受け取らない) */
type ProbeWindow = {
  __TAURI_INTERNALS__?: {
    invoke(cmd: string, args?: unknown): Promise<unknown>;
    transformCallback(callback: (value: unknown) => void): number;
  };
  __bfIpc?: IpcRecord;
  __bfE2eClose?: number;
  __bfE2eCloseInstalled?: boolean;
  __bfE2eMarker?: string;
};

/**
 * ページ内の観測器(postMessage の包み込みと callbacks.get の包み込み)を設置する。page.evaluate に渡すため自己完結させる。
 * - 内部 API(callbacks の Map、chrome.webview.postMessage)が無ければ理由つきで失敗する(Tauri の変更の早期検出)
 * - 二重に設置しない(設置済みなら何もしない)
 * - 要求: postMessage に渡る JSON から cmd・callback・error を取って記録する。元の呼び出しと戻り値はそのまま
 * - 完了: runCallback が記録した callback または error の id を callbacks.get で引いた時点で done を付ける。
 *   成否は引かれた id の種類で決める(callback の id なら ok: true、error の id なら ok: false)
 * - window.fetch の ipc.localhost 宛ても記録する(製品の CSP が connect-src で許すため、通常の IPC はこちらで記録される)
 */
function installIpcRecorder(): string {
  const w = window as unknown as {
    __bfIpc?: IpcRecord;
    __TAURI_INTERNALS__?: { callbacks?: unknown };
    chrome?: { webview?: { postMessage?: unknown } };
    fetch?: (input: unknown, init?: unknown) => Promise<unknown>;
  };
  const callbacks = w.__TAURI_INTERNALS__?.callbacks;
  if (!(callbacks instanceof Map)) {
    throw new Error("IPC の観測の設置に失敗: __TAURI_INTERNALS__.callbacks が Map でない(Tauri の内部 API が変わった可能性)");
  }
  const webview = w.chrome?.webview;
  if (webview === undefined || typeof webview.postMessage !== "function") {
    throw new Error("IPC の観測の設置に失敗: window.chrome.webview.postMessage が無い(Tauri の内部 API が変わった可能性)");
  }
  if (w.__bfIpc !== undefined) {
    return "already";
  }

  const rec: IpcRecord = { seq: 0, entries: [], byCallback: new Map() };
  w.__bfIpc = rec;

  // 要求: 元の postMessage を呼び、戻り値をそのまま返す。記録の失敗はアプリに影響させない
  const originalPost = webview.postMessage as (this: unknown, ...args: unknown[]) => unknown;
  (webview as { postMessage: unknown }).postMessage = function (this: unknown, ...args: unknown[]): unknown {
    try {
      const data = args[0];
      const msg: unknown = typeof data === "string" ? JSON.parse(data) : data;
      if (msg !== null && typeof msg === "object" && typeof (msg as { cmd?: unknown }).cmd === "string") {
        const m = msg as { cmd: string; callback?: unknown; error?: unknown };
        const entry: IpcEntry = { id: ++rec.seq, cmd: m.cmd, done: 0, ok: null };
        rec.entries.push(entry);
        if (typeof m.callback === "number") {
          rec.byCallback.set(m.callback, { entry, ok: true });
        }
        if (typeof m.error === "number") {
          rec.byCallback.set(m.error, { entry, ok: false });
        }
      }
    } catch {
      // 観測の失敗は握りつぶす(要求の本体には影響させない)
    }
    return originalPost.apply(this, args);
  };

  // 応答: runCallback(id) は callbacks.get(id) を引く。記録した id なら完了を付け、元の結果をそのまま返す
  const originalGet = callbacks.get as (this: unknown, key: unknown) => unknown;
  (callbacks as { get: unknown }).get = function (this: unknown, key: unknown): unknown {
    const hit = typeof key === "number" ? rec.byCallback.get(key) : undefined;
    if (hit !== undefined && hit.entry.done === 0) {
      hit.entry.done = ++rec.seq;
      hit.entry.ok = hit.ok;
    }
    return originalGet.call(this, key);
  };

  // 保険: fetch 方式の ipc.localhost 宛て。応答の Promise が決まった時点を完了とする
  const originalFetch = w.fetch;
  if (typeof originalFetch === "function") {
    w.fetch = function (this: unknown, input: unknown, init?: unknown): Promise<unknown> {
      const url = typeof input === "string" ? input : input instanceof Request ? input.url : String(input);
      const match = /^https?:\/\/ipc\.localhost\/([^?#]*)/.exec(url);
      if (match === null) {
        return originalFetch.call(this, input, init);
      }
      let cmd = match[1] ?? "";
      try {
        cmd = decodeURIComponent(cmd);
      } catch {
        // 壊れたエンコードは生の名前のまま記録する
      }
      const entry: IpcEntry = { id: ++rec.seq, cmd, done: 0, ok: null };
      rec.entries.push(entry);
      const finish = (ok: boolean): void => {
        if (entry.done === 0) {
          entry.done = ++rec.seq;
          entry.ok = ok;
        }
      };
      return originalFetch.call(this, input, init).then(
        (response) => {
          // 応答が決まった時点。HTTP の成否(ok が false)を失敗とする
          finish((response as { ok?: unknown } | null)?.ok !== false);
          return response;
        },
        (error: unknown) => {
          finish(false);
          throw error;
        },
      );
    };
  }
  return "installed";
}

/**
 * 観測を設置する。接続直後に呼ぶ(設置の前の要求は観測の対象にならない)。
 * 設置できなければ、理由つきで失敗する(ページの読み込みは変えない)。
 */
export async function observeIpc(page: Page, mode: IpcObservationMode): Promise<IpcObserver> {
  if (mode !== "postMessage") {
    throw new Error(`未知の IPC の観測の方式: ${String(mode)}`);
  }
  await page.evaluate(installIpcRecorder);
  return {
    mode: "postMessage",
    async commands() {
      return page.evaluate(
        () => ((window as unknown as ProbeWindow).__bfIpc?.entries ?? []).map((entry) => entry.cmd),
      );
    },
    async armResponse(command) {
      // 確定の時点の連番。これより後に完了した同じコマンドの応答だけを数える(確定前の完了は拾わない)
      const armedAt = await page.evaluate(() => (window as unknown as ProbeWindow).__bfIpc?.seq ?? 0);
      // 完了の確定の後に、その要求の成否を読む(エラーの応答でも解決し、呼び出し側が ok を確かめられる)
      const completed = page
        .waitForFunction(
          ([name, after]) =>
            ((window as unknown as ProbeWindow).__bfIpc?.entries ?? []).some(
              (entry) => entry.cmd === name && entry.done > after,
            ),
          [command, armedAt] as const,
        )
        .then(() =>
          page.evaluate(
            ([name, after]) => {
              const entry = ((window as unknown as ProbeWindow).__bfIpc?.entries ?? []).find(
                (e) => e.cmd === name && e.done > after,
              );
              return { ok: entry?.ok === true };
            },
            [command, armedAt] as const,
          ),
        );
      // 呼び出し側が待たなくても未処理の拒否にしない
      completed.catch(() => undefined);
      return { completed };
    },
  };
}

export interface CloseEventCounter {
  count(): Promise<number>;
}

/** ページ内で close-requested をもう 1 つ購読する。page.evaluate に渡すため自己完結させる */
function installCloseCounterInPage(eventName: string): Promise<void> {
  const w = window as unknown as ProbeWindow;
  if (w.__bfE2eCloseInstalled === true) {
    return Promise.resolve();
  }
  const internals = w.__TAURI_INTERNALS__;
  if (internals === undefined) {
    return Promise.reject(new Error("__TAURI_INTERNALS__ が無い"));
  }
  w.__bfE2eCloseInstalled = true;
  w.__bfE2eClose = 0;
  const handlerId = internals.transformCallback(() => {
    w.__bfE2eClose = (w.__bfE2eClose ?? 0) + 1;
  });
  return internals
    .invoke("plugin:event|listen", { event: eventName, target: { kind: "Any" }, handler: handlerId })
    .then(() => undefined);
}

/** ページ内で close-requested を購読し、受信回数を数える(イベント名はフロントの定数を使う) */
export async function installCloseEventCounter(page: Page): Promise<CloseEventCounter> {
  await page.evaluate(installCloseCounterInPage, CLOSE_REQUESTED_EVENT);
  return {
    count: () => page.evaluate(() => (window as unknown as ProbeWindow).__bfE2eClose ?? 0),
  };
}

export interface NavigationMarker {
  /** 目印が残っていれば true(再読み込みや遷移で消える) */
  isIntact(): Promise<boolean>;
  /** 設置の後のメインフレームの遷移の回数 */
  readonly navigations: number;
}

/** ページに一意の目印を置き、メインフレームの遷移を数える */
export async function placeNavigationMarker(page: Page): Promise<NavigationMarker> {
  const token = randomUUID();
  await page.evaluate((value) => {
    (window as unknown as ProbeWindow).__bfE2eMarker = value;
  }, token);

  const main = page.mainFrame();
  let navigations = 0;
  page.on("framenavigated", (frame) => {
    if (frame === main) {
      navigations += 1;
    }
  });

  return {
    isIntact: () =>
      page.evaluate(
        (value) => (window as unknown as ProbeWindow).__bfE2eMarker === value,
        token,
      ),
    get navigations() {
      return navigations;
    },
  };
}

/**
 * 陽性対照用: window の capture で指定の種類のイベントの伝播を止め、アプリの抑止(window の bubble の既定動作の抑止)を無効にする。
 * アプリのリスナーより前に動くため、アプリの抑止は呼ばれず、既定動作が残る。
 */
export async function bypassAppSuppression(
  page: Page,
  kinds: readonly ("keydown" | "contextmenu")[],
): Promise<void> {
  await page.evaluate((list) => {
    for (const kind of list) {
      window.addEventListener(
        kind,
        (event) => {
          event.stopImmediatePropagation();
        },
        { capture: true },
      );
    }
  }, [...kinds]);
}
