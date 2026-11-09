// @vitest-environment node
// AppProbes の単体テスト。実アプリも実ブラウザも使わず、偽の Page と偽の window で確かめる。
// 偽の Page の evaluate は、関数はそのまま window の上で呼ぶ(ページ内で実行される形を模す)。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Page } from "@playwright/test";
import {
  bypassAppSuppression,
  installCloseEventCounter,
  IPC_OBSERVATION_MODE,
  observeIpc,
  placeNavigationMarker,
} from "./appProbes.ts";

type Handler = (arg: unknown) => void;

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
    async evaluate(target: (a?: unknown) => unknown, arg?: unknown) {
      return target(arg);
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

/** 指定の時間だけ待つ */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

type FakeWindow = {
  __TAURI_INTERNALS__: Record<string, unknown>;
  __bfE2eClose?: number;
  __bfE2eMarker?: string;
  __bfIpc?: { seq: number; entries: { id: number; cmd: string; done: number; ok: boolean | null }[] };
  addEventListener: ReturnType<typeof vi.fn>;
  [key: string]: unknown;
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

/**
 * IPC の観測の偽のページ内。postMessage(要求の送信)と callbacks(Map。応答の runCallback が引く)を持つ。
 * `complete(id)` は Tauri の runCallback と同じく callbacks.get(id) を引いて応答を模す。
 */
function makeIpcWindow(
  options: { callbacks?: unknown; webview?: unknown; post?: (data: unknown) => unknown } = {},
) {
  const callbacks = options.callbacks ?? new Map<number, unknown>();
  // post は元の postMessage の spy。設置の後の webview.postMessage は包み込みになるため、元の呼び出しは post で確かめる
  const post = vi.fn(options.post ?? ((data: unknown): unknown => ({ posted: data })));
  const webview = options.webview ?? { postMessage: post };
  const win = makeFakeWindow(async () => undefined);
  win.__TAURI_INTERNALS__.callbacks = callbacks;
  win.chrome = { webview };
  return {
    win,
    post,
    webview: webview as { postMessage: (data: unknown) => unknown },
    callbacks: callbacks as Map<number, unknown>,
    /** 要求を送る(Tauri の ipc.postMessage と同じく JSON 文字列) */
    send(cmd: string, callback: number, error: number) {
      return (win.chrome as { webview: { postMessage: (d: string) => unknown } }).webview.postMessage(
        JSON.stringify({ cmd, callback, error, options: { customProtocolIpcBlocked: true } }),
      );
    },
    /** 応答を返す(runCallback が callbacks.get を引く) */
    complete(id: number) {
      return (win.__TAURI_INTERNALS__.callbacks as Map<number, unknown>).get(id);
    },
  };
}

let previousWindow: unknown;

beforeEach(() => {
  previousWindow = (globalThis as { window?: unknown }).window;
});

afterEach(() => {
  (globalThis as { window?: unknown }).window = previousWindow;
});

function useWindow(win: object): void {
  (globalThis as { window?: unknown }).window = win;
}

describe("IPC_OBSERVATION_MODE", () => {
  it("既定の方式は postMessage", () => {
    expect(IPC_OBSERVATION_MODE).toBe("postMessage");
  });
});

describe("observeIpc(postMessage) の要求の記録", () => {
  it("要求の cmd と callback を順序どおりに記録し、commands で返す", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    ipc.send("load_source", 1, 2);
    ipc.send("set_document_dirty", 3, 4);

    expect(observer.mode).toBe("postMessage");
    expect(await observer.commands()).toEqual(["load_source", "set_document_dirty"]);
    expect(ipc.win.__bfIpc?.entries.map((e) => e.cmd)).toEqual(["load_source", "set_document_dirty"]);
  });

  it("cmd を持たないメッセージ・JSON でない文字列は記録せず、元の postMessage には渡す", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    ipc.webview.postMessage("not json");
    ipc.webview.postMessage({ other: 1 });

    expect(ipc.post).toHaveBeenCalledTimes(2);
    expect(await observer.commands()).toEqual([]);
    expect(ipc.win.__bfIpc?.entries).toEqual([]);
  });

  it("元の postMessage の引数と戻り値、呼び出しの this をそのまま保つ", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    await observeIpc(makeFakePage(), "postMessage");

    const data = JSON.stringify({ cmd: "run_batch", callback: 5, error: 6 });
    const returned = ipc.webview.postMessage(data);

    expect(returned).toEqual({ posted: data });
    expect(ipc.post.mock.calls).toEqual([[data]]);
    expect(ipc.post.mock.contexts[0]).toBe(ipc.webview);
  });
});

describe("observeIpc(postMessage) の完了の記録", () => {
  it("callbacks.get が記録した callback id で呼ばれると done を単調に増やし、元の結果をそのまま返す", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    await observeIpc(makeFakePage(), "postMessage");
    ipc.callbacks.set(7, "first");
    ipc.callbacks.set(9, "second");

    ipc.send("set_document_dirty", 7, 8);
    ipc.send("load_source", 9, 10);

    expect(ipc.complete(7)).toBe("first");
    const afterFirst = ipc.win.__bfIpc?.entries.map((e) => e.done) ?? [];
    expect(ipc.complete(9)).toBe("second");
    const afterSecond = ipc.win.__bfIpc?.entries.map((e) => e.done) ?? [];

    expect(afterFirst[0]).toBeGreaterThan(0);
    expect(afterFirst[1]).toBe(0);
    expect(afterSecond[1]).toBeGreaterThan(afterFirst[0] ?? 0);
  });

  it("error の id でも完了として記録する", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    await observeIpc(makeFakePage(), "postMessage");
    ipc.callbacks.set(20, "ok-value");
    ipc.callbacks.set(21, "err-value");

    ipc.send("run_batch", 20, 21);
    expect(ipc.complete(21)).toBe("err-value");

    expect(ipc.win.__bfIpc?.entries[0]?.done).toBeGreaterThan(0);
  });

  it("完了の成否を記録する: callback の id で引くと ok: true、error の id で引くと ok: false、未完了は null", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    await observeIpc(makeFakePage(), "postMessage");
    ipc.callbacks.set(30, "value");
    ipc.callbacks.set(31, "error-value");

    ipc.send("load_source", 30, 32);
    ipc.send("run_batch", 33, 31);
    expect(ipc.win.__bfIpc?.entries.map((e) => e.ok)).toEqual([null, null]);

    ipc.complete(30);
    expect(ipc.win.__bfIpc?.entries[0]?.ok).toBe(true);
    expect(ipc.win.__bfIpc?.entries[1]?.ok).toBeNull();

    ipc.complete(31);
    expect(ipc.win.__bfIpc?.entries[1]?.ok).toBe(false);
    expect(ipc.win.__bfIpc?.entries[0]?.ok).toBe(true);
  });

  it("成否は最初に引かれた応答で確定し、後の取得で書き換えない", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    await observeIpc(makeFakePage(), "postMessage");
    ipc.callbacks.set(40, "value");

    ipc.send("set_document_dirty", 40, 41);
    ipc.complete(41);
    ipc.complete(40);

    expect(ipc.win.__bfIpc?.entries[0]?.ok).toBe(false);
  });

  it("記録していない id の get は完了を付けず、結果をそのまま返す", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    await observeIpc(makeFakePage(), "postMessage");
    ipc.callbacks.set(99, "unrelated");

    ipc.send("load_source", 1, 2);
    expect(ipc.complete(99)).toBe("unrelated");

    expect(ipc.win.__bfIpc?.entries[0]?.done).toBe(0);
  });

  it("記録していない id・callbacks に無い id(404)の get は、元の結果 undefined をそのまま返す", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    await observeIpc(makeFakePage(), "postMessage");

    ipc.send("load_source", 1, 2);
    expect(ipc.complete(404)).toBeUndefined();
    expect(ipc.complete(99)).toBeUndefined();

    expect(ipc.win.__bfIpc?.entries[0]?.done).toBe(0);
  });

  it("記録した id でも、元の値が undefined・null・0 のときは値を変えずに返す", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    await observeIpc(makeFakePage(), "postMessage");
    ipc.callbacks.set(1, undefined);
    ipc.callbacks.set(2, null);
    ipc.callbacks.set(3, 0);

    ipc.send("undefined_result", 1, 10);
    ipc.send("null_result", 2, 11);
    ipc.send("zero_result", 3, 12);

    expect(ipc.complete(1)).toBeUndefined();
    expect(ipc.complete(2)).toBeNull();
    expect(ipc.complete(3)).toBe(0);
    const entries = ipc.win.__bfIpc?.entries ?? [];
    expect(entries.map((e) => e.done)).toEqual([expect.any(Number), expect.any(Number), expect.any(Number)]);
    expect(entries.every((e) => e.done > 0)).toBe(true);
  });
});

describe("observeIpc(postMessage) の元の postMessage が例外を投げる場合", () => {
  it("例外をそのまま伝え、要求は 1 件記録し、元の postMessage は 1 回呼ばれる", async () => {
    const ipc = makeIpcWindow({
      post: () => {
        throw new Error("boom");
      },
    });
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    expect(() => ipc.send("load_source", 1, 2)).toThrow("boom");
    expect(await observer.commands()).toEqual(["load_source"]);
    expect(ipc.post).toHaveBeenCalledTimes(1);
  });
});

describe("observeIpc(postMessage) の直列化", () => {
  it("page.evaluate に渡す関数の文字列だけで、偽の窓の下に設置でき、外側のスコープに依存しない", async () => {
    const ipc = makeIpcWindow();
    // 設置の evaluate だけ捕まえ、関数の文字列を取る(実行は下で、偽の窓の下で行う)
    const sources: string[] = [];
    const page = makeFakePage();
    (page as unknown as { evaluate: (target: unknown) => Promise<unknown> }).evaluate = async (target: unknown) => {
      sources.push(String(target));
      return "stub";
    };
    await observeIpc(page, "postMessage");

    expect(sources).toHaveLength(1);
    const src = sources[0] ?? "";
    expect(src).not.toContain("__name");

    // ページ内で動かす: window と Request を引数で渡し、モジュールのスコープは見えない
    class StubRequest {}
    const run = new Function("window", "Request", `return (${src})();`) as (w: unknown, r: unknown) => unknown;
    expect(run(ipc.win, StubRequest)).toBe("installed");

    ipc.callbacks.set(7, "value");
    ipc.send("load_source", 7, 8);
    expect(ipc.win.__bfIpc?.entries.map((e) => e.cmd)).toEqual(["load_source"]);
    expect(ipc.complete(7)).toBe("value");
    expect(ipc.win.__bfIpc?.entries[0]?.done).toBeGreaterThan(0);
    expect(ipc.post).toHaveBeenCalledTimes(1);

    expect(run(ipc.win, StubRequest)).toBe("already");
  });
});

describe("observeIpc(postMessage) の待ち受けの確定", () => {
  it("確定の後に完了した同じコマンドの応答で解決し、確定の前の完了・別のコマンドの完了では解決しない", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    // 確定の前に完了した応答(以前の要求)
    ipc.send("set_document_dirty", 1, 2);
    ipc.complete(1);

    const armed = await observer.armResponse("set_document_dirty");
    let settled = false;
    armed.completed.then(
      () => {
        settled = true;
      },
      () => undefined,
    );
    await sleep(5);
    expect(settled).toBe(false);

    // 別のコマンドの完了では解決しない
    ipc.send("load_source", 3, 4);
    ipc.complete(3);
    await sleep(5);
    expect(settled).toBe(false);

    // 確定の後の同じコマンドの完了で解決する
    ipc.send("set_document_dirty", 5, 6);
    ipc.complete(5);
    await expect(armed.completed).resolves.toEqual({ ok: true });
  });

  it("completed は完了した応答の成否で解決する: callback は { ok: true }、error は { ok: false }(エラーでも解決する)", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    const succeeded = await observer.armResponse("load_source");
    ipc.send("load_source", 50, 51);
    ipc.complete(50);
    await expect(succeeded.completed).resolves.toEqual({ ok: true });

    const failed = await observer.armResponse("start_session");
    ipc.send("start_session", 52, 53);
    ipc.complete(53);
    await expect(failed.completed).resolves.toEqual({ ok: false });
  });

  it("completed の成否は、確定の後に完了した同じコマンドの応答から取る(確定の前の成功は拾わない)", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    ipc.send("run_batch", 60, 61);
    ipc.complete(60);
    const armed = await observer.armResponse("run_batch");
    ipc.send("run_batch", 62, 63);
    ipc.complete(63);

    await expect(armed.completed).resolves.toEqual({ ok: false });
  });

  it("確定の直前に完了した応答(確定の時点の連番と同じ)を拾わない", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    ipc.send("set_document_dirty", 1, 2);
    ipc.complete(1);
    // 最後に起きた出来事が完了の記録。確定の連番はその完了の連番と等しい

    const armed = await observer.armResponse("set_document_dirty");
    let settled = false;
    armed.completed.then(
      () => {
        settled = true;
      },
      () => undefined,
    );
    await sleep(5);
    expect(settled).toBe(false);
  });

  it("確定の後の要求の完了は、要求が確定の前に送られていても解決する", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    ipc.send("run_batch", 1, 2);
    const armed = await observer.armResponse("run_batch");
    ipc.complete(1);

    await expect(armed.completed).resolves.toEqual({ ok: true });
  });

  it("completed は未処理の拒否にならない", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const page = makeFakePage();
    // 待ちが失敗する(時間切れ相当)ページを模す。completed の拒否を呼び出し側が待たなくても未処理にしないこと
    (page as unknown as { waitForFunction: () => Promise<never> }).waitForFunction = async () => {
      throw new Error("timeout");
    };
    const observer = await observeIpc(page, "postMessage");
    const unhandled = vi.fn();
    process.on("unhandledRejection", unhandled);
    try {
      await observer.armResponse("never");
      await sleep(5);
      expect(unhandled).not.toHaveBeenCalled();
    } finally {
      process.off("unhandledRejection", unhandled);
    }
  });
});

describe("observeIpc(postMessage) の設置の失敗", () => {
  it("callbacks が Map でなければ理由つきで失敗する", async () => {
    const ipc = makeIpcWindow({ callbacks: {} });
    useWindow(ipc.win);

    await expect(observeIpc(makeFakePage(), "postMessage")).rejects.toThrow(/callbacks/);
  });

  it("chrome.webview.postMessage が無ければ理由つきで失敗する", async () => {
    const ipc = makeIpcWindow({ webview: {} });
    useWindow(ipc.win);

    await expect(observeIpc(makeFakePage(), "postMessage")).rejects.toThrow(/postMessage/);
  });

  it("chrome が無ければ理由つきで失敗する", async () => {
    const win = makeFakeWindow(async () => undefined);
    win.__TAURI_INTERNALS__.callbacks = new Map();
    useWindow(win);

    await expect(observeIpc(makeFakePage(), "postMessage")).rejects.toThrow(/postMessage/);
  });
});

describe("observeIpc(postMessage) の二重の設置", () => {
  it("2 回設置しても包み込みは 1 回だけで、要求は 1 回だけ記録され、元の postMessage も 1 回だけ呼ばれる", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const page = makeFakePage();
    const first = await observeIpc(page, "postMessage");
    const postAfterFirst = ipc.webview.postMessage;
    const getAfterFirst = ipc.callbacks.get;
    const recordAfterFirst = ipc.win.__bfIpc;
    const second = await observeIpc(page, "postMessage");

    // 2 回目は何も包み直さず、記録も置き換えない(同じ関数・同じ記録のまま)
    expect(ipc.webview.postMessage).toBe(postAfterFirst);
    expect(ipc.callbacks.get).toBe(getAfterFirst);
    expect(ipc.win.__bfIpc).toBe(recordAfterFirst);

    ipc.send("load_source", 1, 2);
    ipc.complete(1);

    expect(await first.commands()).toEqual(["load_source"]);
    expect(await second.commands()).toEqual(["load_source"]);
    expect(ipc.post).toHaveBeenCalledTimes(1);
    expect(ipc.win.__bfIpc?.entries[0]?.done).toBeGreaterThan(0);
  });

  it("2 回目の設置で応答の get を包み直さない(完了は 1 回だけ付く)", async () => {
    const ipc = makeIpcWindow();
    useWindow(ipc.win);
    const page = makeFakePage();
    await observeIpc(page, "postMessage");
    const doneAfterFirst = ipc.win.__bfIpc?.seq ?? 0;
    await observeIpc(page, "postMessage");

    ipc.send("load_source", 1, 2);
    ipc.complete(1);
    ipc.complete(1);

    expect(ipc.win.__bfIpc?.seq).toBe(doneAfterFirst + 2);
    expect(ipc.win.__bfIpc?.entries[0]?.done).toBe(doneAfterFirst + 2);
  });
});

describe("observeIpc(postMessage) の fetch の保険", () => {
  it("ipc.localhost 宛ての fetch を記録し、応答の決まった時点で完了にする", async () => {
    let resolveFetch: (value: unknown) => void = () => undefined;
    const fetchFn = vi.fn(
      () =>
        new Promise((resolve) => {
          resolveFetch = resolve;
        }),
    );
    const ipc = makeIpcWindow();
    (ipc.win as { fetch?: unknown }).fetch = fetchFn;
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    const pending = (ipc.win as unknown as { fetch: (u: string) => Promise<unknown> }).fetch(
      "http://ipc.localhost/load%5Fsource?x=1",
    );
    expect(await observer.commands()).toEqual(["load_source"]);
    expect(fetchFn).toHaveBeenCalledWith("http://ipc.localhost/load%5Fsource?x=1", undefined);

    resolveFetch("response");
    await expect(pending).resolves.toBe("response");
    expect(ipc.win.__bfIpc?.entries[0]?.done).toBeGreaterThan(0);
  });

  it("ipc.localhost 以外の fetch は記録せず、そのまま返す", async () => {
    const fetchFn = vi.fn(async () => "other");
    const ipc = makeIpcWindow();
    (ipc.win as { fetch?: unknown }).fetch = fetchFn;
    useWindow(ipc.win);
    const observer = await observeIpc(makeFakePage(), "postMessage");

    const result = await (ipc.win as unknown as { fetch: (u: string) => Promise<unknown> }).fetch(
      "http://tauri.localhost/app.js",
    );

    expect(result).toBe("other");
    expect(await observer.commands()).toEqual([]);
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
