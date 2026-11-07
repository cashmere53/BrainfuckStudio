import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ApiError } from "../generated/ApiError";
import { CLOSE_REQUESTED_EVENT, COMMANDS, invokeCommand, tauriBackend } from "./backend";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const listenMock = vi.mocked(listen);

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
});

describe("invokeCommand: 引数 1 個の規約と拒否値の正規化", () => {
  it("成功値をそのまま返し、引数は { args } の 1 個にまとめて渡す", async () => {
    invokeMock.mockResolvedValueOnce({ kind: "saved", fileName: "a.bf" });
    const result = await invokeCommand<{ kind: string; fileName: string }>("save_source_file", {
      source: "+",
      saveAs: false,
    });
    expect(result).toEqual({ kind: "saved", fileName: "a.bf" });
    expect(invokeMock).toHaveBeenCalledWith("save_source_file", {
      args: { source: "+", saveAs: false },
    });
  });

  it("引数を省略した呼び出しは第 2 引数を渡さない", async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    await invokeCommand("confirm_exit");
    expect(invokeMock.mock.calls[0]?.[0]).toBe("confirm_exit");
    expect(invokeMock.mock.calls[0]?.[1]).toBeUndefined();
  });

  it("ApiError の拒否値は ApiError として reject する", async () => {
    const err: ApiError = { kind: "staleProgram", message: "stale program", field: null };
    invokeMock.mockRejectedValueOnce(err);
    await expect(invokeCommand("start_session", { args: {} })).rejects.toEqual(err);
  });

  it("未知の形の拒否値(文字列・Error)は internal に正規化して reject する", async () => {
    invokeMock.mockRejectedValueOnce("raw string");
    const first = await invokeCommand("step").catch((e: unknown) => e);
    expect(first).toMatchObject({ kind: "internal", field: null });

    invokeMock.mockRejectedValueOnce(new Error("ipc is not available"));
    const second = await invokeCommand("step").catch((e: unknown) => e);
    expect(second).toMatchObject({ kind: "internal", field: null });
  });
});

describe("tauriBackend: 全コマンドの呼び出し形", () => {
  it("各メソッドが対応するコマンド名と { args } で invoke を呼ぶ", async () => {
    invokeMock.mockResolvedValue(undefined);

    await tauriBackend.loadSource({ source: "+" });
    await tauriBackend.startSession({
      programId: 1,
      inputBase64: "",
      limit: { kind: "unlimited" },
      breakpointOffsets: [0],
      view: { tapeRadius: 16, outputFrom: 0 },
    });
    await tauriBackend.runBatch({
      sessionId: 2,
      batchSize: null,
      view: { tapeRadius: 16, outputFrom: 0 },
    });
    await tauriBackend.step({ sessionId: 2, view: { tapeRadius: 16, outputFrom: 0 } });
    await tauriBackend.reset({ sessionId: 2, view: { tapeRadius: 16, outputFrom: 0 } });
    await tauriBackend.getSnapshot({ sessionId: 2, view: { tapeRadius: 16, outputFrom: 0 } });
    await tauriBackend.getTapeWindow({ sessionId: 2, radius: 8 });
    await tauriBackend.setBreakpoints({ sessionId: 2, offsets: [1, 3] });
    await tauriBackend.pause({ sessionId: 2 });
    await tauriBackend.newDocument();
    await tauriBackend.openSourceFile();
    await tauriBackend.saveSourceFile({ source: "+", saveAs: true });
    await tauriBackend.setDocumentDirty(true);
    await tauriBackend.confirmExit();

    const calls = invokeMock.mock.calls.map(([name, payload]) => [name, payload]);
    expect(calls).toEqual([
      ["load_source", { args: { source: "+" } }],
      [
        "start_session",
        {
          args: {
            programId: 1,
            inputBase64: "",
            limit: { kind: "unlimited" },
            breakpointOffsets: [0],
            view: { tapeRadius: 16, outputFrom: 0 },
          },
        },
      ],
      ["run_batch", { args: { sessionId: 2, batchSize: null, view: { tapeRadius: 16, outputFrom: 0 } } }],
      ["step", { args: { sessionId: 2, view: { tapeRadius: 16, outputFrom: 0 } } }],
      ["reset", { args: { sessionId: 2, view: { tapeRadius: 16, outputFrom: 0 } } }],
      ["get_snapshot", { args: { sessionId: 2, view: { tapeRadius: 16, outputFrom: 0 } } }],
      ["get_tape_window", { args: { sessionId: 2, radius: 8 } }],
      ["set_breakpoints", { args: { sessionId: 2, offsets: [1, 3] } }],
      ["pause", { args: { sessionId: 2 } }],
      ["new_document", undefined],
      ["open_source_file", undefined],
      ["save_source_file", { args: { source: "+", saveAs: true } }],
      ["set_document_dirty", { args: { dirty: true } }],
      ["confirm_exit", undefined],
    ]);
  });

  it("コマンド名の定数は 14 本で、関数名と一致する", () => {
    expect(Object.values(COMMANDS)).toHaveLength(14);
    expect(COMMANDS.getTapeWindow).toBe("get_tape_window");
    expect(COMMANDS.setDocumentDirty).toBe("set_document_dirty");
  });

  it("失敗は ApiError として reject する(メソッド経由でも正規化される)", async () => {
    invokeMock.mockRejectedValueOnce({ kind: "sessionNotFound", message: "x", field: null });
    await expect(
      tauriBackend.step({ sessionId: 9, view: { tapeRadius: 0, outputFrom: 0 } }),
    ).rejects.toMatchObject({ kind: "sessionNotFound" });
  });
});

describe("tauriBackend.onCloseRequested: 閉じる要求の購読", () => {
  it("CLOSE_REQUESTED_EVENT を購読し、通知で handler を呼び、解除関数を返す", async () => {
    const unlisten = vi.fn();
    let fire: (() => void) | undefined;
    listenMock.mockImplementationOnce(async (_event, callback) => {
      fire = () => callback({ event: "close-requested", id: 1, payload: undefined });
      return unlisten;
    });

    const handler = vi.fn();
    const stop = await tauriBackend.onCloseRequested(handler);
    expect(listenMock.mock.calls[0]?.[0]).toBe(CLOSE_REQUESTED_EVENT);

    fire?.();
    expect(handler).toHaveBeenCalledTimes(1);

    stop();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
