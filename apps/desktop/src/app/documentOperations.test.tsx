import { StrictMode } from "react";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { OpenFileResult } from "../generated/OpenFileResult";
import type { SaveFileResult } from "../generated/SaveFileResult";
import type { SessionStartedDto } from "../generated/SessionStartedDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import { encodeBase64 } from "../domain/base64";
import { createFakeBackend, type CommandMethod, type FakeBackend, type FakeScript } from "../ipc/fakeBackend";
import { createWorkspace, type WorkspaceStore } from "../state/workspace";
import { DebuggerScreen } from "./DebuggerScreen";

function stateDto(output = ""): SessionStateDto {
  const bytes = new TextEncoder().encode(output);
  return {
    sessionId: 1,
    snapshot: { pc: 0, dp: 0, currentCell: 0, stepsExecuted: 0, halted: false },
    tapeWindow: { start: 0, cells: Array.from({ length: 17 }, () => 0), pointerOffset: 0 },
    output: { dataStart: 0, totalLen: bytes.length, dataBase64: encodeBase64(bytes) },
    inputConsumed: 0,
    inputTotal: 0,
    lastStop: null,
    currentRange: null,
  };
}

/** 既定の台本: 保存・開く・終了・未保存フラグは成功する。Run は Hello World 相当を出力して停止する */
function script(overrides: FakeScript = {}): FakeScript {
  return {
    loadSource: () => ({ kind: "ok", programId: 7, instructionCount: 3, lineCount: 1 }),
    startSession: (): SessionStartedDto => ({
      state: stateDto(),
      breakpoints: { breakpoints: [], unresolved: [] },
    }),
    runBatch: (): BatchResultDto => {
      const halted = stateDto("Hi");
      return {
        stop: { kind: "halted" },
        executed: 3,
        state: { ...halted, snapshot: { ...halted.snapshot, halted: true }, lastStop: "halted" },
      };
    },
    setDocumentDirty: () => undefined,
    confirmExit: () => undefined,
    newDocument: () => undefined,
    saveSourceFile: (): SaveFileResult => ({ kind: "saved", fileName: "a.bf" }),
    openSourceFile: (): OpenFileResult => ({ kind: "cancelled" }),
    setBreakpoints: (req) => ({ breakpoints: [], unresolved: req.offsets }),
    ...overrides,
  };
}

function renderScreen(overrides: FakeScript = {}, options: { strict?: boolean } = {}) {
  const backend: FakeBackend = createFakeBackend(script(overrides));
  const workspace: WorkspaceStore = createWorkspace();
  const element = <DebuggerScreen backend={backend} workspace={workspace} />;
  const view = render(options.strict === true ? <StrictMode>{element}</StrictMode> : element);
  return { backend, workspace, ...view };
}

function countOf(backend: FakeBackend, method: CommandMethod): number {
  return backend.calls.filter((call) => call.method === method).length;
}

async function flush(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
}

/** ウィンドウ全体へキー入力を送る(操作ショートカットはウィンドウのバブル段階で受ける)。戻り値は既定動作を抑止したら false */
function press(init: KeyboardEventInit & { key: string }): boolean {
  return fireEvent.keyDown(window, init);
}

/** Run の応答: Hello World 相当を出力して停止する */
function haltedBatch(): BatchResultDto {
  const halted = stateDto("Hi");
  return {
    stop: { kind: "halted" },
    executed: 3,
    state: { ...halted, snapshot: { ...halted.snapshot, halted: true }, lastStop: "halted" },
  };
}

const dialog = () => screen.queryByRole("alertdialog");

describe("ショートカット: 文書操作(要件 2.5・2.8・2.9・14.1)", () => {
  it("Ctrl+S は保存を要求し、保存できたら紐づけ先のファイル名と未保存の解除を表示する", async () => {
    const { backend } = renderScreen();

    press({ key: "s", ctrlKey: true });
    await flush();

    expect(backend.calls.find((call) => call.method === "saveSourceFile")?.args[0]).toMatchObject({ saveAs: false });
    expect(screen.getByTestId("document-status").textContent).toBe("a.bf");
  });

  it("Ctrl+Shift+S は名前を付けて保存を要求する", async () => {
    const { backend } = renderScreen();

    press({ key: "S", ctrlKey: true, shiftKey: true });
    await flush();

    expect(backend.calls.find((call) => call.method === "saveSourceFile")?.args[0]).toMatchObject({ saveAs: true });
  });

  it("保存の取消では未保存の表示を維持する", async () => {
    const { workspace } = renderScreen({ saveSourceFile: () => ({ kind: "cancelled" }) });
    act(() => workspace.setSource("+"));

    press({ key: "s", ctrlKey: true });
    await flush();

    expect(screen.getByTestId("document-status").textContent).toContain("未保存の変更あり");
  });

  it("編集すると未保存の変更を表示する(要件 2.2)", async () => {
    const { workspace } = renderScreen();

    act(() => workspace.setSource("+"));

    expect(screen.getByTestId("document-status").textContent).toContain("未保存の変更あり");
  });

  it("Ctrl+O で開いた内容に置き換え、ファイル名を表示する", async () => {
    const { workspace } = renderScreen({
      openSourceFile: () => ({ kind: "opened", source: "+.\n", fileName: "hello.bf", lossy: false }),
    });

    press({ key: "o", ctrlKey: true });
    await flush();

    expect(workspace.getSnapshot().source).toBe("+.\n");
    expect(screen.getByTestId("document-status").textContent).toBe("hello.bf");
  });

  it("置換読み込みでは通知を出す(要件 2.12)", async () => {
    renderScreen({
      openSourceFile: () => ({ kind: "opened", source: "+.\n", fileName: "hello.bf", lossy: true }),
    });

    press({ key: "o", ctrlKey: true });
    await flush();

    expect(screen.getByRole("status", { name: "通知" }).textContent).toContain("置換文字");
  });
});

describe("ショートカット: 実行操作と無効な操作", () => {
  it("F5 は Run を発行し、正常終了を表示する", async () => {
    const { backend } = renderScreen();

    press({ key: "F5" });
    await flush();

    expect(countOf(backend, "runBatch")).toBe(1);
    expect(screen.getByTestId("status-label").textContent).toBe("正常終了");
  });

  it("待機していないときの F6(Pause)は発行しない(操作が無効な状態では発火しない)", async () => {
    const { backend } = renderScreen();

    press({ key: "F6" });
    await flush();

    expect(countOf(backend, "pause")).toBe(0);
  });

  it("修飾キーの付いた F5 は Run にならない", async () => {
    const { backend } = renderScreen();

    press({ key: "F5", ctrlKey: true });
    await flush();

    expect(countOf(backend, "runBatch")).toBe(0);
  });
});

describe("ショートカット: F9 のブレークポイント切替(二重切替の防止)", () => {
  it("エディタの外で F9 を押すと、カーソル以降の命令へ 1 回だけ切り替える", async () => {
    const { workspace } = renderScreen();

    press({ key: "F9" });
    await flush();

    expect(workspace.getSnapshot().breakpoints).toHaveLength(1);
  });

  it("エディタの内側で F9 を押しても、エディタのキーマップだけが処理し 1 回だけ切り替える", async () => {
    const { workspace, container } = renderScreen();
    const content = container.querySelector(".cm-content");
    if (!(content instanceof HTMLElement)) throw new Error("editor content not found");

    fireEvent.keyDown(content, { key: "F9" });
    await flush();

    expect(workspace.getSnapshot().breakpoints).toHaveLength(1);
  });

  it("テキスト入力欄で F9 を押しても、ブレークポイントは切り替えない", async () => {
    const { workspace } = renderScreen();
    const input = screen.getByRole("textbox", { name: "入力" });

    fireEvent.keyDown(input, { key: "F9" });
    await flush();

    expect(workspace.getSnapshot().breakpoints).toEqual([]);
  });
});

describe("未保存時の確認(新規・開く)(要件 2.7)", () => {
  it("未保存で Ctrl+N を押すと確認を出し、取消では文書を変えない", async () => {
    const { backend, workspace } = renderScreen();
    act(() => workspace.setSource("+"));

    press({ key: "n", ctrlKey: true });
    expect(dialog()).not.toBeNull();

    fireEvent.click(within(dialog() as HTMLElement).getByRole("button", { name: "取消" }));
    await flush();

    expect(dialog()).toBeNull();
    expect(countOf(backend, "newDocument")).toBe(0);
    expect(workspace.getSnapshot().source).toBe("+");
  });

  it("確認で破棄を確定すると、空の文書にして未保存を解除する", async () => {
    const { backend, workspace } = renderScreen();
    act(() => workspace.setSource("+"));

    press({ key: "n", ctrlKey: true });
    fireEvent.click(within(dialog() as HTMLElement).getByRole("button", { name: "破棄して新規" }));
    await flush();

    expect(countOf(backend, "newDocument")).toBe(1);
    expect(workspace.getSnapshot().source).toBe("");
    expect(screen.getByTestId("document-status").textContent).toBe("無題");
  });

  it("確認の表示中はショートカットを受けない(保存が走らない)", async () => {
    const { backend, workspace } = renderScreen();
    act(() => workspace.setSource("+"));

    press({ key: "o", ctrlKey: true });
    press({ key: "s", ctrlKey: true });
    await flush();

    expect(dialog()).not.toBeNull();
    expect(countOf(backend, "saveSourceFile")).toBe(0);
    expect(countOf(backend, "openSourceFile")).toBe(0);
  });
});

describe("ウィンドウを閉じる要求(要件 2.7)", () => {
  it("未保存のとき閉じる要求で確認を出し、確定で終了を確定させる", async () => {
    const { backend, workspace } = renderScreen();
    act(() => workspace.setSource("+"));
    await flush();

    act(() => backend.emitCloseRequested());
    fireEvent.click(within(dialog() as HTMLElement).getByRole("button", { name: "破棄して閉じる" }));
    await flush();

    expect(countOf(backend, "confirmExit")).toBe(1);
  });

  it("未保存でなければ、閉じる要求に確認なしで応答する", async () => {
    const { backend } = renderScreen();
    await flush();

    act(() => backend.emitCloseRequested());
    await flush();

    expect(dialog()).toBeNull();
    expect(countOf(backend, "confirmExit")).toBe(1);
  });

  it("アンマウント後は閉じる要求に応答しない(購読の解除)", async () => {
    const { backend, unmount } = renderScreen();
    await flush();
    unmount();

    backend.emitCloseRequested();
    await flush();

    expect(countOf(backend, "confirmExit")).toBe(0);
  });
});

describe("ライフサイクル", () => {
  it("StrictMode の二重マウントでも、保存の要求は 1 回だけ届く", async () => {
    const { backend } = renderScreen({}, { strict: true });

    press({ key: "s", ctrlKey: true });
    await flush();

    expect(countOf(backend, "saveSourceFile")).toBe(1);
  });
});

describe("ショートカット: 既定動作の抑止と押しっぱなし(要件 14.1)", () => {
  it("割り当て済みのキーは既定動作(ブラウザのリロード・ページの保存など)を抑止する", async () => {
    renderScreen();
    const keys: (KeyboardEventInit & { key: string })[] = [
      { key: "F5" },
      { key: "F6" },
      { key: "F9" },
      { key: "F11" },
      { key: "n", ctrlKey: true },
      { key: "o", ctrlKey: true },
      { key: "s", ctrlKey: true },
      { key: "S", ctrlKey: true, shiftKey: true },
    ];

    const results = keys.map((init) => press(init));
    await flush();

    expect(results).toEqual(keys.map(() => false));
  });

  it("押しっぱなし(repeat)の F5 と Ctrl+S は既定動作を抑止するが、操作は発火させない", async () => {
    const { backend } = renderScreen();

    expect(press({ key: "F5", repeat: true })).toBe(false);
    expect(press({ key: "s", ctrlKey: true, repeat: true })).toBe(false);
    await flush();

    expect(countOf(backend, "runBatch")).toBe(0);
    expect(countOf(backend, "saveSourceFile")).toBe(0);
  });

  it("リロードのキー(Ctrl+R・Ctrl+F5)は既定動作を抑止するが、操作は発火させない", async () => {
    const { backend } = renderScreen();

    expect(press({ key: "r", ctrlKey: true })).toBe(false);
    expect(press({ key: "F5", ctrlKey: true })).toBe(false);
    await flush();

    expect(countOf(backend, "runBatch")).toBe(0);
  });
});

describe("ショートカット: 操作が無効な状態では発火しない(要件 5.10・14.1)", () => {
  it("実行中は Ctrl+N・Ctrl+O・Ctrl+S・F9 を発火させない", async () => {
    let releaseRun: () => void = () => undefined;
    const { backend, workspace } = renderScreen({
      runBatch: () =>
        new Promise<BatchResultDto>((resolve) => {
          releaseRun = () => resolve(haltedBatch());
        }),
    });

    press({ key: "F5" });
    await flush();
    expect(countOf(backend, "runBatch")).toBe(1);

    press({ key: "n", ctrlKey: true });
    press({ key: "o", ctrlKey: true });
    press({ key: "s", ctrlKey: true });
    press({ key: "F9" });
    await flush();

    expect(countOf(backend, "newDocument")).toBe(0);
    expect(countOf(backend, "openSourceFile")).toBe(0);
    expect(countOf(backend, "saveSourceFile")).toBe(0);
    expect(workspace.getSnapshot().breakpoints).toEqual([]);

    releaseRun();
    await flush();
  });

  it("待機中は Pause(F6)を発行せず、実行中は発行する", async () => {
    let releaseRun: () => void = () => undefined;
    const { backend } = renderScreen({
      pause: () => undefined,
      runBatch: () =>
        new Promise<BatchResultDto>((resolve) => {
          releaseRun = () => resolve(haltedBatch());
        }),
    });

    press({ key: "F6" });
    await flush();
    expect(countOf(backend, "pause")).toBe(0);

    press({ key: "F5" });
    await flush();
    press({ key: "F6" });
    await flush();
    expect(countOf(backend, "pause")).toBe(1);

    releaseRun();
    await flush();
  });
});

describe("確認の表示中の画面側判定(要件 2.7)", () => {
  it("確認の表示中は F5・F11 で実行・ステップを発行しない", async () => {
    const { backend, workspace } = renderScreen();
    act(() => workspace.setSource("+"));

    press({ key: "n", ctrlKey: true });
    expect(dialog()).not.toBeNull();
    press({ key: "F5" });
    press({ key: "F11" });
    await flush();

    expect(countOf(backend, "runBatch")).toBe(0);
    expect(countOf(backend, "step")).toBe(0);
  });
});

describe("閉じる要求の購読(StrictMode でも解除の数が登録の数と一致する)", () => {
  it("アンマウントで購読をすべて解除し、以降の閉じる要求に応答しない", async () => {
    const backend: FakeBackend = createFakeBackend(script());
    let subscribed = 0;
    let released = 0;
    const original = backend.onCloseRequested;
    backend.onCloseRequested = async (handler) => {
      subscribed += 1;
      const unlisten = await original(handler);
      return () => {
        released += 1;
        unlisten();
      };
    };

    const view = render(
      <StrictMode>
        <DebuggerScreen backend={backend} workspace={createWorkspace()} />
      </StrictMode>,
    );
    await flush();
    view.unmount();
    await flush();

    expect(subscribed).toBeGreaterThan(0);
    expect(released).toBe(subscribed);

    backend.emitCloseRequested();
    await flush();
    expect(countOf(backend, "confirmExit")).toBe(0);
  });
});
