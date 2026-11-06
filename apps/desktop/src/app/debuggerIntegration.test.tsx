// 操作列と CLI 結果一致の画面レベルの統合テスト(8.4)。
// 実際の DebuggerScreen を描画し、バックエンドは台本の機械(src/test/machineScript)を使う。
// 機械は実サービスと同じ応答規則(出力の末尾窓・停止の再開規則・上限・範囲外)で動くため、画面に出る
// 状態ラベル・出力・テープ・エラー・エディタの強調・ブレークポイントの装飾が、9.2/9.3 で固定した値
// (Hello World: pc=106・dp=6・実行数 906)と一致することを確かめる。実 CLI バイナリは使わない(9.3 で検証済み)。
//
// 決定性:
// - 描画フレーム(requestAnimationFrame)は手動で発火させる。時刻(performance.now)は読み取りごとに 30 ms 進む
//   固定の時計にし、バッチ幅が 1,000 のまま変わらないようにする(往復時間 30 ms は据え置きの帯)。
// - 無限実行の系列は `gated` の機械で、バッチ要求をテストが 1 つずつ解放する。解放のたびに、マイクロタスクを
//   使い切る `settle` で次の状態まで進める(実時間の待ちは使わない)。
// - 機械の譲りはマイクロタスクだけ(タイマーなし)。一時停止を実行の途中で届ける試験は、譲りの時点で操作を発行する。

import { StrictMode } from "react";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { EditorView } from "@codemirror/view";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BatchResultDto } from "../generated/BatchResultDto";
import type { SessionStateDto } from "../generated/SessionStateDto";
import { encodeBase64 } from "../domain/base64";
import { createFakeBackend, type CommandMethod, type FakeBackend, type FakeScript } from "../ipc/fakeBackend";
import { createMachineScript, type MachineOptions, type MachineScript } from "../test/machineScript";
import { createWorkspace, type WorkspaceStore } from "../state/workspace";
import { DebuggerScreen } from "./DebuggerScreen";

/** リポジトリ同梱の Hello World(9.2/9.3 のコーパスと同じ) */
const HELLO = readFileSync(join(process.cwd(), "..", "..", "examples", "hello.bf"), "utf8");
const HELLO_OUTPUT = "Hello World!\n";
const HELLO_STEPS = 906;

/**
 * 無限実行の系列。1 行目の '+' 65 個で 'A'(65)を作り、2 行目の `[.]` で出し続ける。
 * 命令添字: '[' は 65、'.' は 66、']' は 67。
 */
const LOOP_SRC = `${"+".repeat(65)}\n[.]`;
/** 2 行目の '.'(命令添字 66・オフセット 67) */
const DOT_INDEX = 66;
const DOT_OFFSET = 67;

const runButton = () => screen.getByRole("button", { name: /^Run/ });
const stepButton = () => screen.getByRole("button", { name: /^Step/ });
const resetButton = () => screen.getByRole("button", { name: /^Reset/ });
const pauseButton = () => screen.getByRole("button", { name: /^Pause/ });
const label = () => screen.getByTestId("status-label").textContent;

/** テキスト表示の内容。出力が空のときは表示の要素が無いため空文字とする */
const outputText = () => screen.queryByLabelText("出力の内容(テキスト)")?.textContent ?? "";
/** 状態領域の値(dt の見出しに対応する dd) */
function valueOf(name: string): string | null {
  const term = within(screen.getByLabelText("状態")).getByText(name, { selector: "dt" });
  return term.nextElementSibling?.textContent ?? null;
}
const errorBanner = () => document.querySelector(".error-banner")?.textContent ?? null;
/** エディタの現在命令の強調(無ければ null) */
const editorCurrent = () => document.querySelector(".cm-execution-current")?.textContent ?? null;
const editorRuntimeError = () => document.querySelector(".cm-execution-runtime-error")?.textContent ?? null;
/** ブレークポイントの装飾: ガターの丸印の数と、命令文字の枠の文字 */
const gutterMarkCount = () => document.querySelectorAll(".cm-breakpoint-marker").length;
const breakpointChars = () => Array.from(document.querySelectorAll(".cm-breakpoint-char")).map((el) => el.textContent);
/** テープ窓の dp のセル(aria-current の付いたセル)の番地・値・ラベル */
function pointerCell(): { index: string | null; value: string | null; label: string | null } {
  const cell = document.querySelector('[aria-current="true"]');
  return {
    index: cell?.querySelector(".tape-cell-index")?.textContent ?? null,
    value: cell?.querySelector(".tape-cell-dec")?.textContent ?? null,
    label: cell?.querySelector(".tape-cell-pointer-label")?.textContent ?? null,
  };
}

/** 命令文字(+-><.,[])だけを取り出す */
function opsOf(source: string): string {
  return Array.from(source)
    .filter((ch) => "><+-.,[]".includes(ch))
    .join("");
}

/** 決定的な時計と、手動で発火する描画フレーム */
let clockTicks = 0;
let frameSeq = 0;
let frames = new Map<number, () => void>();

beforeEach(() => {
  clockTicks = 0;
  frameSeq = 0;
  frames = new Map();
  // 読み取りごとに 30 ms 進む。バッチの往復時間は必ず 30 ms になり、幅は 1,000 のまま変わらない
  vi.spyOn(performance, "now").mockImplementation(() => {
    clockTicks += 30;
    return clockTicks;
  });
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frameSeq += 1;
    const id = frameSeq;
    frames.set(id, () => callback(0));
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    frames.delete(id);
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** 保留中の描画フレームを発火する(描画の間引きの周期に相当) */
function fireFrames(): void {
  const due = [...frames.values()];
  frames.clear();
  act(() => {
    for (const callback of due) callback();
  });
}

/** マイクロタスクを使い切るまで進める。機械・IPC・コントローラは全てマイクロタスクで進むため、実時間を使わない */
async function settle(): Promise<void> {
  await act(async () => {
    for (let i = 0; i < 200; i += 1) {
      await Promise.resolve();
    }
  });
}

function press(init: KeyboardEventInit & { key: string }): void {
  fireEvent.keyDown(window, init);
}

/** エディタのカーソル(選択の先頭)を置く。クリックの代わりに、エディタの表示そのものへ位置を与える */
function placeCursor(offset: number): void {
  const content = document.querySelector<HTMLElement>(".cm-content");
  const view = content ? EditorView.findFromDOM(content) : null;
  if (view === null) throw new Error("editor view is not mounted");
  act(() => {
    view.dispatch({ selection: { anchor: offset } });
  });
}

function countOf(backend: FakeBackend, method: CommandMethod): number {
  return backend.calls.filter((call) => call.method === method).length;
}

type ViewArg = { sessionId: number; view: { outputFrom: number } };
/** 要求された outputFrom の列(呼ばれた順) */
function requestedFroms(backend: FakeBackend, method: "runBatch" | "step"): number[] {
  return backend.calls.filter((call) => call.method === method).map((call) => (call.args[0] as ViewArg).view.outputFrom);
}

type Harness = {
  machine: MachineScript;
  backend: FakeBackend;
  workspace: WorkspaceStore;
  /** 機械が返したバッチ応答(呼ばれた順) */
  batchResults: BatchResultDto[];
  /** gated のとき、解放を待っているバッチ要求 */
  waiting: Array<() => void>;
  unmount: () => void;
};

/**
 * 台本の機械を繋いだ画面を描画する。
 * - `gated`: バッチ要求は解放されるまで機械へ渡さない(無限実行を 1 バッチずつ制御する)
 * - `machine`: 機械の譲りの設定(一時停止を実行の途中で届ける試験に使う)
 * - `overrides`: 機械を受け取り、一部の応答だけを差し替える台本を返す
 */
function renderMachine(
  options: {
    strict?: boolean;
    gated?: boolean;
    machine?: MachineOptions;
    overrides?: (machine: MachineScript) => FakeScript;
  } = {},
): Harness {
  const machine = createMachineScript(options.machine);
  const batchResults: BatchResultDto[] = [];
  const waiting: Array<() => void> = [];
  const extra = options.overrides?.(machine) ?? {};
  const script: FakeScript = {
    ...machine.script,
    setDocumentDirty: () => undefined,
    runBatch: async (req) => {
      if (options.gated === true) {
        await new Promise<void>((release) => {
          waiting.push(release);
        });
      }
      const result = await machine.script.runBatch!(req);
      batchResults.push(result);
      return result;
    },
    ...extra,
  };
  const backend = createFakeBackend(script);
  const workspace: WorkspaceStore = createWorkspace();
  const element = <DebuggerScreen backend={backend} workspace={workspace} />;
  const view = render(options.strict === true ? <StrictMode>{element}</StrictMode> : element);
  return { machine, backend, workspace, batchResults, waiting, unmount: view.unmount };
}

/** 待っているバッチ要求を 1 つ解放し、その応答が画面の制御へ届くまで進める */
async function releaseBatch(harness: Harness): Promise<void> {
  const release = harness.waiting.shift();
  if (release === undefined) throw new Error("no batch request is waiting");
  release();
  await settle();
}

/**
 * 文書を読み込んだ状態にする(既定のサンプルを使わず、同じ文字列で始める)。
 * 文書の置換は revision を進めるため、エディタの表示も同じ文字列へ差し替わる(編集中の setSource は表示を追わない)。
 */
function useSource(workspace: WorkspaceStore, source: string): void {
  act(() => workspace.replaceDocument({ source, fileName: null, lossy: false }));
}

/** 入力欄を 16 進で与える(入力の形式の切替を含む) */
function setHexInput(hex: string): void {
  const modes = within(screen.getByRole("radiogroup", { name: "入力の形式" }));
  fireEvent.click(modes.getByRole("radio", { name: "16 進" }));
  fireEvent.change(screen.getByRole("textbox", { name: "入力" }), { target: { value: hex } });
}

describe("操作列: Hello World の画面操作(ブレークポイント停止 → Step → 再開 → リセット → 再実行)", () => {
  it("F9 で先頭命令に置き、Run・Step・Run・Reset・F9 解除・Run の順で状態・強調・テープ・出力・実行数が期待どおりに変わる", async () => {
    const { machine, backend, workspace } = renderMachine();
    useSource(workspace, HELLO);
    const ops = opsOf(HELLO);

    // 先頭の命令(添字 0・オフセット 0)にブレークポイント。ガターの丸印と命令文字の枠が出る
    press({ key: "F9" });
    await settle();
    expect(workspace.getSnapshot().breakpoints).toEqual([0]);
    expect(gutterMarkCount()).toBe(1);
    expect(breakpointChars()).toEqual([ops[0]]);

    // Run: 先頭で停止する(実行 0)。現在命令の強調は先頭の命令、テープの dp は 0
    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("ブレークポイントで停止");
    expect(valueOf("pc")).toBe("0");
    expect(valueOf("実行数")).toBe("0");
    expect(outputText()).toBe("");
    expect(editorCurrent()).toBe(ops[0]);
    expect(pointerCell()).toMatchObject({ index: "0", value: "0", label: "▼ dp" });
    expect(machine.inspect()).toMatchObject({ pc: 0, steps: 0, breakpoints: [0] });

    // Step: 1 命令だけ進む(強調は次の命令へ移る)
    fireEvent.click(stepButton());
    await settle();
    expect(label()).toBe("ステップ実行後");
    expect(valueOf("pc")).toBe("1");
    expect(valueOf("実行数")).toBe("1");
    expect(editorCurrent()).toBe(ops[1]);

    // Run(再開): 停止位置の命令は照合せず、最後まで進む。停止の後は現在命令の強調が無い
    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("正常終了");
    expect(screen.getByText(`最終実行数: ${HELLO_STEPS} 命令`)).toBeTruthy();
    expect(valueOf("pc")).toBe("106");
    expect(valueOf("dp")).toBe("6");
    expect(outputText()).toBe(HELLO_OUTPUT);
    expect(editorCurrent()).toBeNull();
    expect(pointerCell()).toMatchObject({ index: "6", value: "10", label: "▼ dp" });

    // Reset: 未開始へ戻り、出力は空。ブレークポイントは保持される(丸印と枠が残る)
    fireEvent.click(resetButton());
    await settle();
    expect(label()).toBe("未開始");
    expect(outputText()).toBe("");
    expect(valueOf("pc")).toBe("0");
    expect(workspace.getSnapshot().breakpoints).toEqual([0]);
    expect(gutterMarkCount()).toBe(1);

    // F9 で解除してから再実行: ブレークポイントに止まらず、同じ最終状態になる
    press({ key: "F9" });
    await settle();
    expect(workspace.getSnapshot().breakpoints).toEqual([]);
    expect(gutterMarkCount()).toBe(0);
    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("正常終了");
    expect(outputText()).toBe(HELLO_OUTPUT);
    expect(valueOf("実行数")).toBe(String(HELLO_STEPS));
    expect(countOf(backend, "reset")).toBe(1);
    expect(requestedFroms(backend, "step")).toEqual([0]);
  });
});

describe("操作列: 2 行目の F9 によるブレークポイントと Step の反復", () => {
  it("行頭で F9 して置いたブレークポイントで止まり、Step を 5 回反復して各回の pc・実行数・テープ・出力・強調を確かめ、再開は次の停止で止まる", async () => {
    const { machine, backend, workspace } = renderMachine();
    useSource(workspace, LOOP_SRC);
    act(() => workspace.setUnlimited(true));

    // 2 行目の行頭(オフセット 66)へカーソルを置き、F9 で切り替える(F9 はカーソル以降で最初の命令に置く)。
    // jsdom では行の位置が測れずガターの行要素が出ないため、ガターのクリックの代わりにカーソルを置く
    // 2 行目の '.'(オフセット 67・命令添字 66)へカーソルを置いて F9
    placeCursor(DOT_OFFSET);
    press({ key: "F9" });
    await settle();
    expect(workspace.getSnapshot().breakpoints).toEqual([DOT_OFFSET]);
    expect(gutterMarkCount()).toBe(1);
    expect(breakpointChars()).toEqual(["."]);

    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("ブレークポイントで停止");
    expect(valueOf("pc")).toBe(String(DOT_INDEX));
    expect(valueOf("実行数")).toBe("66");
    expect(editorCurrent()).toBe(".");
    expect(outputText()).toBe("");

    // Step を 5 回。core と同じく ']' は '[' の次(添字 66)へ戻すため、'[' は再実行せず '.' ']' を繰り返す。
    // テープの値 65 は変わらず、出力は 1 命令ごとに 'A' が 1 つ増える
    const expected = [
      { pc: 67, steps: 67, ch: "]", out: "A" },
      { pc: 66, steps: 68, ch: ".", out: "A" },
      { pc: 67, steps: 69, ch: "]", out: "AA" },
      { pc: 66, steps: 70, ch: ".", out: "AA" },
      { pc: 67, steps: 71, ch: "]", out: "AAA" },
    ];
    for (const step of expected) {
      fireEvent.click(stepButton());
      await settle();
      expect(label()).toBe("ステップ実行後");
      expect(valueOf("pc")).toBe(String(step.pc));
      expect(valueOf("実行数")).toBe(String(step.steps));
      expect(editorCurrent()).toBe(step.ch);
      expect(outputText()).toBe(step.out);
      expect(pointerCell()).toMatchObject({ index: "0", value: "65", label: "▼ dp" });
      expect(outputText()).toBe("A".repeat(machine.inspect()?.output.length ?? 0));
    }
    // 各 Step の要求は、画面に出ている出力の末尾から(先頭から欠けず、重ならない)
    expect(requestedFroms(backend, "step")).toEqual([0, 1, 1, 2, 2]);

    // Run(再開): 直前の Step の位置は照合せず、']' を実行して '.' の位置(ブレークポイント)で止まる
    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("ブレークポイントで停止");
    expect(valueOf("pc")).toBe(String(DOT_INDEX));
    expect(valueOf("実行数")).toBe("72");
    expect(outputText()).toBe("AAA");
  });
});

describe("出力の分割(5.6 の回帰): 未反映の応答の間に次のバッチを要求しても、先頭が欠けない", () => {
  it("バッチ 1 の応答が画面へ届く前に次のバッチを要求し、その最中にフレームが発火しても、要求の出力位置は受理済みの末尾で、画面の出力は全バイト", async () => {
    const h = renderMachine({ gated: true });
    useSource(h.workspace, LOOP_SRC);
    act(() => h.workspace.setUnlimited(true));
    fireEvent.click(runButton());
    await settle();

    // バッチ 1 の応答が届く。描画フレームはまだ発火せず、画面は空のまま。バッチ 2 を要求している
    await releaseBatch(h);
    const first = h.machine.inspect()?.output.length ?? 0;
    expect(first).toBeGreaterThan(0);
    expect(h.waiting.length).toBe(1);
    expect(outputText()).toBe("");
    // バッチ 2 の要求は、受理済みの末尾(バッチ 1 の総量)から出す(画面の末尾 0 から出してはならない)
    expect(requestedFroms(h.backend, "runBatch")).toEqual([0, first]);

    // バッチ 2 の要求中に描画フレームが発火する: 画面はバッチ 1 の分を反映する
    fireFrames();
    expect(outputText()).toBe("A".repeat(first));

    // バッチ 2 の応答が届き、バッチ 3 を要求する。フレームが発火するまで画面は first のまま
    await releaseBatch(h);
    const second = h.machine.inspect()?.output.length ?? 0;
    expect(second).toBeGreaterThan(first);
    expect(requestedFroms(h.backend, "runBatch")).toEqual([0, first, second]);
    expect(outputText()).toBe("A".repeat(first));
    fireFrames();
    expect(outputText()).toBe("A".repeat(second));

    // 一時停止: バッチ 3 の応答で止まり、最終応答まで画面に反映する(先頭から欠けず、省略も出ない)
    fireEvent.click(pauseButton());
    await releaseBatch(h);
    const total = h.machine.inspect()?.output.length ?? 0;
    expect(label()).toBe("一時停止");
    expect(outputText()).toBe("A".repeat(total));
    expect(screen.queryByText(/バイトを省略/)).toBeNull();
    expect(requestedFroms(h.backend, "runBatch")).toEqual([0, first, second]);
  });
});

describe("一時停止の配線: 実行中のバッチへ届き、同じ位置から再開して進む", () => {
  it("実行の途中の Pause は機械の実行を cancelled で止め、Run で同じ位置から進み、次の Pause も効く", async () => {
    // 譲りの 3 回目(命令 300 件の時点)で、画面の Pause を押す。以後の譲りでは押さない
    let yields = 0;
    let pausedInside = false;
    const h = renderMachine({
      gated: true,
      machine: {
        yieldEvery: 100,
        onYield: () => {
          yields += 1;
          if (!pausedInside && yields === 3) {
            pausedInside = true;
            act(() => {
              fireEvent.click(pauseButton());
            });
          }
        },
      },
    });
    useSource(h.workspace, LOOP_SRC);
    act(() => h.workspace.setUnlimited(true));

    fireEvent.click(runButton());
    await settle();
    await releaseBatch(h);

    // 機械側は cancelled で止まり、画面は一時停止になる。Pause は 1 回だけ届いている
    expect(countOf(h.backend, "pause")).toBe(1);
    const cancelled = h.batchResults.at(-1);
    expect(cancelled?.stop).toEqual({ kind: "cancelled" });
    expect(cancelled?.state.lastStop).toBe("cancelled");
    expect(label()).toBe("一時停止");
    const stoppedAt = h.machine.inspect()?.steps ?? 0;
    expect(stoppedAt).toBe(cancelled?.state.snapshot.stepsExecuted);
    expect(stoppedAt).toBeGreaterThan(0);
    expect(stoppedAt).toBeLessThan(1_000);
    expect(valueOf("実行数")).toBe(String(stoppedAt));
    expect(outputText()).toBe("A".repeat(h.machine.inspect()?.output.length ?? 0));

    // Run で再開する: 同じ位置で止まり続けず、次のバッチ(1,000 命令)が要求されて進む
    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("実行中");
    expect(h.waiting.length).toBe(1);
    await releaseBatch(h);
    expect(h.batchResults.at(-1)?.stop).toEqual({ kind: "batchReached" });
    expect(h.machine.inspect()?.steps).toBe(stoppedAt + 1_000);
    // 実行中の表示は描画フレームで反映する(間引き)
    fireFrames();
    expect(valueOf("実行数")).toBe(String(stoppedAt + 1_000));

    // 次の Pause(バッチの間に届く)も効き、一時停止で止まる
    fireEvent.click(pauseButton());
    await releaseBatch(h);
    expect(countOf(h.backend, "pause")).toBe(2);
    expect(label()).toBe("一時停止");
    expect(outputText()).toBe("A".repeat(h.machine.inspect()?.output.length ?? 0));
  });
});

describe("実行中の編集の拒否(要件 5.10・3.1・5.1)", () => {
  it("無限実行の間はソース・入力・上限を編集できず、Pause で止まると編集できる", async () => {
    const h = renderMachine({ gated: true });
    useSource(h.workspace, LOOP_SRC);
    act(() => h.workspace.setUnlimited(true));

    fireEvent.click(runButton());
    await settle();
    await releaseBatch(h);
    expect(screen.getByRole("textbox", { name: "ソース" }).getAttribute("contenteditable")).toBe("false");
    expect((screen.getByRole("textbox", { name: "入力" }) as HTMLTextAreaElement).disabled).toBe(true);
    expect((screen.getByLabelText("上限なし") as HTMLInputElement).disabled).toBe(true);

    fireEvent.click(pauseButton());
    await releaseBatch(h);
    expect(label()).toBe("一時停止");
    expect(screen.getByRole("textbox", { name: "ソース" }).getAttribute("contenteditable")).toBe("true");
    expect((screen.getByRole("textbox", { name: "入力" }) as HTMLTextAreaElement).disabled).toBe(false);
  });
});

describe("編集による破棄: 編集 → 未開始へ戻る → 再度 Run で新しいセッション", () => {
  it("Hello World を実行したあとにソースを編集すると未開始へ戻り、次の Run は新しいセッションで同じ結果になる", async () => {
    const h = renderMachine();
    useSource(h.workspace, HELLO);
    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("正常終了");
    expect(outputText()).toBe(HELLO_OUTPUT);

    // 末尾に改行を足す(命令は変わらないが、ソースの変更としてセッションを破棄する)
    act(() => h.workspace.setSource(`${HELLO}\n`));
    await settle();
    expect(label()).toBe("未開始");
    expect(outputText()).toBe("");
    expect(editorCurrent()).toBeNull();

    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("正常終了");
    expect(outputText()).toBe(HELLO_OUTPUT);
    expect(valueOf("実行数")).toBe(String(HELLO_STEPS));
    expect(countOf(h.backend, "startSession")).toBe(2);
    const sessions = new Set(h.backend.calls.filter((c) => c.method === "runBatch").map((c) => (c.args[0] as ViewArg).sessionId));
    expect([...sessions]).toEqual([1, 2]);
  });
});

describe("実行エラーと解析エラーの表示(次の操作まで維持される)", () => {
  it("ポインタ範囲外は実行エラーとして状態 pc=4・dp=0 とバナーを示し、テープ半径の変更では消えず、リセットで消える", async () => {
    const { workspace } = renderMachine();
    useSource(workspace, "+>+<<");

    fireEvent.click(runButton());
    await settle();

    expect(label()).toBe("実行エラー");
    expect(valueOf("pc")).toBe("4");
    expect(valueOf("dp")).toBe("0");
    expect(valueOf("実行数")).toBe("4");
    expect(errorBanner()).toContain("ポインタ範囲外");
    expect(editorRuntimeError()).toBe("<");
    expect(editorCurrent()).toBeNull();
    expect(pointerCell()).toMatchObject({ index: "0", value: "1", label: "▼ dp" });

    act(() => workspace.setTapeRadius(3));
    await settle();
    expect(errorBanner()).toContain("ポインタ範囲外");
    expect(editorRuntimeError()).toBe("<");

    fireEvent.click(resetButton());
    await settle();
    expect(label()).toBe("未開始");
    expect(errorBanner()).toBeNull();
    expect(editorRuntimeError()).toBeNull();
  });

  it("命令数の上限超過は上限と pc を示す(上限 10 の '+[]' は pc=2 で止まる)", async () => {
    const { workspace } = renderMachine();
    useSource(workspace, "+[]");
    act(() => workspace.setLimitText("10"));

    fireEvent.click(runButton());
    await settle();

    expect(label()).toBe("実行エラー");
    expect(errorBanner()).toContain("命令数の上限(10 命令)を超えました");
    expect(errorBanner()).toContain("pc: 2");
    expect(valueOf("実行数")).toBe("10");
  });

  it("解析エラー(閉じない '[')は実行せず、解析エラーとして位置を示す", async () => {
    const { backend, workspace } = renderMachine();
    useSource(workspace, "[");

    fireEvent.click(runButton());
    await settle();

    expect(screen.getByText(/対応する閉じ括弧がない開き括弧/)).toBeTruthy();
    expect(editorCurrent()).toBeNull();
    expect(document.querySelector(".cm-parse-error")?.textContent).toBe("[");
    expect(label()).toBe("未開始");
    expect(countOf(backend, "startSession")).toBe(0);
  });
});

describe("入力と出力: 0x00・0xFF と入力切れ", () => {
  it("入力 00 FF をそのまま出力し、読み取り数を示す", async () => {
    const { workspace } = renderMachine();
    useSource(workspace, ",.,.");
    setHexInput("00ff");

    fireEvent.click(runButton());
    await settle();

    expect(label()).toBe("正常終了");
    expect(valueOf("入力の読み取り済み")).toBe("2");
    expect(valueOf("入力の総数")).toBe("2");
    fireEvent.click(within(screen.getByRole("radiogroup", { name: "出力の表示形式" })).getByRole("radio", { name: "16 進" }));
    expect(screen.getByLabelText("出力の内容(16 進)").textContent).toMatch(/00.*FF/);
  });
});

describe("破棄済みセッションの遅延応答は画面に反映されない", () => {
  it("Reset の応答が届く前にソースを変えると、遅れて届いた応答は表示に出ない", async () => {
    const gate: { release?: () => void } = {};
    let resets = 0;
    const { workspace, backend } = renderMachine({
      overrides: (machine) => ({
        reset: async (req): Promise<SessionStateDto> => {
          resets += 1;
          const real = await machine.script.reset!(req);
          // 遅れて届く応答の目印: 出力 "STALE" と実行数 999(表示に出ればテストが落ちる)
          const stale: SessionStateDto = {
            ...real,
            output: { dataStart: 0, totalLen: 5, dataBase64: encodeBase64(new TextEncoder().encode("STALE")) },
            snapshot: { ...real.snapshot, stepsExecuted: 999 },
          };
          return new Promise<SessionStateDto>((resolve) => {
            gate.release = () => resolve(stale);
          });
        },
      }),
    });
    useSource(workspace, HELLO);
    fireEvent.click(runButton());
    await settle();
    expect(label()).toBe("正常終了");

    fireEvent.click(resetButton());
    await settle();
    expect(resets).toBe(1);
    act(() => workspace.setSource("+"));
    expect(label()).toBe("未開始");

    act(() => gate.release?.());
    await settle();

    expect(label()).toBe("未開始");
    expect(outputText()).not.toContain("STALE");
    expect(valueOf("実行数")).toBe("0");
    expect(countOf(backend, "reset")).toBe(1);
  });
});

describe("ライフサイクル: StrictMode と破棄", () => {
  it("StrictMode でも Hello World は 1 回の読み込みと 1 回の一括実行で、正しい最終状態になる", async () => {
    const { backend, workspace } = renderMachine({ strict: true });
    useSource(workspace, HELLO);

    fireEvent.click(runButton());
    await settle();

    expect(label()).toBe("正常終了");
    expect(outputText()).toBe(HELLO_OUTPUT);
    expect(countOf(backend, "loadSource")).toBe(1);
    expect(countOf(backend, "startSession")).toBe(1);
  });

  it("実行中にアンマウントしても、以後のバッチ要求は届かない(購読と予約の解除)", async () => {
    const h = renderMachine({ gated: true });
    useSource(h.workspace, LOOP_SRC);
    act(() => h.workspace.setUnlimited(true));

    fireEvent.click(runButton());
    await settle();
    await releaseBatch(h);
    await releaseBatch(h);
    const before = countOf(h.backend, "runBatch");
    expect(before).toBe(3);

    // 待機中の要求を残したままアンマウントし、解放しても次のバッチは発行されない
    h.unmount();
    const release = h.waiting.shift();
    release?.();
    await settle();
    fireFrames();
    await settle();
    expect(countOf(h.backend, "runBatch")).toBe(before);
  });
});
