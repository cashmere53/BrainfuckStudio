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
import { createRunController } from "./runController";
import { createWorkspace, type WorkspaceStore } from "./workspace";

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

function setup(script: FakeScript = okScript()) {
  const backend = createFakeBackend(script);
  const workspace = createWorkspace();
  const host = makeHost();
  const controller = createRunController({ backend, workspace, host });
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

    workspace.markSaved("a.bf");

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
