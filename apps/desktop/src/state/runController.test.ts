import { describe, expect, it } from "vitest";
import type { ApiError } from "../generated/ApiError";
import type { BreakpointsDto } from "../generated/BreakpointsDto";
import type { LoadSourceResult } from "../generated/LoadSourceResult";
import type { ParseErrorDto } from "../generated/ParseErrorDto";
import type { SessionStartedDto } from "../generated/SessionStartedDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { StepResultDto } from "../generated/StepResultDto";
import type { TapeWindowDto } from "../generated/TapeWindowDto";
import { createFakeBackend, type FakeScript } from "../ipc/fakeBackend";
import { encodeBase64 } from "../domain/base64";
import { MAX_SOURCE_BYTES, DEFAULT_MAX_STEPS } from "../domain/limits";
import { debuggerReducer, initialDebuggerState, type DebuggerAction, type DebuggerState } from "./debuggerStore";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { BatchStopDto } from "../generated/BatchStopDto";
import type { OutputChunkDto } from "../generated/OutputChunkDto";
import type { RunBatchRequest } from "../generated/RunBatchRequest";
import { createRunController, type RunControllerDeps } from "./runController";
import { createWorkspace, type WorkspaceStore } from "./workspace";

type RunOptions = Partial<Pick<RunControllerDeps, "now" | "frames" | "canStart">>;

const PARSE_ERR: ParseErrorDto = {
  kind: "unmatchedOpeningBracket",
  instructionIndex: 0,
  position: { byteOffset: 0, utf16Offset: 0, line: 1, column: 1 },
};
const SESSION_ERR: ApiError = { kind: "sessionNotFound", message: "session not found", field: null };
const IO_ERR: ApiError = { kind: "io", message: "failed to read file: denied", field: null };

/** 状態 DTO を作る(表示窓は 16、出力は空) */
function stateDto(sessionId: number, stepsExecuted: number, halted = false): SessionStateDto {
  return {
    sessionId,
    snapshot: { pc: stepsExecuted, dp: 0, currentCell: 0, stepsExecuted, halted },
    tapeWindow: { start: -16, cells: Array.from({ length: 33 }, () => 0), pointerOffset: 16 },
    output: { dataStart: 0, totalLen: 0, dataBase64: "" },
    inputConsumed: 0,
    inputTotal: 1,
    lastStop: null,
    currentRange: null,
  };
}

const OK_LOAD: LoadSourceResult = { kind: "ok", programId: 7, instructionCount: 3, lineCount: 1 };

/** 既定の台本: 読み込み成功 → セッション 1 → Step で 1 命令実行 */
function okScript(overrides: FakeScript = {}): FakeScript {
  let steps = 0;
  return {
    loadSource: () => OK_LOAD,
    startSession: (): SessionStartedDto => ({
      state: stateDto(1, 0),
      breakpoints: { breakpoints: [], unresolved: [] },
    }),
    step: (): StepResultDto => {
      steps += 1;
      return { outcome: { kind: "executed" }, state: stateDto(1, steps) };
    },
    reset: () => stateDto(1, 0),
    ...overrides,
  };
}

/** 状態を保持する最小のホスト(React の useReducer と同じ reducer を使う) */
function makeHost() {
  let state: DebuggerState = initialDebuggerState();
  const dispatched: DebuggerAction[] = [];
  return {
    getState: () => state,
    dispatch: (action: DebuggerAction) => {
      dispatched.push(action);
      state = debuggerReducer(state, action);
    },
    dispatched,
  };
}

function setup(script: FakeScript = okScript(), options: RunOptions = {}) {
  const backend = createFakeBackend(script);
  const workspace = createWorkspace();
  const host = makeHost();
  const controller = createRunController({ backend, workspace, host, ...options });
  return { backend, workspace, host, controller };
}

const methods = (backend: { calls: readonly { method: string }[] }) => backend.calls.map((c) => c.method);

/** 手動で解決できる Promise(応答の到着タイミングを制御する) */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** 保留中の非同期処理を進める(マイクロタスクとタイマーを消化する) */
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("RunController: セッション開始(ensureSession)", () => {
  it("編集中の未保存ソースを読み込み、入力・上限・ブレークポイント・表示窓で開始する", async () => {
    const { backend, workspace, host, controller } = setup();
    workspace.setSource("+.");
    workspace.setInput({ mode: "text", text: "A" });
    workspace.setBreakpoints([1]);

    await expect(controller.ensureSession()).resolves.toBe(true);

    expect(backend.calls.map((c) => c.args)).toEqual([
      [{ source: "+." }],
      [
        {
          programId: 7,
          inputBase64: "QQ==",
          limit: { kind: "limited", maxSteps: DEFAULT_MAX_STEPS },
          breakpointOffsets: [1],
          view: { tapeRadius: 16, outputFrom: 0 },
        },
      ],
    ]);
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().sessionId).toBe(1);
    expect(host.getState().programId).toBe(7);
  });

  it("上限が無効な文字列でも直前の有効な上限で開始する", async () => {
    const { backend, workspace, controller } = setup();
    workspace.setLimitText("5");
    workspace.setLimitText("abc");

    await controller.ensureSession();

    const startArgs = backend.calls.find((c) => c.method === "startSession")?.args[0] as { limit: unknown };
    expect(startArgs.limit).toEqual({ kind: "limited", maxSteps: 5 });
  });

  it("上限なしが選ばれていれば unlimited で開始する", async () => {
    const { backend, workspace, controller } = setup();
    workspace.setUnlimited(true);

    await controller.ensureSession();

    const startArgs = backend.calls.find((c) => c.method === "startSession")?.args[0] as { limit: unknown };
    expect(startArgs.limit).toEqual({ kind: "unlimited" });
  });

  it("解析エラーならセッションを作らず、解析エラーを表示する", async () => {
    const { backend, host, controller } = setup(
      okScript({ loadSource: (): LoadSourceResult => ({ kind: "parseError", error: PARSE_ERR }) }),
    );

    await expect(controller.ensureSession()).resolves.toBe(false);

    expect(methods(backend)).toEqual(["loadSource"]);
    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().sessionId).toBeNull();
    expect(host.getState().error).toEqual({ category: "parse", error: PARSE_ERR });
  });

  it("入力が不正なら読み込みも開始もせず、どの要求も発行しない", async () => {
    const { backend, workspace, host, controller } = setup();
    workspace.setInput({ mode: "hex", text: "4" });

    await expect(controller.ensureSession()).resolves.toBe(false);

    expect(backend.calls).toEqual([]);
    expect(host.getState().phase).toBe("noSession");
  });

  it("ソースが 1 MiB を超えるなら実行を開始せず、通知だけを出す", async () => {
    const { backend, workspace, host, controller } = setup();
    workspace.setSource("+".repeat(MAX_SOURCE_BYTES + 1));

    await expect(controller.ensureSession()).resolves.toBe(false);

    expect(backend.calls).toEqual([]);
    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([{ kind: "sourceTooLarge", message: "source is too large", field: "source" }]);
  });

  it("上限は UTF-8 のバイト数で判定する: UTF-16 長は上限以下でも UTF-8 が上限を超えれば開始しない", async () => {
    // 「あ」は UTF-16 で 1 単位・UTF-8 で 3 バイト。UTF-16 長は上限以下、UTF-8 長は上限超え
    const source = "あ".repeat(Math.floor(MAX_SOURCE_BYTES / 3) + 1);
    expect(source.length).toBeLessThanOrEqual(MAX_SOURCE_BYTES);
    expect(new TextEncoder().encode(source).length).toBeGreaterThan(MAX_SOURCE_BYTES);
    const { backend, workspace, host, controller } = setup();
    workspace.setSource(source);

    await expect(controller.ensureSession()).resolves.toBe(false);

    expect(backend.calls).toEqual([]);
    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([{ kind: "sourceTooLarge", message: "source is too large", field: "source" }]);
  });

  it("UTF-8 で上限ちょうどのソースは開始できる", async () => {
    const { backend, workspace, controller } = setup();
    workspace.setSource("+".repeat(MAX_SOURCE_BYTES));

    await expect(controller.ensureSession()).resolves.toBe(true);

    expect(methods(backend)).toEqual(["loadSource", "startSession"]);
  });

  it("読み込みの失敗は操作の通知になり、セッションは作られない", async () => {
    const { host, controller } = setup(okScript({ loadSource: () => Promise.reject(IO_ERR) }));

    await controller.ensureSession();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().sessionId).toBeNull();
    expect(host.getState().notices).toEqual([IO_ERR]);
  });

  it("開始時の staleProgram は破棄扱いにして通知を残す", async () => {
    const stale: ApiError = { kind: "staleProgram", message: "stale", field: null };
    const { host, controller } = setup(okScript({ startSession: () => Promise.reject(stale) }));

    await controller.ensureSession();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([stale]);
  });

  it("既にセッションがあれば開始し直さない", async () => {
    const { backend, controller } = setup();
    await controller.ensureSession();
    await controller.ensureSession();

    expect(methods(backend).filter((m) => m === "startSession")).toHaveLength(1);
  });
});

describe("RunController: Step", () => {
  it("セッションが無い状態の Step は開始してから 1 命令実行する", async () => {
    const { backend, host, controller } = setup();

    await controller.step();

    expect(methods(backend)).toEqual(["loadSource", "startSession", "step"]);
    expect(backend.calls[2]?.args[0]).toEqual({ sessionId: 1, view: { tapeRadius: 16, outputFrom: 0 } });
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(1);
    expect(host.getState().scrollRequest).toBe(true);
  });

  it("出力の差分は保持済み末尾の位置から要求する", async () => {
    const bytes = encodeBase64(new Uint8Array([0x41, 0x42]));
    const { backend, controller } = setup(
      okScript({
        step: () => ({
          outcome: { kind: "executed" },
          state: { ...stateDto(1, 1), output: { dataStart: 0, totalLen: 2, dataBase64: bytes } },
        }),
      }),
    );

    await controller.step();
    await controller.step();

    expect(backend.calls.filter((c) => c.method === "step")[1]?.args[0]).toEqual({
      sessionId: 1,
      view: { tapeRadius: 16, outputFrom: 2 },
    });
  });

  it("最後の命令で正常終了(halted)になる", async () => {
    const { host, controller } = setup(
      okScript({ step: () => ({ outcome: { kind: "halted" }, state: stateDto(1, 3, true) }) }),
    );

    await controller.step();

    expect(host.getState().phase).toBe("halted");
    expect(host.getState().scrollRequest).toBe(true);
  });

  it("実行エラーの結果は failed になり、実行エラーとして保持する", async () => {
    const runtimeError = { kind: "pointerOutOfBounds", pc: 0, dp: 0, direction: "left" } as const;
    const { host, controller } = setup(
      okScript({ step: () => ({ outcome: { kind: "error", error: runtimeError }, state: stateDto(1, 0) }) }),
    );

    await controller.step();

    expect(host.getState().phase).toBe("failed");
    expect(host.getState().error).toEqual({ category: "runtime", error: runtimeError });
  });

  it("停止状態(halted)では Step を発行しない", async () => {
    const { backend, controller } = setup(
      okScript({ step: () => ({ outcome: { kind: "halted" }, state: stateDto(1, 1, true) }) }),
    );
    await controller.step();
    await controller.step();

    expect(methods(backend).filter((m) => m === "step")).toHaveLength(1);
  });

  it("連続で呼んでも直列に発行し、前の Step が解決するまで次を backend へ発行しない", async () => {
    const gate = deferred<StepResultDto>();
    let steps = 0;
    const { backend, host, controller } = setup(
      okScript({
        step: (): Promise<StepResultDto> | StepResultDto => {
          steps += 1;
          if (steps === 1) {
            return gate.promise;
          }
          return { outcome: { kind: "executed" }, state: stateDto(1, steps) };
        },
      }),
    );

    const first = controller.step();
    const second = controller.step();
    await flush();

    // 1 本目が未解決の間は 2 本目を発行しない(並行呼び出しなし)
    expect(methods(backend)).toEqual(["loadSource", "startSession", "step"]);

    gate.resolve({ outcome: { kind: "executed" }, state: stateDto(1, 1) });
    await Promise.all([first, second]);

    // 呼び出し列は要求の順のまま完全一致する
    expect(methods(backend)).toEqual(["loadSource", "startSession", "step", "step"]);
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(2);
  });

  it("Step の失敗(セッションなし)は破棄してから通知を出し、未開始へ戻る", async () => {
    const { host, controller } = setup(okScript({ step: () => Promise.reject(SESSION_ERR) }));

    await controller.step();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().sessionId).toBeNull();
    expect(host.getState().notices).toEqual([SESSION_ERR]);
  });

  it("その他の Step の失敗はセッションを保ったまま通知だけを出し、後続の Step は発行される", async () => {
    let steps = 0;
    const { backend, host, controller } = setup(
      okScript({
        step: (): Promise<StepResultDto> => {
          steps += 1;
          if (steps === 1) {
            return Promise.reject(IO_ERR);
          }
          return Promise.resolve({ outcome: { kind: "executed" }, state: stateDto(1, 1) });
        },
      }),
    );

    await controller.step();
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().sessionId).toBe(1);
    expect(host.getState().notices).toEqual([IO_ERR]);

    // 失敗で直列の鎖が止まらず、2 回目の Step が実際に backend へ発行される
    await controller.step();

    expect(methods(backend).filter((m) => m === "step")).toHaveLength(2);
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().sessionId).toBe(1);
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(1);
  });

  it("タスクが例外で終わっても後続の Step は発行される(キューが詰まらない)", async () => {
    const base = createWorkspace();
    let failNextSnapshot = false;
    const workspace: WorkspaceStore = {
      ...base,
      getSnapshot: () => {
        if (failNextSnapshot) {
          failNextSnapshot = false;
          throw new Error("snapshot unavailable");
        }
        return base.getSnapshot();
      },
    };
    const backend = createFakeBackend(okScript());
    const host = makeHost();
    const controller = createRunController({ backend, workspace, host });

    await controller.step();
    failNextSnapshot = true;
    await expect(controller.step()).rejects.toThrow("snapshot unavailable");
    await controller.step();

    expect(methods(backend).filter((m) => m === "step")).toHaveLength(2);
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(2);
  });
});

describe("RunController: Reset", () => {
  it("出力を空にし、強調を初期化し、ブレークポイントを保持する", async () => {
    const bytes = encodeBase64(new Uint8Array([0x41]));
    const { backend, workspace, host, controller } = setup(
      okScript({
        step: () => ({
          outcome: { kind: "executed" },
          state: { ...stateDto(1, 1), output: { dataStart: 0, totalLen: 1, dataBase64: bytes } },
        }),
        reset: () => stateDto(1, 0),
      }),
    );
    workspace.setBreakpoints([0]);
    await controller.step();
    expect(host.getState().output.totalLen).toBe(1);

    await controller.reset();

    expect(backend.calls.at(-1)).toEqual({ method: "reset", args: [{ sessionId: 1, view: { tapeRadius: 16, outputFrom: 0 } }] });
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().output.totalLen).toBe(0);
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(0);
    expect(host.getState().scrollRequest).toBe(false);
    expect(workspace.getSnapshot().breakpoints).toEqual([0]);
  });

  it("セッションが無ければ発行しない", async () => {
    const { backend, controller } = setup();

    await controller.reset();

    expect(backend.calls).toEqual([]);
  });

  it("実行エラー後の Reset で実行エラーを解除して待機へ戻る", async () => {
    const runtimeError = { kind: "pointerOutOfBounds", pc: 0, dp: 0, direction: "left" } as const;
    const { host, controller } = setup(
      okScript({ step: () => ({ outcome: { kind: "error", error: runtimeError }, state: stateDto(1, 0) }) }),
    );
    await controller.step();
    expect(host.getState().error).not.toBeNull();

    await controller.reset();

    expect(host.getState().phase).toBe("idle");
    expect(host.getState().error).toBeNull();
  });

  it("正常終了(halted)後の Reset で待機へ戻り、出力をクリアし、ブレークポイントを保持する", async () => {
    const bytes = encodeBase64(new Uint8Array([0x41]));
    const { backend, workspace, host, controller } = setup(
      okScript({
        step: () => ({
          outcome: { kind: "halted" },
          state: { ...stateDto(1, 1, true), output: { dataStart: 0, totalLen: 1, dataBase64: bytes } },
        }),
      }),
    );
    workspace.setBreakpoints([0]);
    await controller.step();
    expect(host.getState().phase).toBe("halted");
    expect(host.getState().output.totalLen).toBe(1);

    await controller.reset();

    expect(backend.calls.at(-1)?.method).toBe("reset");
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().output.totalLen).toBe(0);
    expect(host.getState().state?.snapshot.halted).toBe(false);
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(0);
    expect(workspace.getSnapshot().breakpoints).toEqual([0]);
  });

  it("Reset の失敗(セッションなし)は未開始へ戻し通知を出す", async () => {
    const { host, controller } = setup(okScript({ reset: () => Promise.reject(SESSION_ERR) }));
    await controller.step();

    await controller.reset();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([SESSION_ERR]);
  });
});

/** ブレークポイント設定の台本: 指定した位置だけを未解決として返す */
function bpScript(unresolvedAt: readonly number[] = []): FakeScript {
  return {
    setBreakpoints: (req) => ({
      breakpoints: [],
      unresolved: req.offsets.filter((offset) => unresolvedAt.includes(offset)),
    }),
  };
}

/** 開始の台本: 指定した位置を未解決にして開始する */
function startWithUnresolved(unresolvedAt: readonly number[]): FakeScript {
  return okScript({
    startSession: (req): SessionStartedDto => ({
      state: stateDto(1, 0),
      breakpoints: {
        breakpoints: [],
        unresolved: req.breakpointOffsets.filter((offset) => unresolvedAt.includes(offset)),
      },
    }),
  });
}

/** 半径に応じた窓(中心は dp = 半径の位置) */
const windowOf = (radius: number): TapeWindowDto => ({
  start: -radius,
  cells: Array.from({ length: 2 * radius + 1 }, () => 0),
  pointerOffset: radius,
});

describe("RunController: 無効化(invalidate)", () => {
  it("ソースの編集でセッションを破棄して未開始へ戻り、出力もクリアする。次の Step は読み込みからやり直す", async () => {
    const { backend, workspace, host, controller } = setup();
    await controller.step();
    expect(host.getState().sessionId).toBe(1);

    workspace.setSource("+.");

    expect(host.getState()).toMatchObject({ phase: "noSession", sessionId: null, programId: null, state: null });
    expect(host.getState().output.totalLen).toBe(0);

    await controller.step();

    expect(methods(backend)).toEqual(["loadSource", "startSession", "step", "loadSource", "startSession", "step"]);
    expect(host.getState().sessionId).toBe(1);
  });

  it("入力の変更でセッションを破棄する", async () => {
    const { host, workspace, controller } = setup();
    await controller.ensureSession();

    workspace.setInput({ mode: "text", text: "A" });

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().sessionId).toBeNull();
  });

  it("有効な上限の値の変更と「上限なし」の切替でセッションを破棄する", async () => {
    const { host, workspace, controller } = setup();
    await controller.ensureSession();
    workspace.setLimitText("42");
    expect(host.getState().phase).toBe("noSession");

    await controller.ensureSession();
    workspace.setUnlimited(true);
    expect(host.getState().phase).toBe("noSession");
  });

  it("不正な上限の入力では破棄しない(採用される上限は直前の有効値のまま)", async () => {
    const { backend, workspace, host, controller } = setup();
    await controller.ensureSession();

    workspace.setLimitText("abc");
    await flush();

    expect(host.getState().phase).toBe("idle");
    expect(host.getState().sessionId).toBe(1);
    expect(methods(backend)).toEqual(["loadSource", "startSession"]);
  });

  it("文書の置換(開く・新規)は内容が同じでもセッションを破棄する", async () => {
    const { host, workspace, controller } = setup();
    await controller.ensureSession();
    workspace.replaceDocument({ source: workspace.getSnapshot().source, fileName: "a.bf", lossy: false });
    expect(host.getState().phase).toBe("noSession");

    await controller.ensureSession();
    workspace.newDocument();
    expect(host.getState().phase).toBe("noSession");
  });

  it("保存済みへの変更(markSaved)では破棄しない", async () => {
    const { host, workspace, controller } = setup();
    await controller.ensureSession();

    workspace.markSaved(workspace.getSnapshot().source, "a.bf");

    expect(host.getState().phase).toBe("idle");
    expect(host.getState().sessionId).toBe(1);
  });

  it("ブレークポイントの変更では破棄しない", async () => {
    const { host, workspace, controller } = setup(okScript(bpScript()));
    await controller.ensureSession();

    workspace.setBreakpoints([0]);
    await flush();

    expect(host.getState().phase).toBe("idle");
    expect(host.getState().sessionId).toBe(1);
  });
});

describe("RunController: ブレークポイント同期", () => {
  it("停止中のブレークポイント切替は現在のセッションへ置換コマンドとして送る", async () => {
    const { backend, workspace, controller } = setup(okScript(bpScript()));
    await controller.ensureSession();

    workspace.setBreakpoints([0, 2]);
    await flush();

    expect(backend.calls.at(-1)).toEqual({ method: "setBreakpoints", args: [{ sessionId: 1, offsets: [0, 2] }] });
  });

  it("未解決の位置は応答どおり設定から外し、同じ集合を再送しない(冪等)", async () => {
    const { backend, workspace, controller } = setup(okScript(bpScript([5])));
    await controller.ensureSession();

    workspace.setBreakpoints([0, 5]);
    await flush();

    expect(workspace.getSnapshot().breakpoints).toEqual([0]);
    expect(methods(backend).filter((m) => m === "setBreakpoints")).toHaveLength(1);
  });

  it("開始時に未解決の位置は設定から外す", async () => {
    const { backend, workspace, controller } = setup(startWithUnresolved([5]));
    workspace.setBreakpoints([5]);

    await controller.ensureSession();
    await flush();

    expect(workspace.getSnapshot().breakpoints).toEqual([]);
    expect(methods(backend)).toEqual(["loadSource", "startSession"]);
  });

  it("置換で sessionNotFound が返れば未開始へ戻り通知を出す", async () => {
    const { host, workspace, controller } = setup(okScript({ setBreakpoints: () => Promise.reject(SESSION_ERR) }));
    await controller.ensureSession();

    workspace.setBreakpoints([0]);
    await flush();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([SESSION_ERR]);
  });
});

describe("RunController: テープ窓の再取得", () => {
  it("半径の変更でセッションを破棄せず、窓だけを取り直して反映する", async () => {
    const { backend, workspace, host, controller } = setup(okScript({ getTapeWindow: (req) => windowOf(req.radius) }));
    await controller.ensureSession();

    workspace.setTapeRadius(3);
    await flush();

    expect(host.getState().phase).toBe("idle");
    expect(host.getState().sessionId).toBe(1);
    expect(backend.calls.at(-1)).toEqual({ method: "getTapeWindow", args: [{ sessionId: 1, radius: 3 }] });
    expect(host.getState().state?.tapeWindow).toEqual(windowOf(3));
  });

  it("窓の取得で sessionNotFound が返れば未開始へ戻り通知を出す", async () => {
    const { host, workspace, controller } = setup(okScript({ getTapeWindow: () => Promise.reject(SESSION_ERR) }));
    await controller.ensureSession();

    workspace.setTapeRadius(3);
    await flush();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([SESSION_ERR]);
  });
});

describe("RunController: 古い応答の排除", () => {
  it("開始の途中で編集されたら、後から届いた開始の応答を反映しない", async () => {
    const gate = deferred<SessionStartedDto>();
    const { host, workspace, controller } = setup(okScript({ startSession: () => gate.promise }));

    const pending = controller.ensureSession();
    await flush();
    workspace.setSource("+.");
    gate.resolve({ state: stateDto(1, 0), breakpoints: { breakpoints: [], unresolved: [] } });

    await expect(pending).resolves.toBe(false);
    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().sessionId).toBeNull();
  });

  it("Step の途中で編集されたら、後から届いた Step の応答を反映しない", async () => {
    const gate = deferred<StepResultDto>();
    const { host, workspace, controller } = setup(okScript({ step: () => gate.promise }));

    const pending = controller.step();
    await flush();
    workspace.setSource("+.");
    gate.resolve({ outcome: { kind: "executed" }, state: stateDto(1, 1) });
    await pending;

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().state).toBeNull();
  });

  it("窓の取得の途中で編集されたら、後から届いた窓の応答を反映しない", async () => {
    const gate = deferred<TapeWindowDto>();
    const { host, workspace, controller } = setup(okScript({ getTapeWindow: () => gate.promise }));
    await controller.ensureSession();

    workspace.setTapeRadius(3);
    await flush();
    workspace.setSource("+.");
    gate.resolve(windowOf(3));
    await flush();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().state).toBeNull();
  });
});

describe("RunController: 無効化の後に届いた拒否(エラー)応答", () => {
  it("開始の途中の読み込みの拒否(IO エラー)が編集の後に届けば、通知を残さず未開始のまま", async () => {
    const gate = deferred<LoadSourceResult>();
    const { host, workspace, controller } = setup(okScript({ loadSource: () => gate.promise }));

    const pending = controller.ensureSession();
    await flush();
    workspace.setSource("+.");
    gate.reject(IO_ERR);

    await expect(pending).resolves.toBe(false);
    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([]);
  });

  it("Step の拒否(IO エラー)が無効化の後に届けば、通知を残さず未開始のまま", async () => {
    const gate = deferred<StepResultDto>();
    const { host, workspace, controller } = setup(okScript({ step: () => gate.promise }));
    await controller.ensureSession();

    const pending = controller.step();
    await flush();
    workspace.setSource("+.");
    gate.reject(IO_ERR);
    await pending;

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([]);
    expect(host.getState().error).toBeNull();
  });

  it("Reset の拒否(IO エラー)が無効化の後に届けば、通知を残さず未開始のまま", async () => {
    const gate = deferred<SessionStateDto>();
    const { host, workspace, controller } = setup(okScript({ reset: () => gate.promise }));
    await controller.ensureSession();

    const pending = controller.reset();
    await flush();
    workspace.setSource("+.");
    gate.reject(IO_ERR);
    await pending;

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([]);
  });

  it("窓の再取得の拒否(sessionNotFound)が無効化の後に届けば、通知を残さず未開始のまま", async () => {
    const gate = deferred<TapeWindowDto>();
    const { host, workspace, controller } = setup(okScript({ getTapeWindow: () => gate.promise }));
    await controller.ensureSession();

    workspace.setTapeRadius(3);
    await flush();
    workspace.setSource("+.");
    gate.reject(SESSION_ERR);
    await flush();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([]);
  });

  it("ブレークポイント置換の拒否(sessionNotFound)が無効化の後に届けば、通知を残さず未開始のまま", async () => {
    const gate = deferred<BreakpointsDto>();
    const { host, workspace, controller } = setup(okScript({ setBreakpoints: () => gate.promise }));
    await controller.ensureSession();

    workspace.setBreakpoints([0]);
    await flush();
    workspace.setSource("+.");
    gate.reject(SESSION_ERR);
    await flush();

    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().notices).toEqual([]);
  });
});

describe("RunController: 出力モードの切替", () => {
  it("出力モードの切替ではセッションを破棄せず、backend も呼ばない", async () => {
    const { backend, workspace, host, controller } = setup();
    await controller.ensureSession();
    const callCount = backend.calls.length;

    workspace.setOutputMode("hex");
    await flush();

    expect(host.getState().phase).toBe("idle");
    expect(host.getState().sessionId).toBe(1);
    expect(backend.calls).toHaveLength(callCount);
  });
});

const INTERNAL_ERR: ApiError = { kind: "internal", message: "temporary", field: null };
const WINDOW: TapeWindowDto = { start: -5, cells: Array.from({ length: 11 }, () => 0), pointerOffset: 5 };

/** 手動のフレーム: 描画の要求を溜め、テストが flush() で発火させる(ループはフレームを待たない) */
function manualFrames() {
  let queue: Array<() => void> = [];
  return {
    request(callback: () => void): () => void {
      queue.push(callback);
      return () => {
        queue = queue.filter((pending) => pending !== callback);
      };
    },
    flush(): void {
      const pending = queue;
      queue = [];
      for (const callback of pending) {
        callback();
      }
    },
    get pending(): number {
      return queue.length;
    },
  };
}

/** 呼ばれるたびに与えた値を順に返す時計(経過時間を決定的にする) */
function scriptedClock(values: readonly number[]): () => number {
  let index = 0;
  return () => {
    const value = values[index];
    index += 1;
    if (value === undefined) {
      throw new Error("clock exhausted");
    }
    return value;
  };
}

/** バッチの応答(セッション 1)。`halted` の停止のときだけ停止状態の DTO にする */
function batchDto(steps: number, stop: BatchStopDto, output?: OutputChunkDto): BatchResultDto {
  const base = stateDto(1, steps, stop.kind === "halted");
  return { stop, executed: 1, state: output === undefined ? base : { ...base, output } };
}

/** Run 系の台本: Pause とテープ窓の既定応答を足す */
function runScript(overrides: FakeScript = {}): FakeScript {
  return okScript({ pause: () => undefined, getTapeWindow: () => WINDOW, ...overrides });
}

/** Run 系の検査用セットアップ(手動フレーム・注入可能な時計) */
function runSetup(overrides: FakeScript = {}, now: () => number = () => 0) {
  const frames = manualFrames();
  const script = runScript(overrides);
  const responder = script.runBatch;
  let calls = 0;
  // 安全弁: ループが暴走しても無限に発行せず、上限を超えたら失敗させる(ワーカーが積み続けて停止しないため)
  script.runBatch = (req) => {
    calls += 1;
    if (calls > MAX_RUN_BATCH_CALLS) {
      throw new Error(`runBatch が安全弁の上限 ${MAX_RUN_BATCH_CALLS} 回を超えた`);
    }
    if (responder === undefined) {
      throw new Error("runBatch の台本がない");
    }
    return responder(req);
  };
  return { ...setup(script, { now, frames }), frames };
}

/** 安全弁: 1 テストで許す runBatch の最大呼び出し数 */
const MAX_RUN_BATCH_CALLS = 200;

/** 全出力 `all` のうち [from, to) を、要求の outputFrom から始まる差分チャンクにする(実バックエンドと同じ形) */
function outputChunk(all: Uint8Array, from: number, to: number): OutputChunkDto {
  return { dataStart: from, totalLen: all.length, dataBase64: encodeBase64(all.slice(from, to)) };
}

describe("RunController: Run ループ", () => {
  it("バッチ到達の間は自動で続け、halted で止まる(描画を待たず、状態の取得も発行しない)", async () => {
    const responses = [
      batchDto(1000, { kind: "batchReached" }),
      batchDto(2000, { kind: "batchReached" }),
      batchDto(2500, { kind: "halted" }),
    ];
    const { backend, host, controller } = runSetup({ runBatch: () => responses.shift() as BatchResultDto });

    await controller.run();

    expect(methods(backend).filter((method) => method === "runBatch")).toHaveLength(3);
    expect(methods(backend)).not.toContain("getSnapshot");
    expect(host.getState().phase).toBe("halted");
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(2500);
    expect(host.getState().localPaused).toBe(false);
  });

  it("ブレークポイントで止まると待機へ戻り、Run をもう一度発行すると続きを要求する", async () => {
    const responses = [batchDto(10, { kind: "breakpoint" }), batchDto(20, { kind: "halted" })];
    const { backend, host, controller } = runSetup({ runBatch: () => responses.shift() as BatchResultDto });

    await controller.run();
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().error).toBeNull();

    await controller.run();
    expect(methods(backend).filter((method) => method === "runBatch")).toHaveLength(2);
    expect(host.getState().phase).toBe("halted");
  });

  it("実行エラーで止まると failed になり、実行エラーとして保持する", async () => {
    const error = { kind: "stepLimitExceeded" as const, limit: 5000, pc: 3 };
    const { host, controller } = runSetup({
      runBatch: () => batchDto(5000, { kind: "error", error }),
    });

    await controller.run();

    expect(host.getState().phase).toBe("failed");
    expect(host.getState().error).toEqual({ category: "runtime", error });
  });

  it("停止済み(halted)の画面では Run を発行しない", async () => {
    const { backend, controller } = runSetup({ runBatch: () => batchDto(1, { kind: "halted" }) });

    await controller.run();
    await controller.run();

    expect(methods(backend).filter((method) => method === "runBatch")).toHaveLength(1);
  });

  it("バッチ幅は往復時間で適応する(15 ms 未満で倍加、60 ms 超で半減、その間は据え置き)", async () => {
    const sizes: number[] = [];
    const responses = [
      batchDto(1000, { kind: "batchReached" }),
      batchDto(2000, { kind: "batchReached" }),
      batchDto(3000, { kind: "batchReached" }),
      batchDto(4000, { kind: "halted" }),
    ];
    const { controller } = runSetup(
      {
        runBatch: (req: RunBatchRequest) => {
          sizes.push(req.batchSize ?? -1);
          return responses.shift() as BatchResultDto;
        },
      },
      // 1 バッチごとに (開始, 終了) の順で読む: 10 ms → 100 ms → 30 ms → 0 ms
      scriptedClock([0, 10, 0, 100, 0, 30, 0, 0]),
    );

    await controller.run();

    expect(sizes).toEqual([1000, 2000, 1000, 1000]);
  });

  it("各バッチで現在のテープ半径を渡す(Run の途中で変えた値が次のバッチに反映される)", async () => {
    const radii: number[] = [];
    const ctx: ReturnType<typeof runSetup> = runSetup({
      runBatch: (req: RunBatchRequest) => {
        radii.push(req.view.tapeRadius);
        if (radii.length === 1) {
          ctx.workspace.setTapeRadius(5);
          return batchDto(1000, { kind: "batchReached" });
        }
        return batchDto(2000, { kind: "halted" });
      },
    });

    await ctx.controller.run();

    expect(radii).toEqual([16, 5]);
  });

  it("表示は描画フレームで間引く: 途中の応答は省き、フレームで最新の応答を反映し、停止時は最終応答を必ず反映する", async () => {
    let stepsBeforeFrame: number | undefined;
    let stepsAfterFrame: number | undefined;
    const ctx: ReturnType<typeof runSetup> = runSetup({
      runBatch: (() => {
        let call = 0;
        return () => {
          call += 1;
          if (call === 1) return batchDto(1000, { kind: "batchReached" });
          if (call === 2) {
            // フレームがまだ来ていないため、表示は開始時の状態のまま
            stepsBeforeFrame = ctx.host.getState().state?.snapshot.stepsExecuted;
            return batchDto(2000, { kind: "batchReached" });
          }
          if (call === 3) {
            // フレームで届いた時点の最新(2 回目の応答)が反映される
            ctx.frames.flush();
            stepsAfterFrame = ctx.host.getState().state?.snapshot.stepsExecuted;
            return batchDto(3000, { kind: "batchReached" });
          }
          return batchDto(4000, { kind: "halted" });
        };
      })(),
    });

    await ctx.controller.run();

    expect(stepsBeforeFrame).toBe(0);
    expect(stepsAfterFrame).toBe(2000);
    expect(ctx.host.getState().state?.snapshot.stepsExecuted).toBe(4000);
    // 反映は「フレーム 1 回 + 停止時の最終応答 1 回」の 2 回だけ
    expect(ctx.host.dispatched.filter((action) => action.type === "batchResult")).toHaveLength(2);
    expect(ctx.frames.pending).toBe(0);
  });

  it("描画フレームが実行中バッチと重なっても出力の先頭を失わない(未反映の応答を含む末尾から要求する)", async () => {
    const ALL = Uint8Array.of(65, 66, 67, 68);
    const outputFrom: number[] = [];
    const ctx: ReturnType<typeof runSetup> = runSetup({
      runBatch: (req: RunBatchRequest) => {
        const from = req.view.outputFrom;
        outputFrom.push(from);
        if (outputFrom.length === 1) {
          return batchDto(1000, { kind: "batchReached" }, outputChunk(ALL, 0, 2));
        }
        if (outputFrom.length === 2) {
          // 要求の後・応答の前に描画フレームが来る: 1 回目の応答(未反映)が画面へ反映され、画面の末尾は 2 になる
          ctx.frames.flush();
          return batchDto(2000, { kind: "batchReached" }, outputChunk(ALL, from, 3));
        }
        return batchDto(3000, { kind: "halted" }, outputChunk(ALL, from, 4));
      },
    });

    await ctx.controller.run();

    // 2 回目は画面の末尾(0)ではなく、受理済みの末尾(2)から要求する。3 回目は受理済みの末尾 3 から要求する
    expect(outputFrom).toEqual([0, 2, 3]);
    expect(ctx.host.getState().output.bytes).toEqual(ALL);
    expect(ctx.host.getState().output.bufferStart).toBe(0);
    expect(ctx.host.getState().output.totalLen).toBe(4);
    expect(ctx.host.getState().output.truncated).toBe(false);
  });

  it("runBatch の失敗の直前に未反映の応答があれば、その応答を反映してから失敗を出す", async () => {
    const ALL = Uint8Array.of(65, 66, 67);
    const ctx: ReturnType<typeof runSetup> = runSetup({
      runBatch: (() => {
        let call = 0;
        return (req: RunBatchRequest) => {
          call += 1;
          if (call === 1) {
            return batchDto(1000, { kind: "batchReached" }, outputChunk(ALL, req.view.outputFrom, 2));
          }
          // 1 回目の応答は描画フレームが来ていないため未反映のまま
          return Promise.reject(IO_ERR);
        };
      })(),
    });

    await ctx.controller.run();

    expect(ctx.frames.pending).toBe(0);
    expect(ctx.host.getState().output.bytes).toEqual(Uint8Array.of(65, 66));
    expect(ctx.host.getState().state?.snapshot.stepsExecuted).toBe(1000);
    expect(ctx.host.getState().notices).toEqual([IO_ERR]);
    expect(ctx.host.getState().phase).toBe("idle");
  });

  it("出力の差分は受理済みの末尾から要求し、停止時に総バイト数まで揃える(表示の末尾が遅れていても)", async () => {
    const ALL = Uint8Array.of(65, 66, 67, 68, 69);
    const outputFrom: number[] = [];
    const ctx: ReturnType<typeof runSetup> = runSetup({
      runBatch: (req: RunBatchRequest) => {
        outputFrom.push(req.view.outputFrom);
        if (outputFrom.length === 1) {
          return batchDto(1000, { kind: "batchReached" }, outputChunk(ALL, 0, 2));
        }
        if (outputFrom.length === 2) {
          return batchDto(2000, { kind: "batchReached" }, outputChunk(ALL, req.view.outputFrom, 4));
        }
        return batchDto(3000, { kind: "halted" }, outputChunk(ALL, req.view.outputFrom, 5));
      },
    });

    await ctx.controller.run();

    // フレームが来ないため、画面の末尾は 0 のままでも、要求は受理済みの末尾 2 → 4 と進む
    expect(outputFrom).toEqual([0, 2, 4]);
    expect(ctx.host.getState().output.bytes).toEqual(ALL);
    expect(ctx.host.getState().output.totalLen).toBe(5);
    expect(ctx.host.getState().output.end).toBe(5);
  });
});

describe("RunController: Pause", () => {
  it("実行中バッチの途中で Pause すると、局所フラグを立てて pause を直接呼び、cancelled で一時停止になる", async () => {
    const gate = deferred<BatchResultDto>();
    const { backend, host, controller } = runSetup({ runBatch: () => gate.promise });

    const running = controller.run();
    await flush();
    expect(host.getState().phase).toBe("running");

    const paused = controller.pause();
    expect(host.getState().phase).toBe("pausing");
    expect(host.getState().localPaused).toBe(true);
    await paused;
    expect(backend.calls.at(-1)).toEqual({ method: "pause", args: [{ sessionId: 1 }] });

    gate.resolve(batchDto(500, { kind: "cancelled" }));
    await running;

    expect(host.getState().phase).toBe("idle");
    expect(host.getState().localPaused).toBe(true);
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(500);
    expect(methods(backend).filter((method) => method === "runBatch")).toHaveLength(1);
  });

  it("Pause が届いた後に batchReached で戻った場合は次のバッチを発行せず、一時停止で止まる", async () => {
    const gate = deferred<BatchResultDto>();
    const { backend, host, controller } = runSetup({ runBatch: () => gate.promise });

    const running = controller.run();
    await flush();
    void controller.pause();
    gate.resolve(batchDto(1000, { kind: "batchReached" }));
    await running;

    expect(methods(backend).filter((method) => method === "runBatch")).toHaveLength(1);
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().localPaused).toBe(true);
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(1000);
  });

  it("Pause 後の Run は停止位置から続きを要求し、セッションを開始し直さない", async () => {
    const gate = deferred<BatchResultDto>();
    let call = 0;
    const { backend, host, controller } = runSetup({
      runBatch: () => {
        call += 1;
        return call === 1 ? gate.promise : batchDto(900, { kind: "halted" });
      },
    });

    const running = controller.run();
    await flush();
    void controller.pause();
    gate.resolve(batchDto(500, { kind: "cancelled" }));
    await running;

    await controller.run();

    expect(methods(backend).filter((method) => method === "startSession")).toHaveLength(1);
    expect(methods(backend).filter((method) => method === "runBatch")).toHaveLength(2);
    expect(host.getState().phase).toBe("halted");
    expect(host.getState().localPaused).toBe(false);
  });

  it("Pause の RPC が失敗しても、実行中のバッチが戻って止まるまで失敗を示さず、止まった時点で最終応答の反映の後に示す", async () => {
    const ALL = Uint8Array.of(65, 66);
    const gate = deferred<BatchResultDto>();
    let call = 0;
    const { host, controller } = runSetup({
      pause: () => Promise.reject(IO_ERR),
      runBatch: (req: RunBatchRequest) => {
        call += 1;
        return call === 1 ? gate.promise : batchDto(9999, { kind: "halted" }, outputChunk(ALL, req.view.outputFrom, 2));
      },
    });

    const running = controller.run();
    await flush();
    await controller.pause();

    // バッチの途中では失敗を示さない(一時停止処理中のまま)
    expect(host.getState().phase).toBe("pausing");
    expect(host.getState().notices).toEqual([]);

    gate.resolve(batchDto(1000, { kind: "batchReached" }, outputChunk(ALL, 0, 2)));
    await running;

    // 次のバッチは発行されず、最終応答(出力を含む)が反映されてから失敗が示される
    expect(call).toBe(1);
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(1000);
    expect(host.getState().output.bytes).toEqual(ALL);
    expect(host.getState().notices).toEqual([IO_ERR]);
  });

  it("dispose は予約中の描画を取り消し、取り消し後に届いた応答を画面へ反映せず、次のバッチも発行しない", async () => {
    const gate = deferred<BatchResultDto>();
    let call = 0;
    const { host, controller, frames } = runSetup({
      runBatch: () => {
        call += 1;
        return call === 1 ? batchDto(1000, { kind: "batchReached" }) : gate.promise;
      },
    });

    const running = controller.run();
    await flush();
    expect(frames.pending).toBe(1);

    controller.dispose();
    expect(frames.pending).toBe(0);

    // 破棄の後に届いた応答: 画面へは反映せず、ループも次のバッチを発行しない
    gate.resolve(batchDto(2000, { kind: "batchReached" }));
    await running;
    // 破棄の後は描画を予約し直さない
    expect(frames.pending).toBe(0);
    frames.flush();

    expect(call).toBe(2);
    expect(host.getState().state?.snapshot.stepsExecuted).toBe(0);
    expect(host.dispatched.filter((action) => action.type === "batchResult")).toHaveLength(0);
  });

  it("待機中の Pause は何も発行しない", async () => {
    const { backend, controller } = runSetup();
    await controller.ensureSession();
    const callCount = backend.calls.length;

    await controller.pause();

    expect(backend.calls).toHaveLength(callCount);
  });
});

describe("RunController: 実行中の操作失敗(failOperation の実行中分岐)", () => {
  it("実行中の IO エラーは待機へ戻し、局所フラグを下ろし、通知を残してセッションを保つ", async () => {
    const { host, controller } = runSetup({ runBatch: () => Promise.reject(IO_ERR) });
    await controller.ensureSession();
    const before = host.dispatched.length;

    await controller.run();

    expect(host.dispatched.slice(before).map((action) => action.type)).toEqual(["runStarted", "operationFailed"]);
    expect(host.getState().phase).toBe("idle");
    expect(host.getState().localPaused).toBe(false);
    expect(host.getState().notices).toEqual([IO_ERR]);
    expect(host.getState().sessionId).toBe(1);
  });

  it("実行中に sessionNotFound が返ると、操作失敗 → 無効化 → 操作失敗 の順で未開始へ戻る", async () => {
    const { host, controller } = runSetup({ runBatch: () => Promise.reject(SESSION_ERR) });
    await controller.ensureSession();
    const before = host.dispatched.length;

    await controller.run();

    expect(host.dispatched.slice(before).map((action) => action.type)).toEqual([
      "runStarted",
      "operationFailed",
      "invalidated",
      "operationFailed",
    ]);
    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().sessionId).toBeNull();
    expect(host.getState().notices).toEqual([SESSION_ERR]);
  });

  it("一時停止処理中(pausing)に sessionNotFound が返っても、同じ順で未開始へ戻る", async () => {
    const gate = deferred<BatchResultDto>();
    const { host, controller } = runSetup({ runBatch: () => gate.promise });
    await controller.ensureSession();
    const before = host.dispatched.length;

    const running = controller.run();
    await flush();
    const paused = controller.pause();
    gate.reject(SESSION_ERR);
    await Promise.all([running, paused]);

    expect(host.dispatched.slice(before).map((action) => action.type)).toEqual([
      "runStarted",
      "pauseRequested",
      "operationFailed",
      "invalidated",
      "operationFailed",
    ]);
    expect(host.getState().phase).toBe("noSession");
    expect(host.getState().localPaused).toBe(false);
  });
});

describe("RunController: Run 開始時のブレークポイント同期", () => {
  it("同期が一時的に失敗しても、次の Run の前に同じ集合を再送してから実行する", async () => {
    let syncCalls = 0;
    const { backend, workspace, controller } = runSetup({
      setBreakpoints: () => {
        syncCalls += 1;
        if (syncCalls === 1) {
          return Promise.reject(INTERNAL_ERR);
        }
        return { breakpoints: [], unresolved: [] };
      },
      runBatch: () => batchDto(1000, { kind: "halted" }),
    });
    await controller.ensureSession();

    workspace.setBreakpoints([0]);
    await flush();
    await controller.run();

    expect(methods(backend).slice(2)).toEqual(["setBreakpoints", "setBreakpoints", "runBatch"]);
    expect(backend.calls.at(-2)).toEqual({ method: "setBreakpoints", args: [{ sessionId: 1, offsets: [0] }] });
  });
});

describe("RunController: 実行中の文書置換と開始処理の直列化(最終検証の修正)", () => {
  it("実行中に文書を置換しても表示は変えず、停止のあと未開始へ戻る(古いプログラムのセッションを残さない)", async () => {
    const gate = deferred<BatchResultDto>();
    const { backend, workspace, host, controller } = runSetup({ runBatch: () => gate.promise });
    const running = controller.run();
    await flush();
    expect(host.getState().phase).toBe("running");

    workspace.replaceDocument({ source: "+.", fileName: "b.bf", lossy: false });
    expect(host.getState().phase).toBe("running");

    gate.resolve(batchDto(10, { kind: "breakpoint" }));
    await running;

    expect(host.getState()).toMatchObject({ phase: "noSession", sessionId: null, programId: null });
    await controller.step();
    expect(methods(backend).filter((method) => method === "loadSource")).toHaveLength(2);
  });

  it.each([
    ["ソース", (w: WorkspaceStore) => w.setSource("+.")],
    ["入力", (w: WorkspaceStore) => w.setInput({ mode: "text", text: "A" })],
    ["有効な上限", (w: WorkspaceStore) => w.setLimitText("42")],
  ])("実行中の%sの変更も、停止のあと未開始へ戻る", async (_label, mutate) => {
    const gate = deferred<BatchResultDto>();
    const { workspace, host, controller } = runSetup({ runBatch: () => gate.promise });
    const running = controller.run();
    await flush();

    mutate(workspace);
    gate.resolve(batchDto(10, { kind: "cancelled" }));
    await running;

    expect(host.getState()).toMatchObject({ phase: "noSession", sessionId: null });
  });

  it("開始処理の往復中に文書を置換すると、開始は破棄され、古いプログラムのセッションは残らない", async () => {
    const start = deferred<SessionStartedDto>();
    const { backend, workspace, host, controller } = runSetup({ startSession: () => start.promise });
    const running = controller.run();
    await flush();
    expect(methods(backend)).toContain("startSession");

    workspace.replaceDocument({ source: "+.", fileName: "b.bf", lossy: false });
    start.resolve({ state: stateDto(1, 0), breakpoints: { breakpoints: [], unresolved: [] } });
    await running;

    expect(host.getState()).toMatchObject({ phase: "noSession", sessionId: null });
    expect(methods(backend)).not.toContain("runBatch");
  });

  it("Run の二重押しは 1 回にまとめ、停止位置の要求を積まない", async () => {
    const responses = [batchDto(10, { kind: "breakpoint" })];
    const { backend, controller } = runSetup({
      runBatch: () => responses.shift() ?? batchDto(10, { kind: "halted" }),
    });

    await Promise.all([controller.run(), controller.run()]);

    expect(methods(backend).filter((method) => method === "startSession")).toHaveLength(1);
    expect(methods(backend).filter((method) => method === "runBatch")).toHaveLength(1);
  });

  it("dispose の後にキューに残った Step は IPC を発行しない", async () => {
    const gate = deferred<StepResultDto>();
    const { backend, controller } = setup(okScript({ step: () => gate.promise }));
    await controller.ensureSession();
    const first = controller.step();
    await flush();
    const queued = controller.step();

    controller.dispose();
    gate.resolve({ outcome: { kind: "executed" }, state: stateDto(1, 1) });
    await first;
    await queued;

    expect(methods(backend).filter((method) => method === "step")).toHaveLength(1);
  });

  it("dispose の後の Run は何も発行しない", async () => {
    const { backend, controller } = runSetup();

    controller.dispose();
    await controller.run();

    expect(backend.calls).toEqual([]);
  });

  it("canStart が false の間(ファイル操作の進行中)は Run・Step を開始せず、何も発行しない", async () => {
    const { backend, controller } = setup(runScript(), { canStart: () => false });

    await controller.run();
    await controller.step();

    expect(backend.calls).toEqual([]);
  });

  it("実行の途中は isBusy が true で、停止すると false に戻る", async () => {
    const gate = deferred<BatchResultDto>();
    const { controller } = runSetup({ runBatch: () => gate.promise });
    expect(controller.isBusy()).toBe(false);

    const running = controller.run();
    await flush();
    expect(controller.isBusy()).toBe(true);

    gate.resolve(batchDto(10, { kind: "breakpoint" }));
    await running;
    expect(controller.isBusy()).toBe(false);
  });
});

describe("RunController: 開始の拒否後も次の Run が動く・開始の直列化の回帰(9.6 レビュー指摘)", () => {
  it("canStart が false の Run の後でも、canStart が true になれば次の Run は開始する", async () => {
    let allowed = false;
    const frames = manualFrames();
    const { backend, controller } = setup(runScript({ runBatch: () => batchDto(10, { kind: "halted" }) }), {
      canStart: () => allowed,
      frames,
    });

    await controller.run();
    expect(methods(backend)).toEqual([]);

    allowed = true;
    await controller.run();

    expect(methods(backend)).toContain("startSession");
    expect(methods(backend)).toContain("runBatch");
  });

  it("canStart が false の間は ensureSession も IPC を発行しない", async () => {
    const { backend, controller } = setup(okScript(), { canStart: () => false });

    await expect(controller.ensureSession()).resolves.toBe(false);

    expect(backend.calls).toEqual([]);
  });

  it("Step の開始処理(読み込み)の往復中は isBusy が true で、応答の後に false に戻る", async () => {
    const load = deferred<LoadSourceResult>();
    const { controller } = setup(okScript({ loadSource: () => load.promise }));

    const stepping = controller.step();
    await flush();
    expect(controller.isBusy()).toBe(true);

    load.resolve(OK_LOAD);
    await stepping;
    expect(controller.isBusy()).toBe(false);
  });
});
