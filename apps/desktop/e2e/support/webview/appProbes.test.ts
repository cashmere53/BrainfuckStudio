// @vitest-environment node
// AppProbes の単体テスト。実アプリも実ブラウザも使わず、偽の Page と偽の window で確かめる。
// 偽の Page の evaluate は、関数はそのまま window の上で呼び、文字列は評価する(ページ内で実行される形を模す)。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Page } from "@playwright/test";
import {
  bypassAppSuppression,
  installCloseEventCounter,
  installInvokeRecorder,
  IPC_OBSERVATION_MODE,
  ipcCommandFromUrl,
  observeIpc,
  placeNavigationMarker,
  type IpcCallLog,
} from "./appProbes.ts";

type Handler = (arg: unknown) => void;
type Invoke = (cmd: string, ...rest: unknown[]) => Promise<unknown>;

/** 偽の Page。on で登録したイベントを emit で発火し、evaluate・waitForFunction はグローバルの window で動かす */
function makeFakePage(mainFrame: object = { name: "main" }) {
  const handlers = new Map<string, Handler[]>();
  const waitPollMs = 1;
  const page = {
    mainFrame: () => mainFrame,
    on(event: string, handler: Handler) {
      handlers.set(event, [...(handlers.get(event) ?? []), handler]);
      return page;
    },
    emit(event: string, arg: unknown) {
      for (const handler of handlers.get(event) ?? []) {
        handler(arg);
      }
    },
    async evaluate(target: unknown, arg?: unknown) {
      if (typeof target === "string") {
        return (0, eval)(target) as unknown;
      }
      return (target as (a: unknown) => unknown)(arg);
    },
    async waitForFunction(target: (a: unknown) => unknown, arg: unknown) {
      for (let attempt = 0; attempt < 2000; attempt += 1) {
        if (target(arg)) {
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, waitPollMs));
      }
      throw new Error("waitForFunction: 条件が成立しなかった");
    },
  };
  return page as unknown as Page & {
    emit(event: string, arg: unknown): void;
  };
}

/** 偽の要求。url() だけを持つ */
function fakeRequest(url: string) {
  return { url: () => url };
}

type FakeWindow = {
  __TAURI_INTERNALS__: Record<string, unknown>;
  __bfE2eIpc?: IpcCallLog;
  __bfE2eClose?: number;
  __bfE2eMarker?: string;
  addEventListener: ReturnType<typeof vi.fn>;
};

function makeFakeWindow(invoke: (cmd: string, args?: unknown) => Promise<unknown>): FakeWindow {
  return {
    __TAURI_INTERNALS__: {
      invoke,
      transformCallback: vi.fn(),
    },
    addEventListener: vi.fn(),
  };
}

let previousWindow: unknown;

beforeEach(() => {
  previousWindow = (globalThis as { window?: unknown }).window;
});

afterEach(() => {
  (globalThis as { window?: unknown }).window = previousWindow;
});

function useWindow(win: FakeWindow): void {
  (globalThis as { window?: unknown }).window = win;
}

describe("IPC_OBSERVATION_MODE", () => {
  it("初期値はネットワーク要求の方式", () => {
    expect(IPC_OBSERVATION_MODE).toBe("network");
  });
});

describe("ipcCommandFromUrl", () => {
  it("IPC の URL からコマンド名を取り出す", () => {
    expect(ipcCommandFromUrl("http://ipc.localhost/load_source")).toBe("load_source");
    expect(ipcCommandFromUrl("http://ipc.localhost/set_document_dirty")).toBe("set_document_dirty");
  });

  it("エンコードされた名前を復号する", () => {
    expect(ipcCommandFromUrl("http://ipc.localhost/plugin%3Aevent%7Clisten")).toBe(
      "plugin:event|listen",
    );
  });

  it("クエリと断片を名前に含めない", () => {
    expect(ipcCommandFromUrl("http://ipc.localhost/run_batch?x=1")).toBe("run_batch");
    expect(ipcCommandFromUrl("http://ipc.localhost/run_batch#frag")).toBe("run_batch");
  });

  it("IPC 以外の URL・名前が空の URL・壊れたエンコードは null", () => {
    expect(ipcCommandFromUrl("http://tauri.localhost/index.html")).toBeNull();
    expect(ipcCommandFromUrl("https://example.com/load_source")).toBeNull();
    expect(ipcCommandFromUrl("http://ipc.localhost/")).toBeNull();
    expect(ipcCommandFromUrl("http://ipc.localhost/%E0%A4%A")).toBeNull();
  });
});

describe("installInvokeRecorder", () => {
  it("呼び出しの名前を記録し、成功した結果をそのまま返す", async () => {
    const result = { ok: true };
    const original = vi.fn<Invoke>(async () =>result);
    const internals: { invoke: Invoke } = { invoke: original };
    const log: IpcCallLog = { calls: [], completed: {} };

    installInvokeRecorder(internals, log);
    const returned = await internals.invoke("run_batch", { args: {} });

    expect(returned).toBe(result);
    expect(log.calls).toEqual(["run_batch"]);
    expect(log.completed).toEqual({ run_batch: 1 });
    expect(original).toHaveBeenCalledWith("run_batch", { args: {} });
  });

  it("拒否された例外をそのまま投げ直し、完了の回数を数える", async () => {
    const failure = new Error("boom");
    const internals: { invoke: Invoke } = {
      invoke: vi.fn<Invoke>(async () =>Promise.reject(failure)),
    };
    const log: IpcCallLog = { calls: [], completed: {} };

    installInvokeRecorder(internals, log);
    const caught = await internals.invoke("load_source").catch((error: unknown) => error);

    expect(caught).toBe(failure);
    expect(log.calls).toEqual(["load_source"]);
    expect(log.completed).toEqual({ load_source: 1 });
  });

  it("同期の例外もそのまま投げ、完了として数える", () => {
    const failure = new Error("sync");
    const internals: { invoke: Invoke } = {
      invoke: vi.fn<Invoke>(() => {
        throw failure;
      }),
    };
    const log: IpcCallLog = { calls: [], completed: {} };

    installInvokeRecorder(internals, log);

    expect(() => internals.invoke("start_session")).toThrow(failure);
    expect(log.completed).toEqual({ start_session: 1 });
  });

  it("同じコマンドの呼び出しと完了を別々に数える", async () => {
    const internals: { invoke: Invoke } = { invoke: vi.fn<Invoke>(async () =>1) };
    const log: IpcCallLog = { calls: [], completed: {} };

    installInvokeRecorder(internals, log);
    await internals.invoke("set_document_dirty", { args: { dirty: true } });
    await internals.invoke("set_document_dirty", { args: { dirty: false } });

    expect(log.calls).toEqual(["set_document_dirty", "set_document_dirty"]);
    expect(log.completed).toEqual({ set_document_dirty: 2 });
  });

  it("二重に適用しても包み込みは 1 回だけ", async () => {
    const internals: { invoke: Invoke } = { invoke: vi.fn<Invoke>(async () =>1) };
    const log: IpcCallLog = { calls: [], completed: {} };

    installInvokeRecorder(internals, log);
    installInvokeRecorder(internals, log);
    await internals.invoke("load_source");

    expect(log.calls).toEqual(["load_source"]);
    expect(log.completed).toEqual({ load_source: 1 });
  });
});

describe("observeIpc(network)", () => {
  it("IPC の要求の URL からコマンド名を順序どおりに集め、IPC 以外は含めない", async () => {
    const page = makeFakePage();
    const observer = await observeIpc(page, "network");

    page.emit("request", fakeRequest("http://ipc.localhost/load_source"));
    page.emit("request", fakeRequest("http://tauri.localhost/app.js"));
    page.emit("request", fakeRequest("http://ipc.localhost/run_batch"));

    expect(observer.mode).toBe("network");
    expect(await observer.commands()).toEqual(["load_source", "run_batch"]);
  });

  it("待ち受けの確定の後に完了した同じコマンドの応答で completed が解決する", async () => {
    const page = makeFakePage();
    const observer = await observeIpc(page, "network");
    page.emit("requestfinished", fakeRequest("http://ipc.localhost/set_document_dirty"));

    const armed = await observer.armResponse("set_document_dirty");
    let settled = false;
    void armed.completed.then(() => {
      settled = true;
    });
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(settled).toBe(false);

    page.emit("requestfinished", fakeRequest("http://ipc.localhost/load_source"));
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(settled).toBe(false);

    page.emit("requestfinished", fakeRequest("http://ipc.localhost/set_document_dirty"));
    await expect(armed.completed).resolves.toBeUndefined();
  });
});

describe("observeIpc(invoke)", () => {
  it("接続後に包んだ invoke の呼び出しを観測し、待ち受けの後の完了で解決する", async () => {
    const win = makeFakeWindow(async () => "done");
    useWindow(win);
    const page = makeFakePage();

    const observer = await observeIpc(page, "invoke");
    expect(observer.mode).toBe("invoke");

    const armed = await observer.armResponse("set_document_dirty");
    await (win.__TAURI_INTERNALS__.invoke as (cmd: string) => Promise<unknown>)("set_document_dirty");
    await expect(armed.completed).resolves.toBeUndefined();

    expect(await observer.commands()).toEqual(["set_document_dirty"]);
    expect(win.__bfE2eIpc?.completed).toEqual({ set_document_dirty: 1 });
  });
});

describe("installCloseEventCounter", () => {
  it("close-requested を購読し、受信のたびに数える", async () => {
    const callbacks = new Map<number, (event: unknown) => void>();
    let nextId = 1;
    const invoke = vi.fn(async (cmd: string, args?: unknown) => {
      if (cmd === "plugin:event|listen") {
        return (args as { handler?: number } | undefined)?.handler;
      }
      return undefined;
    });
    const win = makeFakeWindow(invoke);
    win.__TAURI_INTERNALS__.transformCallback = (callback: (event: unknown) => void) => {
      const id = nextId;
      nextId += 1;
      callbacks.set(id, callback);
      return id;
    };
    useWindow(win);
    const page = makeFakePage();

    const counter = await installCloseEventCounter(page);
    expect(await counter.count()).toBe(0);

    expect(invoke).toHaveBeenCalledWith(
      "plugin:event|listen",
      expect.objectContaining({ event: "close-requested" }),
    );
    const handlerId = (invoke.mock.calls[0]?.[1] as { handler: number }).handler;
    callbacks.get(handlerId)?.({});
    callbacks.get(handlerId)?.({});

    expect(await counter.count()).toBe(2);
  });
});

describe("placeNavigationMarker", () => {
  it("目印が残っていれば intact、消えれば not intact。メインフレームの遷移だけを数える", async () => {
    const win = makeFakeWindow(async () => undefined);
    useWindow(win);
    const main = { name: "main" };
    const child = { name: "child" };
    const page = makeFakePage(main);

    const marker = await placeNavigationMarker(page);
    expect(await marker.isIntact()).toBe(true);
    expect(marker.navigations).toBe(0);

    page.emit("framenavigated", child);
    expect(marker.navigations).toBe(0);

    page.emit("framenavigated", main);
    expect(marker.navigations).toBe(1);

    win.__bfE2eMarker = "reloaded";
    expect(await marker.isIntact()).toBe(false);
  });
});

describe("bypassAppSuppression", () => {
  it("指定の種類について window の capture で伝播を止める既定の抑止の無効化を入れる", async () => {
    const win = makeFakeWindow(async () => undefined);
    useWindow(win);
    const page = makeFakePage();

    await bypassAppSuppression(page, ["keydown", "contextmenu"]);

    const calls = win.addEventListener.mock.calls as [string, (e: unknown) => void, unknown][];
    expect(calls.map((call) => call[0])).toEqual(["keydown", "contextmenu"]);
    for (const [, , options] of calls) {
      expect(options).toEqual(expect.objectContaining({ capture: true }));
    }

    const event = { stopImmediatePropagation: vi.fn() };
    calls[0]?.[1](event);
    expect(event.stopImmediatePropagation).toHaveBeenCalledTimes(1);
  });
});

/** 指定の時間だけ待つ */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** 応答を後から制御できる偽の invoke。呼ばれた順に pending へ積み、resolve で応答を返す */
function controlledInvoke() {
  const pending: { cmd: string; resolve: (value: unknown) => void }[] = [];
  const invoke = (cmd: string) =>
    new Promise<unknown>((resolve) => {
      pending.push({ cmd, resolve });
    });
  return { invoke, pending };
}

describe("ipcCommandFromUrl の接頭辞の位置", () => {
  it("接頭辞が URL の途中や別のホストにあるだけなら null", () => {
    expect(ipcCommandFromUrl("https://example.com/?u=http://ipc.localhost/load_source")).toBeNull();
    expect(ipcCommandFromUrl("http://ipc.localhost.evil/x")).toBeNull();
    expect(ipcCommandFromUrl("https://ipc.localhost/x")).toBeNull();
  });
});

describe("observeIpc(invoke) の待ち受けの確定", () => {
  it("確定の前に完了した同じコマンドの応答・別のコマンドの完了では解決せず、確定の後の同じコマンドの完了で解決する", async () => {
    const control = controlledInvoke();
    const win = makeFakeWindow(control.invoke);
    useWindow(win);
    const page = makeFakePage();
    const observer = await observeIpc(page, "invoke");
    const call = win.__TAURI_INTERNALS__.invoke as (cmd: string) => Promise<unknown>;

    // 確定の前に 1 回完了させる(以前の応答)
    const before = call("set_document_dirty");
    control.pending[0]?.resolve(1);
    await before;

    const armed = await observer.armResponse("set_document_dirty");
    let settled = false;
    void armed.completed.then(() => {
      settled = true;
    });
    await sleep(5);
    expect(settled).toBe(false);

    // 別のコマンドの完了では解決しない
    const other = call("load_source");
    control.pending[1]?.resolve(2);
    await other;
    await sleep(5);
    expect(settled).toBe(false);

    // 確定の後の同じコマンドの完了で解決する
    const after = call("set_document_dirty");
    control.pending[2]?.resolve(3);
    await after;
    await expect(armed.completed).resolves.toBeUndefined();
  });
});

describe("observeIpc(network) の別コマンドの取り違え", () => {
  it("別のコマンドの完了が複数あっても解決せず、確定の後の同じコマンドの完了で初めて解決する", async () => {
    const page = makeFakePage();
    const observer = await observeIpc(page, "network");
    page.emit("requestfinished", fakeRequest("http://ipc.localhost/set_document_dirty"));

    const armed = await observer.armResponse("set_document_dirty");
    let settled = false;
    void armed.completed.then(() => {
      settled = true;
    });

    page.emit("requestfinished", fakeRequest("http://ipc.localhost/load_source"));
    page.emit("requestfinished", fakeRequest("http://ipc.localhost/load_source"));
    await sleep(5);
    expect(settled).toBe(false);

    page.emit("requestfinished", fakeRequest("http://ipc.localhost/set_document_dirty"));
    await expect(armed.completed).resolves.toBeUndefined();
  });
});

describe("installInvokeRecorder の this", () => {
  it("元の invoke を internals を this にして呼ぶ", async () => {
    const seenThis: unknown[] = [];
    const internals = {
      invoke: function (this: unknown, cmd: string) {
        seenThis.push(this);
        return Promise.resolve(cmd);
      },
    };
    const log: IpcCallLog = { calls: [], completed: {} };

    installInvokeRecorder(internals as unknown as { invoke: Invoke }, log);
    await internals.invoke("load_source");

    expect(seenThis).toHaveLength(1);
    expect(seenThis[0]).toBe(internals);
  });
});

describe("installCloseEventCounter の購読", () => {
  it("2 回呼んでも購読は 1 回だけで、listen の引数は target の Any を含み、受信は 1 回ずつ数える", async () => {
    const callbacks = new Map<number, (event: unknown) => void>();
    let nextId = 1;
    const invoke = vi.fn(async (cmd: string, args?: unknown) =>
      cmd === "plugin:event|listen" ? (args as { handler?: number } | undefined)?.handler : undefined,
    );
    const win = makeFakeWindow(invoke);
    win.__TAURI_INTERNALS__.transformCallback = (callback: (event: unknown) => void) => {
      const id = nextId;
      nextId += 1;
      callbacks.set(id, callback);
      return id;
    };
    useWindow(win);
    const page = makeFakePage();

    const first = await installCloseEventCounter(page);
    const second = await installCloseEventCounter(page);

    const listenCalls = invoke.mock.calls.filter(([cmd]) => cmd === "plugin:event|listen");
    expect(listenCalls).toHaveLength(1);
    expect(listenCalls[0]?.[1]).toEqual({
      event: "close-requested",
      target: { kind: "Any" },
      handler: expect.any(Number),
    });

    const handler = (listenCalls[0]?.[1] as { handler: number }).handler;
    callbacks.get(handler)?.({});
    expect(await first.count()).toBe(1);
    expect(await second.count()).toBe(1);
  });
});

describe("placeNavigationMarker の目印", () => {
  it("置くたびに別の値になり、後から置いた目印だけが残る", async () => {
    const win = makeFakeWindow(async () => undefined);
    useWindow(win);
    const page = makeFakePage();

    const first = await placeNavigationMarker(page);
    const firstValue = win.__bfE2eMarker;
    const second = await placeNavigationMarker(page);
    const secondValue = win.__bfE2eMarker;

    expect(firstValue).toEqual(expect.any(String));
    expect(secondValue).not.toBe(firstValue);
    expect(await second.isIntact()).toBe(true);
    expect(await first.isIntact()).toBe(false);
  });
});
