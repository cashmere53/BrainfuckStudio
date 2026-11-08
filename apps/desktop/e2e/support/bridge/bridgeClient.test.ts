// @vitest-environment node
import { PassThrough } from "node:stream";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  BridgeExitedError,
  DEFAULT_CLOSE_GRACE_MS,
  STDERR_TAIL_LIMIT,
  createBridgeClient,
  readyWithinMsFromEnv,
  startBridge,
  waitForReady,
  type BridgeExit,
  type BridgeTransport,
} from "./bridgeClient.ts";
import { ProtocolError, type DialogKind, type DialogResult, type ToBridge } from "./protocol.ts";

const PATH_A: DialogResult = { kind: "path", path: "/work/a.bf" };
const PATH_B: DialogResult = { kind: "path", path: "/work/b.bf" };

type FakeTransport = {
  readonly transport: BridgeTransport;
  /** 標準入力へ書かれた行(Node → ブリッジ)を解釈した結果 */
  readonly written: ToBridge[];
  /** 標準出力へ 1 行を書く(ブリッジ → Node) */
  send(line: object | string): void;
  /** 標準エラーへ書く(プロセスが出す診断の模擬) */
  writeStderr(text: string): void;
  /** プロセスの終了を模擬する */
  finish(exit: BridgeExit): void;
  readonly kill: ReturnType<typeof vi.fn>;
  readonly stdinEnded: () => boolean;
};

/**
 * PassThrough の組で偽の転送路を作る。kill は終了を模擬する。
 * exitOnStdinEnd が真なら、標準入力を閉じたときにプロセスが終了したものとして振る舞う。
 */
function fakeTransport(options: { exitOnStdinEnd?: boolean } = {}): FakeTransport {
  const stdin = new PassThrough();
  const stdout = new PassThrough();
  const stderr = new PassThrough();
  const written: ToBridge[] = [];
  let ended = false;
  stdin.on("data", (chunk: Buffer) => {
    for (const line of chunk.toString("utf8").split("\n")) {
      if (line !== "") written.push(JSON.parse(line) as ToBridge);
    }
  });
  let resolveExit!: (exit: BridgeExit) => void;
  const exited = new Promise<BridgeExit>((resolve) => {
    resolveExit = resolve;
  });
  stdin.on("end", () => {
    ended = true;
    if (options.exitOnStdinEnd === true) resolveExit({ code: 0, signal: null });
  });
  const kill = vi.fn(() => resolveExit({ code: null, signal: "SIGKILL" }));
  const transport: BridgeTransport = { stdin, stdout, stderr, exited, kill };
  return {
    transport,
    written,
    send(line) {
      stdout.write(typeof line === "string" ? `${line}\n` : `${JSON.stringify(line)}\n`);
    },
    writeStderr(text) {
      stderr.write(text);
    },
    finish(exit) {
      resolveExit(exit);
    },
    kill,
    stdinEnded: () => ended,
  };
}

/** Promise が拒否されたときの理由を返す。解決したらテストを失敗させる */
async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => {
      throw new Error("拒否されるはずの Promise が解決しました");
    },
    (error: unknown) => error,
  );
}

afterEach(() => {
  vi.useRealTimers();
});

describe("BridgeClient: 要求と応答", () => {
  it("並行に送った要求を id で対応づけ、応答の到着順に解決する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const first = client.invoke("get_snapshot", null);
    const second = client.invoke("pause", null);

    await vi.waitFor(() => expect(fake.written).toHaveLength(2));
    expect(fake.written[0]).toEqual({ type: "request", id: 1, cmd: "get_snapshot", payload: null });
    expect(fake.written[1]).toEqual({ type: "request", id: 2, cmd: "pause", payload: null });

    // 後に送った要求の応答を先に返す
    fake.send({ type: "response", id: 2, ok: true, value: "paused" });
    fake.send({ type: "response", id: 1, ok: false, error: { kind: "internal" } });

    await expect(second).resolves.toEqual({ ok: true, value: "paused" });
    await expect(first).resolves.toEqual({ ok: false, error: { kind: "internal" } });
    await client.close();
  });
});

describe("BridgeClient: ダイアログの問い合わせ", () => {
  it("ダイアログの問い合わせに、onDialog の結果を同じ id の返事として送る", async () => {
    const fake = fakeTransport();
    const onDialog = vi.fn(() => Promise.resolve(PATH_A));
    const client = createBridgeClient(fake.transport, onDialog);

    const pending = client.invoke("open_source_file", null);
    await vi.waitFor(() => expect(fake.written).toHaveLength(1));

    fake.send({ type: "dialog", id: 1, kind: "open" });

    await vi.waitFor(() =>
      expect(fake.written).toContainEqual({ type: "dialogResult", id: 1, result: PATH_A }),
    );
    expect(onDialog).toHaveBeenCalledWith("open");

    fake.send({ type: "response", id: 1, ok: true, value: null });
    await expect(pending).resolves.toEqual({ ok: true, value: null });
    await client.close();
  });

  it("並行の問い合わせには、それぞれの id の返事を送り、保存と開くの結果が混ざらない", async () => {
    const fake = fakeTransport();
    const onDialog = vi.fn((kind: DialogKind) => Promise.resolve(kind === "open" ? PATH_A : PATH_B));
    const client = createBridgeClient(fake.transport, onDialog);

    const opening = client.invoke("open_source_file", null);
    const saving = client.invoke("save_source_file", null);
    await vi.waitFor(() => expect(fake.written).toHaveLength(2));

    // 後に送った要求(id 2)の問い合わせを先に出す
    fake.send({ type: "dialog", id: 2, kind: "save" });
    fake.send({ type: "dialog", id: 1, kind: "open" });

    await vi.waitFor(() => expect(fake.written).toHaveLength(4));
    expect(fake.written).toContainEqual({ type: "dialogResult", id: 2, result: PATH_B });
    expect(fake.written).toContainEqual({ type: "dialogResult", id: 1, result: PATH_A });
    expect(onDialog).toHaveBeenCalledWith("save");
    expect(onDialog).toHaveBeenCalledWith("open");

    fake.send({ type: "response", id: 1, ok: true, value: "opened" });
    fake.send({ type: "response", id: 2, ok: true, value: "saved" });
    await expect(opening).resolves.toEqual({ ok: true, value: "opened" });
    await expect(saving).resolves.toEqual({ ok: true, value: "saved" });
    await client.close();
  });
});

describe("BridgeClient: 終了と標準エラー", () => {
  it("プロセスが終了したら応答待ちの要求をすべて BridgeExitedError で失敗させ、終了内容と標準エラーの末尾を保持する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const first = client.invoke("run_batch", { args: {} });
    const second = client.invoke("step", null);
    fake.writeStderr("panic: 想定外の状態\n");
    await vi.waitFor(() => expect(client.stderrTail).toContain("panic"));

    fake.finish({ code: 3, signal: null });

    await expect(first).rejects.toBeInstanceOf(BridgeExitedError);
    await expect(second).rejects.toBeInstanceOf(BridgeExitedError);
    expect(client.exit).toEqual({ code: 3, signal: null });
    expect(client.stderrTail).toContain("panic: 想定外の状態");
  });

  it("標準エラーは末尾だけを保持する: 先頭の文字は残さず、上限を超えない", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    fake.writeStderr(`HEAD-MARK\n${"a".repeat(5_000)}\nTAIL-MARK\n`);
    await vi.waitFor(() => expect(client.stderrTail).toContain("TAIL-MARK"));

    expect(client.stderrTail).not.toContain("HEAD-MARK");
    expect(client.stderrTail.length).toBeLessThanOrEqual(STDERR_TAIL_LIMIT);
    expect(STDERR_TAIL_LIMIT).toBe(4_000);
    await client.close({ graceMs: 10 });
  });

  it("終了時の BridgeExitedError のメッセージに、標準エラーの末尾が載り、先頭は載らない", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const pending = client.invoke("run_batch", { args: {} });
    fake.writeStderr(`HEAD-MARK\n${"a".repeat(5_000)}\nTAIL-MARK\n`);
    await vi.waitFor(() => expect(client.stderrTail).toContain("TAIL-MARK"));

    fake.finish({ code: 1, signal: null });

    const error = await rejectionOf(pending);
    expect(error).toBeInstanceOf(BridgeExitedError);
    expect((error as Error).message).toContain("TAIL-MARK");
    expect((error as Error).message).not.toContain("HEAD-MARK");
  });

  it("終了後の要求は送らずに BridgeExitedError で失敗する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    fake.finish({ code: 1, signal: null });
    await vi.waitFor(() => expect(client.exit).not.toBeNull());

    await expect(client.invoke("pause", null)).rejects.toBeInstanceOf(BridgeExitedError);
    expect(fake.written).toHaveLength(0);
  });

  it("close は標準入力を閉じ、猶予の後に強制終了する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    await client.close({ graceMs: 10 });

    expect(fake.stdinEnded()).toBe(true);
    expect(fake.kill).toHaveBeenCalledTimes(1);
    expect(client.exit).not.toBeNull();
  });

  it("猶予の内に終了したら強制終了しない", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const closing = client.close({ graceMs: 1_000 });
    fake.finish({ code: 0, signal: null });
    await closing;

    expect(fake.kill).not.toHaveBeenCalled();
  });

  it("指定した猶予の 1 ミリ秒前には強制終了せず、猶予を過ぎたら 1 回だけ強制終了する", async () => {
    vi.useFakeTimers();
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const closing = client.close({ graceMs: 100 });
    await vi.advanceTimersByTimeAsync(99);
    expect(fake.kill).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    await closing;
    expect(fake.kill).toHaveBeenCalledTimes(1);
  });

  it("猶予を省略したときは DEFAULT_CLOSE_GRACE_MS の後に強制終了する", async () => {
    vi.useFakeTimers();
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const closing = client.close();
    await vi.advanceTimersByTimeAsync(DEFAULT_CLOSE_GRACE_MS - 1);
    expect(fake.kill).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    await closing;
    expect(fake.kill).toHaveBeenCalledTimes(1);
  });
});

describe("BridgeClient: 続けられない失敗", () => {
  it("不正な行を受けたら、応答待ちの要求を失敗させ、プロセスを閉じ、以後の要求も同じ失敗で拒否する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const pending = client.invoke("pause", null);
    await vi.waitFor(() => expect(fake.written).toHaveLength(1));

    fake.send("これは JSON ではありません");

    const error = await rejectionOf(pending);
    expect(error).toBeInstanceOf(ProtocolError);
    expect(fake.kill).toHaveBeenCalledTimes(1);
    await expect(client.invoke("pause", null)).rejects.toBe(error);
    expect(fake.written).toHaveLength(1);
  });

  it("想定外の id の応答を受けたら、応答待ちの要求を失敗させ、プロセスを閉じ、以後の要求も同じ失敗で拒否する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const pending = client.invoke("pause", null);
    await vi.waitFor(() => expect(fake.written).toHaveLength(1));

    fake.send({ type: "response", id: 99, ok: true, value: null });

    const error = await rejectionOf(pending);
    expect((error as Error).message).toContain("想定外の応答");
    expect(fake.kill).toHaveBeenCalledTimes(1);
    await expect(client.invoke("pause", null)).rejects.toBe(error);
  });

  it("onDialog が拒否したら、応答待ちの要求をその理由で失敗させ、プロセスを閉じる", async () => {
    const fake = fakeTransport();
    const onDialog = vi.fn(() => Promise.reject(new Error("ダイアログを開けません")));
    const client = createBridgeClient(fake.transport, onDialog);

    const pending = client.invoke("open_source_file", null);
    await vi.waitFor(() => expect(fake.written).toHaveLength(1));

    fake.send({ type: "dialog", id: 1, kind: "open" });

    await expect(pending).rejects.toThrow("ダイアログを開けません");
    expect(fake.kill).toHaveBeenCalledTimes(1);
    await expect(client.invoke("pause", null)).rejects.toThrow("ダイアログを開けません");
  });
});

describe("startBridge と起動確認", () => {
  it("実行ファイルが無いときは、パスと npm run e2e:prepare を示して失敗する", async () => {
    const missing = path.join(os.tmpdir(), "no-such-dir-e2e-bridge", "brainfuck-e2e-bridge.exe");

    const starting = startBridge(missing, () => Promise.reject(new Error("unused")), {
      readyWithinMs: 1_000,
    });

    await expect(starting).rejects.toThrow(missing);
    await expect(starting).rejects.toThrow("npm run e2e:prepare");
  });

  it("起動確認(e2e:ping)が上限時間までに応答しなければ、案内を示して失敗する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const ready = waitForReady(client, 20);

    await expect(ready).rejects.toThrow("ウイルス対策");
    await expect(ready).rejects.toThrow("除外");
    await expect(ready).rejects.toThrow("一時停止");
    await expect(ready).rejects.toThrow("E2E_BRIDGE_READY_MS");
    await expect(ready).rejects.toThrow("CARGO_TARGET_DIR");
    await expect(ready).rejects.toThrow("npm run e2e:prepare");
    await expect(ready).rejects.toThrow("20");
    expect(fake.written).toEqual([{ type: "request", id: 1, cmd: "e2e:ping", payload: null }]);
  });

  it("起動確認が pong で応答すれば解決する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const ready = waitForReady(client, 1_000);
    await vi.waitFor(() => expect(fake.written).toHaveLength(1));
    fake.send({ type: "response", id: 1, ok: true, value: "pong" });

    await expect(ready).resolves.toBeUndefined();
    await client.close({ graceMs: 10 });
  });

  it("起動確認の応答が pong 以外の値なら失敗する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const ready = waitForReady(client, 1_000);
    await vi.waitFor(() => expect(fake.written).toHaveLength(1));
    fake.send({ type: "response", id: 1, ok: true, value: "ping" });

    await expect(ready).rejects.toThrow("想定と違います");
    await client.close({ graceMs: 10 });
  });

  it("起動確認が ok:false で応答したら失敗する", async () => {
    const fake = fakeTransport();
    const client = createBridgeClient(fake.transport, () => Promise.reject(new Error("unused")));

    const ready = waitForReady(client, 1_000);
    await vi.waitFor(() => expect(fake.written).toHaveLength(1));
    fake.send({ type: "response", id: 1, ok: false, error: { kind: "internal" } });

    await expect(ready).rejects.toThrow("起動確認(e2e:ping)が失敗しました");
    await client.close({ graceMs: 10 });
  });

  it("起動確認に失敗したら、標準入力を閉じてプロセスを終了させてから失敗を伝える", async () => {
    const fake = fakeTransport({ exitOnStdinEnd: true });

    const starting = startBridge("unused.exe", () => Promise.reject(new Error("unused")), {
      readyWithinMs: 20,
      spawn: () => fake.transport,
    });

    await expect(starting).rejects.toThrow("ウイルス対策");
    expect(fake.stdinEnded()).toBe(true);
    expect(fake.kill).not.toHaveBeenCalled();
  });

  it("E2E_BRIDGE_READY_MS が無ければ 30000、数値なら、その値を待ち時間にする", () => {
    expect(readyWithinMsFromEnv({})).toBe(30_000);
    expect(readyWithinMsFromEnv({ E2E_BRIDGE_READY_MS: "" })).toBe(30_000);
    expect(readyWithinMsFromEnv({ E2E_BRIDGE_READY_MS: "45000" })).toBe(45_000);
  });
});
