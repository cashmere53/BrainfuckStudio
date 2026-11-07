import { StrictMode } from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { LoadSourceResult } from "../generated/LoadSourceResult";
import type { SaveFileResult } from "../generated/SaveFileResult";
import type { SessionStartedDto } from "../generated/SessionStartedDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import type { StepResultDto } from "../generated/StepResultDto";
import type { TapeWindowDto } from "../generated/TapeWindowDto";
import { encodeBase64 } from "../domain/base64";
import { createFakeBackend, type CommandMethod, type FakeBackend, type FakeScript } from "../ipc/fakeBackend";
import { EditorView } from "@codemirror/view";
import { toggleBreakpointAtLine } from "../editor/breakpoints";
import { getHelloSource } from "../samples/hello";
import { createWorkspace, type WorkspaceStore } from "../state/workspace";
import { DebuggerScreen } from "./DebuggerScreen";

// SourceEditor の描画を記録する。振る舞いは本物の SourceEditor のまま、受け取った display の
// scrollRequest を記録するだけ(停止時のスクロール要求が立ち、消費されて降りることを観測するため)。
const probe = vi.hoisted(() => ({ scrollRequests: [] as boolean[] }));
vi.mock("../components/SourceEditor", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../components/SourceEditor")>();
  const React = await import("react");
  const Recording = React.forwardRef<
    import("../components/SourceEditor").SourceEditorHandle,
    import("../components/SourceEditor").SourceEditorProps
  >((props, ref) => {
    probe.scrollRequests.push(props.display.scrollRequest);
    return React.createElement(actual.SourceEditor, { ...props, ref });
  });
  return { ...actual, SourceEditor: Recording };
});

const HELLO_OUTPUT = "Hello World!\n";
const HELLO_STEPS = 906;

/** 状態 DTO を作る。出力は `output` を全量として差分 0 から載せる */
function stateDto(
  stepsExecuted: number,
  options: { halted?: boolean; lastStop?: SessionStateDto["lastStop"]; output?: string } = {},
): SessionStateDto {
  const bytes = new TextEncoder().encode(options.output ?? "");
  return {
    sessionId: 1,
    snapshot: { pc: stepsExecuted, dp: 0, currentCell: 0, stepsExecuted, halted: options.halted ?? false },
    tapeWindow: { start: 0, cells: Array.from({ length: 17 }, () => 0), pointerOffset: 0 },
    output: { dataStart: 0, totalLen: bytes.length, dataBase64: encodeBase64(bytes) },
    inputConsumed: 0,
    inputTotal: 0,
    lastStop: options.lastStop ?? null,
    currentRange: null,
  };
}

const WINDOW: TapeWindowDto = { start: 0, cells: Array.from({ length: 17 }, () => 0), pointerOffset: 0 };

/** 既定の台本: 読み込み成功 → セッション 1 → Run は Hello World を出力して正常終了する */
function helloScript(overrides: FakeScript = {}): FakeScript {
  return {
    loadSource: () => ({ kind: "ok", programId: 7, instructionCount: 3, lineCount: 1 }),
    startSession: (): SessionStartedDto => ({
      state: stateDto(0),
      breakpoints: { breakpoints: [], unresolved: [] },
    }),
    runBatch: (): BatchResultDto => ({
      stop: { kind: "halted" },
      executed: HELLO_STEPS,
      state: stateDto(HELLO_STEPS, { halted: true, lastStop: "halted", output: HELLO_OUTPUT }),
    }),
    step: (): StepResultDto => ({ outcome: { kind: "executed" }, state: stateDto(1) }),
    reset: () => stateDto(0),
    getTapeWindow: () => WINDOW,
    pause: () => undefined,
    setBreakpoints: (req) => ({ breakpoints: [], unresolved: req.offsets }),
    ...overrides,
  };
}

function renderScreen(script: FakeScript = helloScript(), options: { strict?: boolean } = {}) {
  const backend = createFakeBackend(script);
  const workspace: WorkspaceStore = createWorkspace();
  const element = <DebuggerScreen backend={backend} workspace={workspace} />;
  const view = render(options.strict === true ? <StrictMode>{element}</StrictMode> : element);
  return { backend, workspace, ...view };
}

/** 呼び出し回数(メソッド別) */
function countOf(backend: FakeBackend, method: CommandMethod): number {
  return backend.calls.filter((call) => call.method === method).length;
}

/** 非同期に発行される呼び出し(キューの後続など)が届くまで待つ */
async function flush(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
}

const runButton = () => screen.getByRole("button", { name: /^Run/ });
const stepButton = () => screen.getByRole("button", { name: /^Step/ });
const resetButton = () => screen.getByRole("button", { name: /^Reset/ });
const pauseButton = () => screen.getByRole("button", { name: /^Pause/ });

beforeEach(() => {
  probe.scrollRequests.length = 0;
});

describe("DebuggerScreen: 起動直後の表示(要件 1.1・1.3・1.4)", () => {
  it("5 領域(操作・ソース・入力・テープ・出力・状態)を同時に表示する", () => {
    renderScreen();

    expect(screen.getByRole("toolbar", { name: "操作" })).toBeTruthy();
    expect(screen.getByRole("textbox", { name: "ソース" })).toBeTruthy();
    expect(screen.getByRole("textbox", { name: "入力" })).toBeTruthy();
    expect(screen.getByLabelText("テープ")).toBeTruthy();
    expect(screen.getByLabelText("出力")).toBeTruthy();
    expect(screen.getByLabelText("状態")).toBeTruthy();
  });

  it("起動時にサンプルを表示し、未開始の状態でバックエンドには何も要求しない", () => {
    const { backend, workspace } = renderScreen();
    const firstLine = getHelloSource().split(/\r?\n/)[0] ?? "";

    expect(workspace.getSnapshot().source).toBe(getHelloSource().replace(/\r\n?/g, "\n"));
    expect(screen.getByRole("textbox", { name: "ソース" }).textContent).toContain(firstLine);
    expect(screen.getByTestId("status-label").textContent).toBe("未開始");
    expect(runButton().hasAttribute("disabled")).toBe(false);
    expect(backend.calls).toEqual([]);
  });
});

describe("DebuggerScreen: Run / Step / Reset の配線", () => {
  it("Run で最後まで実行し、出力と正常終了の最終実行数を表示する", async () => {
    const { backend } = renderScreen();

    fireEvent.click(runButton());

    expect(await screen.findByText("正常終了")).toBeTruthy();
    expect(screen.getByLabelText("出力の内容(テキスト)").textContent).toContain("Hello World!");
    expect(screen.getByText(`最終実行数: ${HELLO_STEPS} 命令`)).toBeTruthy();
    expect(backend.calls.filter((call) => call.method === "runBatch")).toHaveLength(1);
  });

  it("ブレークポイントで止まったとき、その停止理由を状態領域に表示する", async () => {
    const { workspace } = renderScreen(
      helloScript({
        runBatch: () => ({
          stop: { kind: "breakpoint" },
          executed: 3,
          state: stateDto(3, { lastStop: "breakpoint" }),
        }),
      }),
    );
    workspace.setBreakpoints([0]);

    fireEvent.click(runButton());

    expect(await screen.findByText("ブレークポイントで停止")).toBeTruthy();
  });

  it("Step は 1 命令実行して「ステップ実行後」を表示し、Reset で未開始へ戻す", async () => {
    const { backend } = renderScreen();

    fireEvent.click(stepButton());
    expect(await screen.findByText("ステップ実行後")).toBeTruthy();
    expect(backend.calls.some((call) => call.method === "step")).toBe(true);

    fireEvent.click(resetButton());
    expect(await screen.findByText("未開始")).toBeTruthy();
    expect(backend.calls.some((call) => call.method === "reset")).toBe(true);
  });

  it("入力が不正な間は Run と Step を無効にする(要件 4.1 の開始前提)", () => {
    const { workspace } = renderScreen();
    const input = within(screen.getByRole("radiogroup", { name: "入力の形式" }));

    fireEvent.click(input.getByRole("radio", { name: "16 進" }));
    fireEvent.change(screen.getByRole("textbox", { name: "入力" }), { target: { value: "zz" } });

    expect(workspace.getSnapshot().input).toEqual({ mode: "hex", text: "zz" });
    expect(runButton().hasAttribute("disabled")).toBe(true);
    expect(stepButton().hasAttribute("disabled")).toBe(true);
  });

  it("停止後にソースを編集するとセッションが破棄され、未開始へ戻る", async () => {
    const { workspace } = renderScreen();

    fireEvent.click(runButton());
    expect(await screen.findByText("正常終了")).toBeTruthy();

    workspace.setSource("+");

    expect(await screen.findByText("未開始")).toBeTruthy();
    expect(runButton().hasAttribute("disabled")).toBe(false);
  });

  it("全ブレークポイント解除で Workspace のブレークポイントが空になる", () => {
    const { workspace } = renderScreen();
    workspace.setBreakpoints([0]);

    fireEvent.click(screen.getByRole("button", { name: "全ブレークポイント解除" }));

    expect(workspace.getSnapshot().breakpoints).toEqual([]);
  });
});

describe("DebuggerScreen: 表示の切替と Workspace への接続", () => {
  it("出力の表示形式を切り替えると Workspace の outputMode が変わり、16 進表示になる", async () => {
    const { workspace } = renderScreen();
    fireEvent.click(runButton());
    await screen.findByText("正常終了");

    const modes = within(screen.getByRole("radiogroup", { name: "出力の表示形式" }));
    fireEvent.click(modes.getByRole("radio", { name: "16 進" }));

    expect(workspace.getSnapshot().outputMode).toBe("hex");
    expect(screen.getByLabelText("出力の内容(16 進)")).toBeTruthy();
  });

  it("テープの表示半径を変えると Workspace に反映し、セッションがあれば窓を取り直す", async () => {
    const { backend, workspace } = renderScreen();
    fireEvent.click(runButton());
    await screen.findByText("正常終了");

    fireEvent.change(screen.getByLabelText("表示半径"), { target: { value: "4" } });

    expect(workspace.getSnapshot().tapeRadius).toBe(4);
    await waitFor(() => {
      expect(backend.calls.at(-1)).toEqual({ method: "getTapeWindow", args: [{ sessionId: 1, radius: 4 }] });
    });
  });

  it("テープの半径セレクタは Workspace の値を表示する(既定の値を固定で出さない)", () => {
    const { workspace } = renderScreen();

    act(() => workspace.setTapeRadius(4));

    expect((screen.getByLabelText("表示半径") as HTMLSelectElement).value).toBe("4");
    expect(screen.getAllByLabelText(/^セル /)).toHaveLength(5);
  });

  it("停止時にスクロール要求が立ち、エディタが消費すると降りる(要件 7.2 の配線)", async () => {
    renderScreen(
      helloScript({
        runBatch: () => ({
          stop: { kind: "breakpoint" },
          executed: 3,
          state: stateDto(3, { lastStop: "breakpoint" }),
        }),
      }),
    );
    fireEvent.click(runButton());
    await screen.findByText("ブレークポイントで停止");

    await waitFor(() => {
      expect(probe.scrollRequests).toContain(true);
      expect(probe.scrollRequests.at(-1)).toBe(false);
    });
  });
});

describe("DebuggerScreen: ライフサイクル", () => {
  it("StrictMode の二重マウントでも実行セッションを二重に作らず、Run が 1 回の読み込みで完了する", async () => {
    const { backend } = renderScreen(helloScript(), { strict: true });

    fireEvent.click(runButton());

    expect(await screen.findByText("正常終了")).toBeTruthy();
    expect(backend.calls.filter((call) => call.method === "loadSource")).toHaveLength(1);
  });

  it("StrictMode でセッション開始後の半径・ブレークポイントの変更は、1 回ずつだけバックエンドへ届く", async () => {
    const { backend, workspace } = renderScreen(helloScript(), { strict: true });
    fireEvent.click(stepButton());
    await screen.findByText("ステップ実行後");
    const windowsBefore = countOf(backend, "getTapeWindow");
    const breakpointsBefore = countOf(backend, "setBreakpoints");

    act(() => {
      workspace.setTapeRadius(4);
      workspace.setBreakpoints([0]);
    });
    await flush();

    expect(countOf(backend, "getTapeWindow") - windowsBefore).toBe(1);
    expect(countOf(backend, "setBreakpoints") - breakpointsBefore).toBe(1);
  });

  it("アンマウント後は await を挟んでも Workspace の変更がバックエンドへ届かない(購読の解除)", async () => {
    const { backend, workspace, unmount } = renderScreen();
    fireEvent.click(stepButton());
    await screen.findByText("ステップ実行後");

    unmount();
    const before = backend.calls.length;
    act(() => {
      workspace.setBreakpoints([0]);
      workspace.setTapeRadius(2);
    });
    await flush();

    expect(backend.calls.length).toBe(before);
  });
});

describe("DebuggerScreen: 一時停止と実行エラーの表示", () => {
  it("実行中のバッチ間で Pause すると、状態ラベルは「一時停止」になる(要件 5.7・6.5)", async () => {
    const hold: { release?: (result: BatchResultDto) => void } = {};
    let batches = 0;
    const { backend } = renderScreen(
      helloScript({
        runBatch: () => {
          batches += 1;
          if (batches === 1) {
            return { stop: { kind: "batchReached" }, executed: 1, state: stateDto(1) };
          }
          return new Promise<BatchResultDto>((resolve) => {
            hold.release = resolve;
          });
        },
        pause: () => {
          hold.release?.({ stop: { kind: "cancelled" }, executed: 0, state: stateDto(1) });
        },
      }),
    );

    fireEvent.click(runButton());
    await waitFor(() => {
      expect(countOf(backend, "runBatch")).toBe(2);
    });
    fireEvent.click(pauseButton());

    expect(await screen.findByText("一時停止")).toBeTruthy();
    expect(countOf(backend, "pause")).toBe(1);
  });

  it("実行エラーは ErrorBanner に、操作の失敗は NoticeList に、別の領域として出る(要件 10.4・10.7)", async () => {
    const { workspace } = renderScreen(
      helloScript({
        runBatch: () => ({
          stop: { kind: "error", error: { kind: "pointerOutOfBounds", pc: 0, dp: 0, direction: "left" } },
          executed: 0,
          state: stateDto(0),
        }),
        getTapeWindow: () => {
          throw { kind: "io", message: "failed to read file: denied", field: null };
        },
      }),
    );
    fireEvent.click(runButton());
    await waitFor(() => expect(document.querySelector(".error-banner")).not.toBeNull());

    act(() => workspace.setTapeRadius(3));

    expect(await screen.findByText(/ファイルの読み込みに失敗しました/)).toBeTruthy();
    const notices = screen.getByLabelText("通知");
    const banner = document.querySelector(".error-banner");
    expect(banner).not.toBeNull();
    expect(banner?.textContent).toContain("ポインタ範囲外");
    expect(banner?.textContent).not.toContain("ファイルの読み込み");
    expect(notices.textContent).not.toContain("ポインタ範囲外");
    expect(notices.contains(banner)).toBe(false);
  });
});

describe("DebuggerScreen: 実行中の編集の制限(要件 5.10・3.1・5.1)", () => {
  it("実行中はソースを読み取り専用にし、入力と上限の欄を無効にする", async () => {
    const hold: { release?: (result: BatchResultDto) => void } = {};
    const { backend } = renderScreen(
      helloScript({
        runBatch: () =>
          new Promise<BatchResultDto>((resolve) => {
            hold.release = resolve;
          }),
      }),
    );

    fireEvent.click(runButton());
    await waitFor(() => {
      expect(countOf(backend, "runBatch")).toBe(1);
    });

    expect(screen.getByRole("textbox", { name: "ソース" }).getAttribute("contenteditable")).toBe("false");
    expect((screen.getByRole("textbox", { name: "入力" }) as HTMLTextAreaElement).disabled).toBe(true);
    expect((screen.getByLabelText("命令数上限") as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByLabelText("上限なし") as HTMLInputElement).disabled).toBe(true);

    hold.release?.({
      stop: { kind: "halted" },
      executed: 1,
      state: stateDto(1, { halted: true, lastStop: "halted" }),
    });
    expect(await screen.findByText("正常終了")).toBeTruthy();
    expect(screen.getByRole("textbox", { name: "ソース" }).getAttribute("contenteditable")).toBe("true");
  });
});

describe("DebuggerScreen: クライアント側の拒否の通知(要件 3.7・6.3・1.5)", () => {
  it("16 進の入力を UTF-8 として不正なままテキストへ切り替えると、理由を通知し、モードは変えない", () => {
    const { workspace } = renderScreen();
    const modes = within(screen.getByRole("radiogroup", { name: "入力の形式" }));

    fireEvent.click(modes.getByRole("radio", { name: "16 進" }));
    fireEvent.change(screen.getByRole("textbox", { name: "入力" }), { target: { value: "ff" } });
    fireEvent.click(modes.getByRole("radio", { name: "テキスト" }));

    expect(workspace.getSnapshot().input).toEqual({ mode: "hex", text: "ff" });
    const notices = screen.getByLabelText("通知");
    expect(notices.textContent).toContain("入力が UTF-8 として正しくないため、テキストに切り替えられません");
    expect(notices.textContent).not.toMatch(/input|invalid/);
  });

  it("命令の無い行のガターをクリックすると、ブレークポイントを置けない理由を通知する", () => {
    const { workspace } = renderScreen();
    act(() => workspace.setSource("+\nabc\n"));

    // エディタの実体から 2 行目(命令の無い行)のブレークポイント切替を実行する(ガターのクリックと同じ経路)。
    // jsdom では座標に基づくガターのクリックを再現できないため、切替の関数を直接呼ぶ
    const view = EditorView.findFromDOM(document.querySelector(".cm-content") as HTMLElement);
    expect(view).not.toBeNull();
    act(() => {
      toggleBreakpointAtLine(view as EditorView, 2);
    });

    const notices = screen.getByLabelText("通知");
    expect(notices.textContent).toContain("この行(カーソル以降)に命令が無いため、ブレークポイントを設定できません");
    expect(notices.textContent).not.toMatch(/breakpoint|field/);
  });
});

describe("DebuggerScreen: 実行中の文書の同期失敗(要件 10.7・10.10)", () => {
  it("実行中に未保存フラグの同期が失敗しても、状態ラベルは「実行中」のままで、通知として表示する", async () => {
    const hold: { release?: (result: BatchResultDto) => void } = {};
    const { backend, workspace } = renderScreen(
      helloScript({
        runBatch: () =>
          new Promise<BatchResultDto>((resolve) => {
            hold.release = resolve;
          }),
        setDocumentDirty: () => {
          throw { kind: "io", message: "failed to write file: denied", field: null };
        },
      }),
    );
    fireEvent.click(runButton());
    await waitFor(() => {
      expect(countOf(backend, "runBatch")).toBe(1);
    });

    // 実行中は編集できないが、Workspace の値が変われば(別経路の更新)同期が試みられる
    act(() => workspace.setSource("+"));
    await flush();

    expect(screen.getByTestId("status-label").textContent).toBe("実行中");
    expect(screen.getByLabelText("通知").textContent).toContain("ファイルの書き込みに失敗しました");
    expect(document.querySelector(".error-banner")).toBeNull();

    hold.release?.({
      stop: { kind: "halted" },
      executed: 1,
      state: stateDto(1, { halted: true, lastStop: "halted" }),
    });
    expect(await screen.findByText("正常終了")).toBeTruthy();
  });
});

describe("DebuggerScreen: 既定のメニューの抑止と、ファイル操作・実行の占有(9.6 レビュー指摘)", () => {
  it("画面の上の右クリックは既定のメニューを抑止し、編集可能なソースの上では残す", () => {
    renderScreen();
    const tape = document.querySelector(".debugger-area-tape") as Element;
    const shown = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    tape.dispatchEvent(shown);
    expect(shown.defaultPrevented).toBe(true);

    const source = screen.getByRole("textbox", { name: "ソース" });
    const onSource = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    source.dispatchEvent(onSource);
    expect(onSource.defaultPrevented).toBe(false);
  });

  it("ファイル保存の進行中は Run と F5 が実行を始めず、保存の後は Run できる", async () => {
    let releaseSave!: (result: SaveFileResult) => void;
    const savePending = new Promise<SaveFileResult>((resolve) => {
      releaseSave = resolve;
    });
    const { backend } = renderScreen(helloScript({ saveSourceFile: () => savePending }));

    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    await waitFor(() => {
      expect(countOf(backend, "saveSourceFile")).toBe(1);
    });

    fireEvent.click(runButton());
    fireEvent.keyDown(window, { key: "F5" });
    await flush();
    expect(countOf(backend, "loadSource")).toBe(0);
    expect(countOf(backend, "startSession")).toBe(0);

    releaseSave({ kind: "saved", fileName: "hello.bf" });
    await flush();
    fireEvent.click(runButton());
    expect(await screen.findByText("正常終了")).toBeTruthy();
    expect(countOf(backend, "loadSource")).toBe(1);
  });

  it("開始処理の往復中に新規・開くを押しても、文書を置換せず、ダイアログも出さない", async () => {
    let releaseLoad!: (result: LoadSourceResult) => void;
    const loadPending = new Promise<LoadSourceResult>((resolve) => {
      releaseLoad = resolve;
    });
    const { backend, workspace } = renderScreen(helloScript({ loadSource: () => loadPending }));
    const before = workspace.getSnapshot().source;

    fireEvent.click(runButton());
    await waitFor(() => {
      expect(countOf(backend, "loadSource")).toBe(1);
    });

    fireEvent.keyDown(window, { key: "n", ctrlKey: true });
    fireEvent.keyDown(window, { key: "o", ctrlKey: true });
    await flush();

    expect(countOf(backend, "newDocument")).toBe(0);
    expect(countOf(backend, "openSourceFile")).toBe(0);
    expect(workspace.getSnapshot().source).toBe(before);
    expect(screen.queryByRole("alertdialog")).toBeNull();

    releaseLoad({ kind: "ok", programId: 7, instructionCount: 3, lineCount: 1 });
    expect(await screen.findByText("正常終了")).toBeTruthy();
  });
});
