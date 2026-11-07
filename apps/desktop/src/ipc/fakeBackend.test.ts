import { describe, expect, it } from "vitest";
import type { ApiError } from "../generated/ApiError";
import { createFakeBackend } from "./fakeBackend";

const view = { tapeRadius: 16, outputFrom: 0 };
const state = {
  sessionId: 1,
  snapshot: { pc: 0, dp: 0, currentCell: 0, stepsExecuted: 0, halted: false },
  tapeWindow: { start: -16, cells: [], pointerOffset: 16 },
  output: { dataStart: 0, totalLen: 0, dataBase64: "" },
  inputConsumed: 0,
  inputTotal: 0,
  lastStop: null,
  currentRange: null,
};

describe("createFakeBackend: 台本どおりの応答", () => {
  it("台本の応答を返し、呼び出しを記録する", async () => {
    const fake = createFakeBackend({
      getSnapshot: () => state,
      confirmExit: () => undefined,
    });
    await expect(fake.getSnapshot({ sessionId: 1, view })).resolves.toEqual(state);
    await expect(fake.confirmExit()).resolves.toBeUndefined();
    expect(fake.calls).toEqual([
      { method: "getSnapshot", args: [{ sessionId: 1, view }] },
      { method: "confirmExit", args: [] },
    ]);
  });

  it("台本に無いメソッドは ApiError(internal)で reject する", async () => {
    const fake = createFakeBackend({});
    const error = await fake.newDocument().catch((e: unknown) => e);
    expect(error).toMatchObject({ kind: "internal", field: null });
  });

  it("台本が投げた ApiError はそのまま reject し、未知の値は internal に正規化する", async () => {
    const apiError: ApiError = { kind: "io", message: "failed to read file: denied", field: null };
    const fake = createFakeBackend({
      openSourceFile: () => {
        throw apiError;
      },
      saveSourceFile: () => {
        throw "unexpected";
      },
    });
    await expect(fake.openSourceFile()).rejects.toEqual(apiError);
    await expect(fake.saveSourceFile({ source: "", saveAs: false })).rejects.toMatchObject({
      kind: "internal",
    });
  });

  it("全コマンドを台本で呼べる", async () => {
    const fake = createFakeBackend({
      loadSource: () => ({ kind: "ok", programId: 1, instructionCount: 0, lineCount: 1 }),
      startSession: () => ({ state, breakpoints: { breakpoints: [], unresolved: [] } }),
      runBatch: () => ({ stop: { kind: "halted" }, executed: 0, state }),
      step: () => ({ outcome: { kind: "halted" }, state }),
      reset: () => state,
      getSnapshot: () => state,
      getTapeWindow: () => state.tapeWindow,
      setBreakpoints: () => ({ breakpoints: [], unresolved: [] }),
      pause: () => undefined,
      newDocument: () => undefined,
      openSourceFile: () => ({ kind: "cancelled" as const }),
      saveSourceFile: () => ({ kind: "cancelled" as const }),
      setDocumentDirty: () => undefined,
      confirmExit: () => undefined,
    });
    await fake.loadSource({ source: "" });
    await fake.startSession({
      programId: 1,
      inputBase64: "",
      limit: { kind: "unlimited" },
      breakpointOffsets: [],
      view,
    });
    await fake.runBatch({ sessionId: 1, batchSize: null, view });
    await fake.step({ sessionId: 1, view });
    await fake.reset({ sessionId: 1, view });
    await fake.getSnapshot({ sessionId: 1, view });
    await fake.getTapeWindow({ sessionId: 1, radius: 4 });
    await fake.setBreakpoints({ sessionId: 1, offsets: [] });
    await fake.pause({ sessionId: 1 });
    await fake.newDocument();
    await fake.openSourceFile();
    await fake.saveSourceFile({ source: "", saveAs: false });
    await fake.setDocumentDirty(false);
    await fake.confirmExit();
    expect(fake.calls.map((c) => c.method)).toHaveLength(14);
  });
});

describe("createFakeBackend: 閉じる要求の購読", () => {
  it("emitCloseRequested で購読者へ通知し、解除後は通知されない", async () => {
    const fake = createFakeBackend({});
    const received: number[] = [];
    const stopA = await fake.onCloseRequested(() => received.push(1));
    await fake.onCloseRequested(() => received.push(2));

    fake.emitCloseRequested();
    expect(received).toEqual([1, 2]);

    stopA();
    fake.emitCloseRequested();
    expect(received).toEqual([1, 2, 2]);
  });
});
