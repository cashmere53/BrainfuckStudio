import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { ApiError } from "../../generated/ApiError";
import type { ComparisonInfoDto } from "../../generated/ComparisonInfoDto";
import type { ComparisonResultDto } from "../../generated/ComparisonResultDto";
import type { RunComparisonRequest } from "../../generated/RunComparisonRequest";
import { apiErrorMessageJa } from "../../ipc/errors";
import { createWorkspace } from "../../state/workspace";
import { createFakeComparisonBackend } from "../ipc";
import type { Selection } from "../domain/comparisonRequest";
import {
  comparisonReducer,
  createComparisonController,
  initialComparisonState,
  isResultStale,
  type ComparisonAction,
  type ComparisonState,
} from "./comparisonStore";

const INFO: ComparisonInfoDto = {
  maxSteps: 1000,
  maxRepetitions: 5,
  defaultRepetitions: 1,
  methods: ["naive", "optimized", "wasm"],
};

const SELECTION: Selection = { optimized: true, wasm: false, repetitions: 1 };

const SNAPSHOT = {
  source: "+.",
  input: { mode: "text" as const, text: "A" },
  limit: { text: "100", unlimited: false, lastValid: { kind: "limited" as const, maxSteps: 100 } },
};

function result(limit: number): ComparisonResultDto {
  return { kind: "completed", limit, overall: "allMatch", methods: [] };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** テストごとに入力・上限・ソースを決めた文書を作る */
function makeWorkspace() {
  const workspace = createWorkspace();
  workspace.setSource("+.");
  workspace.setInput({ mode: "text", text: "A" });
  workspace.setLimitText("100");
  return workspace;
}

describe("comparisonReducer: 状態遷移・古い応答の破棄", () => {
  const started = (requestId: number): ComparisonAction => ({ type: "runStarted", requestId, snapshot: SNAPSHOT });

  it("初期状態はアイドルで、結果・スナップショット・エラーを持たない", () => {
    expect(initialComparisonState).toEqual({
      phase: "idle",
      requestId: 0,
      result: null,
      usedSnapshot: null,
      error: null,
    });
  });

  it("開始で実行中になり、要求 ID とスナップショットを保持し、前の結果を消す", () => {
    const previous: ComparisonState = { ...initialComparisonState, phase: "done", requestId: 1, result: result(1) };
    const next = comparisonReducer(previous, started(2));
    expect(next).toEqual({ phase: "running", requestId: 2, result: null, usedSnapshot: SNAPSHOT, error: null });
  });

  it("最新の要求 ID の成功応答で完了になる", () => {
    const running = comparisonReducer(initialComparisonState, started(1));
    const next = comparisonReducer(running, { type: "runSucceeded", requestId: 1, result: result(7) });
    expect(next.phase).toBe("done");
    expect(next.result).toEqual(result(7));
    expect(next.usedSnapshot).toBe(SNAPSHOT);
  });

  it("古い要求 ID の成功応答は無視する(状態の同一性も保つ)", () => {
    const running = comparisonReducer(initialComparisonState, started(2));
    const next = comparisonReducer(running, { type: "runSucceeded", requestId: 1, result: result(7) });
    expect(next).toBe(running);
  });

  it("最新の要求 ID の失敗応答は失敗になり、エラーを保持する。古い失敗は無視する", () => {
    const error: ApiError = { kind: "sourceTooLarge", message: "", field: null };
    const running = comparisonReducer(initialComparisonState, started(3));
    expect(comparisonReducer(running, { type: "runFailed", requestId: 2, error })).toBe(running);
    const failed = comparisonReducer(running, { type: "runFailed", requestId: 3, error });
    expect(failed).toMatchObject({ phase: "failed", result: null, error });
  });

  it("実行中でない状態への応答は、要求 ID が一致しても無視する", () => {
    const done = comparisonReducer(comparisonReducer(initialComparisonState, started(1)), {
      type: "runSucceeded",
      requestId: 1,
      result: result(1),
    });
    expect(comparisonReducer(done, { type: "runSucceeded", requestId: 1, result: result(9) })).toBe(done);
  });

  it("破棄は結果を消してアイドルに戻し、要求 ID を進めるため届き遅れた応答を無効にする", () => {
    const running = comparisonReducer(initialComparisonState, started(1));
    const discarded = comparisonReducer(running, { type: "discarded" });
    expect(discarded).toMatchObject({ phase: "idle", result: null, usedSnapshot: null, error: null });
    expect(discarded.requestId).toBe(2);
    expect(comparisonReducer(discarded, { type: "runSucceeded", requestId: 1, result: result(1) })).toBe(discarded);
  });
});

describe("isResultStale: 実行時スナップショットと現在の共有値の比較(要件 11.8)", () => {
  const current = () => ({
    source: SNAPSHOT.source,
    input: { ...SNAPSHOT.input },
    limit: { ...SNAPSHOT.limit },
  });

  it("スナップショットが無ければ古くない", () => {
    expect(isResultStale(null, current())).toBe(false);
  });

  it("値が同じ(別のオブジェクトでも)なら古くない", () => {
    expect(isResultStale(SNAPSHOT, current())).toBe(false);
  });

  it("ソース・入力の内容・入力の方式・上限の文字列・上限なしの指定のいずれかが変われば古い", () => {
    expect(isResultStale(SNAPSHOT, { ...current(), source: "+" })).toBe(true);
    expect(isResultStale(SNAPSHOT, { ...current(), input: { mode: "text", text: "B" } })).toBe(true);
    expect(isResultStale(SNAPSHOT, { ...current(), input: { mode: "hex", text: "41" } })).toBe(true);
    expect(isResultStale(SNAPSHOT, { ...current(), limit: { ...SNAPSHOT.limit, text: "101" } })).toBe(true);
    expect(isResultStale(SNAPSHOT, { ...current(), limit: { ...SNAPSHOT.limit, unlimited: true } })).toBe(true);
  });
});

describe("createComparisonController: 実行・抑止・古い応答・エラー", () => {
  it("共有値と選択から要求を導出して 1 回送り、完了の結果と実行時スナップショットを保持する", async () => {
    const workspace = makeWorkspace();
    const backend = createFakeComparisonBackend({ run: () => result(100) });
    const controller = createComparisonController({ backend, workspace });

    await expect(controller.run(SELECTION, INFO)).resolves.toBe(true);

    const expected: RunComparisonRequest = {
      source: "+.",
      inputBase64: "QQ==",
      maxSteps: 100,
      methods: ["optimized"],
      repetitions: 1,
    };
    expect(backend.calls).toEqual([{ method: "run", args: [expected] }]);
    expect(controller.getState()).toMatchObject({
      phase: "done",
      result: result(100),
      usedSnapshot: { source: "+.", input: { mode: "text", text: "A" }, limit: { text: "100", unlimited: false } },
    });
  });

  it("実行中は再実行を抑止し、バックエンドを呼ばない", async () => {
    const workspace = makeWorkspace();
    const pending = deferred<ComparisonResultDto>();
    const backend = createFakeComparisonBackend({ run: () => pending.promise });
    const controller = createComparisonController({ backend, workspace });

    const first = controller.run(SELECTION, INFO);
    expect(controller.getState().phase).toBe("running");
    await expect(controller.run(SELECTION, INFO)).resolves.toBe(false);
    expect(backend.calls).toHaveLength(1);

    pending.resolve(result(100));
    await expect(first).resolves.toBe(true);
    expect(controller.getState().phase).toBe("done");
  });

  it("破棄の後に届いた古い応答は反映せず、新しい要求の応答のみ反映する", async () => {
    const workspace = makeWorkspace();
    const first = deferred<ComparisonResultDto>();
    const second = deferred<ComparisonResultDto>();
    const responses = [first, second];
    const backend = createFakeComparisonBackend({ run: () => responses.shift()!.promise });
    const controller = createComparisonController({ backend, workspace });

    const oldRun = controller.run(SELECTION, INFO);
    controller.discard();
    const newRun = controller.run(SELECTION, INFO);
    expect(backend.calls).toHaveLength(2);

    first.resolve(result(1));
    await oldRun;
    expect(controller.getState()).toMatchObject({ phase: "running", result: null });

    second.resolve(result(2));
    await newRun;
    expect(controller.getState()).toMatchObject({ phase: "done", result: result(2) });
  });

  it("無効な選択(上限なし)は要求を送らず、状態も変えない", async () => {
    const workspace = makeWorkspace();
    workspace.setUnlimited(true);
    const backend = createFakeComparisonBackend({ run: () => result(1) });
    const controller = createComparisonController({ backend, workspace });

    await expect(controller.run(SELECTION, INFO)).resolves.toBe(false);
    expect(backend.calls).toHaveLength(0);
    expect(controller.getState()).toEqual(initialComparisonState);
  });

  it("繰り返し回数が範囲外なら要求を送らない", async () => {
    const workspace = makeWorkspace();
    const backend = createFakeComparisonBackend({ run: () => result(1) });
    const controller = createComparisonController({ backend, workspace });

    await expect(controller.run({ ...SELECTION, repetitions: 6 }, INFO)).resolves.toBe(false);
    expect(backend.calls).toHaveLength(0);
  });

  it("操作エラーは失敗として保持し、通知文に変換できる", async () => {
    const workspace = makeWorkspace();
    const error: ApiError = { kind: "invalidArgument", message: "bad", field: "batchSize" };
    const backend = createFakeComparisonBackend({
      run: () => {
        throw error;
      },
    });
    const controller = createComparisonController({ backend, workspace });

    await controller.run(SELECTION, INFO);
    const state = controller.getState();
    expect(state.phase).toBe("failed");
    expect(state.error).toEqual(error);
    expect(apiErrorMessageJa(state.error!)).toContain("バッチ幅");
  });

  it("未知の形の拒否は internal に正規化して保持する", async () => {
    const workspace = makeWorkspace();
    const backend = createFakeComparisonBackend({
      run: () => {
        throw new Error("secret detail");
      },
    });
    const controller = createComparisonController({ backend, workspace });

    await controller.run(SELECTION, INFO);
    expect(controller.getState().error).toEqual({
      kind: "internal",
      message: "unexpected error response from the backend",
      field: null,
    });
  });

  it("共有値の変更後は、結果が古いと判定される。元に戻せば古くない", async () => {
    const workspace = makeWorkspace();
    const backend = createFakeComparisonBackend({ run: () => result(100) });
    const controller = createComparisonController({ backend, workspace });
    await controller.run(SELECTION, INFO);

    const snapshot = controller.getState().usedSnapshot;
    workspace.setSource("+.+");
    expect(isResultStale(snapshot, workspace.getSnapshot())).toBe(true);
    workspace.setSource("+.");
    expect(isResultStale(snapshot, workspace.getSnapshot())).toBe(false);
  });

  it("デバッガのストア・実行コントローラを参照・変更しない(要件 11.11)", () => {
    const source = readFileSync(join(__dirname, "comparisonStore.ts"), "utf8");
    expect(source).not.toMatch(/debuggerStore|runController|RunController/);
    const ipcSource = readFileSync(join(__dirname, "..", "ipc.ts"), "utf8");
    expect(ipcSource).not.toMatch(/debuggerStore|runController|RunController/);
  });
});
