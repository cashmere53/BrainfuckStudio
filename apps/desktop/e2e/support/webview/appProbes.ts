// webview の E2E の実アプリのページの観測の補助(AppProbes)。製品のコードは変えず、ページの外から観測する。
// - IPC の観測(2 つの方式。既定は network、切り替え先は invoke。方式は IPC_OBSERVATION_MODE の 1 つの定数で選ぶ)
// - close-requested の受信の計数、ページ内の目印によるナビゲーションの検出、抑止の無効化(陽性対照)
// ページ内で実行する関数(page.evaluate に渡すもの)は自己完結させ、モジュールのスコープの値を参照しない。
import { randomUUID } from "node:crypto";
import type { Page } from "@playwright/test";
import { CLOSE_REQUESTED_EVENT } from "../../../src/ipc/backend.ts";
import { IPC_URL_PREFIX } from "./paths.ts";

/**
 * IPC の URL からコマンド名を取り出す(純粋関数)。
 * IPC の接頭辞で始まらない URL・名前が空の URL・壊れたエンコードは null。クエリと断片は名前に含めない。
 */
export function ipcCommandFromUrl(url: string): string | null {
  if (!url.startsWith(IPC_URL_PREFIX)) {
    return null;
  }
  const rest = url.slice(IPC_URL_PREFIX.length).split(/[?#]/)[0] ?? "";
  if (rest === "") {
    return null;
  }
  try {
    const name = decodeURIComponent(rest);
    return name === "" ? null : name;
  } catch {
    return null;
  }
}

/** network: CDP 越しのページのネットワーク要求(request / requestfinished)。invoke: ページ内の __TAURI_INTERNALS__.invoke の包み込み(切り替え先) */
export type IpcObservationMode = "network" | "invoke";

/** フィクスチャが使う方式。2.3 の実アプリでの確認で確定する(初期値は "network") */
export const IPC_OBSERVATION_MODE: IpcObservationMode = "network";

export interface IpcObserver {
  readonly mode: IpcObservationMode;
  /** 観測した要求のコマンド名(順序どおり) */
  commands(): Promise<readonly string[]>;
  /** 待ち受けを確定してから返る。completed は、確定以降に完了したそのコマンドの応答で解決する */
  armResponse(command: string): Promise<{ readonly completed: Promise<void> }>;
}

/** invoke の方式がページ内に残す記録 */
export type IpcCallLog = { calls: string[]; completed: Record<string, number> };

/** ページ内の窓口(型だけに使う。ページ内の関数の本体は値を外から受け取らない) */
type ProbeWindow = {
  __TAURI_INTERNALS__?: {
    invoke(cmd: string, args?: unknown): Promise<unknown>;
    transformCallback(callback: (value: unknown) => void): number;
  };
  __bfE2eIpc?: IpcCallLog;
  __bfE2eClose?: number;
  __bfE2eCloseInstalled?: boolean;
  __bfE2eMarker?: string;
};

/**
 * invoke の方式の本体。internals.invoke を包み、コマンド名の呼び出しと完了を log に記録する。
 * 元の結果と例外はそのまま返す。page.evaluate に文字列で渡されるため、自己完結させ、名前付きの内側の関数を使わない。
 * 二重に適用された場合は何もしない。
 */
export function installInvokeRecorder(
  internals: { invoke: (cmd: string, ...rest: unknown[]) => Promise<unknown> },
  log: IpcCallLog,
): void {
  const original = internals.invoke;
  if ((original as unknown as { __bfRecorder?: boolean }).__bfRecorder === true) {
    return;
  }
  const wrapped = function (cmd: string, ...rest: unknown[]): Promise<unknown> {
    log.calls.push(cmd);
    let result: Promise<unknown>;
    try {
      result = Promise.resolve(original.call(internals, cmd, ...rest));
    } catch (error) {
      log.completed[cmd] = (log.completed[cmd] ?? 0) + 1;
      throw error;
    }
    return result.then(
      (value) => {
        log.completed[cmd] = (log.completed[cmd] ?? 0) + 1;
        return value;
      },
      (error: unknown) => {
        log.completed[cmd] = (log.completed[cmd] ?? 0) + 1;
        throw error;
      },
    );
  };
  (wrapped as unknown as { __bfRecorder?: boolean }).__bfRecorder = true;
  internals.invoke = wrapped;
}

/** 待ち受けの確定の時点の完了の回数を、その後に超えたら解決する待ちの 1 件 */
type ResponseWaiter = { command: string; baseline: number; resolve: () => void };

/** ネットワーク要求の方式: 要求の URL からコマンド名を取り、完了の回数を数える */
function observeNetwork(page: Page): IpcObserver {
  const commands: string[] = [];
  const finished = new Map<string, number>();
  let waiters: ResponseWaiter[] = [];

  page.on("request", (request) => {
    const name = ipcCommandFromUrl(request.url());
    if (name !== null) {
      commands.push(name);
    }
  });
  page.on("requestfinished", (request) => {
    const name = ipcCommandFromUrl(request.url());
    if (name === null) {
      return;
    }
    const count = (finished.get(name) ?? 0) + 1;
    finished.set(name, count);
    waiters = waiters.filter((waiter) => {
      if (waiter.command === name && count > waiter.baseline) {
        waiter.resolve();
        return false;
      }
      return true;
    });
  });

  return {
    mode: "network",
    async commands() {
      return [...commands];
    },
    async armResponse(command) {
      const baseline = finished.get(command) ?? 0;
      let resolve: () => void = () => undefined;
      const completed = new Promise<void>((done) => {
        resolve = done;
      });
      waiters.push({ command, baseline, resolve });
      return { completed };
    },
  };
}

/** invoke の方式: 接続後にページ内の invoke を包み、記録をページから読む */
async function observeInvoke(page: Page): Promise<IpcObserver> {
  // 関数の本体を文字列にしてページで評価する(installInvokeRecorder は外の値を参照しないため安全)。
  const source = `(${installInvokeRecorder.toString()})(window.__TAURI_INTERNALS__, window.__bfE2eIpc = window.__bfE2eIpc ?? { calls: [], completed: {} })`;
  await page.evaluate(source);

  return {
    mode: "invoke",
    async commands() {
      return page.evaluate(() => (window as unknown as ProbeWindow).__bfE2eIpc?.calls ?? []);
    },
    async armResponse(command) {
      const baseline = await page.evaluate(
        (name) => (window as unknown as ProbeWindow).__bfE2eIpc?.completed[name] ?? 0,
        command,
      );
      const completed = page
        .waitForFunction(
          ([name, count]) =>
            ((window as unknown as ProbeWindow).__bfE2eIpc?.completed[name] ?? 0) > count,
          [command, baseline] as const,
        )
        .then(() => undefined);
      return { completed };
    },
  };
}

/** 方式に応じて IPC の観測を始める。接続直後に呼ぶ(以前の要求は観測の対象にならない) */
export async function observeIpc(page: Page, mode: IpcObservationMode): Promise<IpcObserver> {
  return mode === "network" ? observeNetwork(page) : observeInvoke(page);
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
