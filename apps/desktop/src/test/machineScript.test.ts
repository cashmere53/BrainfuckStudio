// 台本の機械(machineScript)の契約テスト。実サービス(brainfuck-gui-api の DebuggerService)と同じ応答規則を
// 小さく模したものが、固定の期待値(Hello World の pc=106・dp=6・実行数 906、9.3 の実 CLI と同じ出力)と
// 核の意味論(ブレークポイントの再開規則・上限・範囲外・出力の末尾窓)に従うことを確かめる。

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { decodeBase64, encodeBase64 } from "../domain/base64";
import { createFakeBackend, type FakeBackend } from "../ipc/fakeBackend";
import { createMachineScript } from "./machineScript";

/**
 * リポジトリ同梱の Hello World(CLI・サービスの 9.2/9.3 と同じ入力)。
 * `npm test` は apps/desktop で動くため、作業ディレクトリから相対に解決する。
 */
const HELLO = readFileSync(join(process.cwd(), "..", "..", "examples", "hello.bf"), "utf8");

const ALL = 1_000_000;
const VIEW = { tapeRadius: 2, outputFrom: 0 };

function bytesOf(base64: string): Uint8Array {
  const decoded = decodeBase64(base64);
  if (!decoded.ok) throw new Error("bad base64");
  return decoded.bytes;
}

function textOf(base64: string): string {
  return new TextDecoder().decode(bytesOf(base64));
}

/** 読み込みとセッション開始をまとめる。入力は生のバイト列、上限は未指定なら無制限 */
async function open(
  source: string,
  options: { input?: number[]; limit?: number; breakpointOffsets?: number[] } = {},
): Promise<{ backend: FakeBackend; sessionId: number; started: Awaited<ReturnType<FakeBackend["startSession"]>> }> {
  const backend = createFakeBackend(createMachineScript().script);
  const loaded = await backend.loadSource({ source });
  if (loaded.kind !== "ok") throw new Error(`load failed: ${JSON.stringify(loaded)}`);
  const started = await backend.startSession({
    programId: loaded.programId,
    inputBase64: encodeBase64(new Uint8Array(options.input ?? [])),
    limit: options.limit === undefined ? { kind: "unlimited" } : { kind: "limited", maxSteps: options.limit },
    breakpointOffsets: options.breakpointOffsets ?? [],
    view: VIEW,
  });
  return { backend, sessionId: started.state.sessionId, started };
}

describe("台本の機械: 固定の期待値(要件 11.1・11.2 と 9.3 の実 CLI と同じ値)", () => {
  it("Hello World は一括実行で Hello World!\\n・pc=106・dp=6・実行数 906 で正常終了する", async () => {
    const { backend, sessionId } = await open(HELLO);

    const result = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    expect(result.stop).toEqual({ kind: "halted" });
    expect(result.executed).toBe(906);
    // 最後の出力 '\n'(10)をセル 6 から出した後、停止時点のセル 6 は 10 のまま
    expect(result.state.snapshot).toEqual({ pc: 106, dp: 6, currentCell: 10, stepsExecuted: 906, halted: true });
    expect(textOf(result.state.output.dataBase64)).toBe("Hello World!\n");
    expect(result.state.lastStop).toBe("halted");
  });

  it("Hello World を 1,000 命令ずつに分けても、出力の連結と最終状態が一致する", async () => {
    const { backend, sessionId } = await open(HELLO);
    let output = "";
    let outputFrom = 0;
    let last: Awaited<ReturnType<FakeBackend["runBatch"]>> | null = null;
    for (let guard = 0; guard < 100; guard += 1) {
      last = await backend.runBatch({ sessionId, batchSize: 1_000, view: { tapeRadius: 2, outputFrom } });
      output += textOf(last.state.output.dataBase64);
      outputFrom = last.state.output.totalLen;
      if (last.stop.kind !== "batchReached") break;
    }
    expect(output).toBe("Hello World!\n");
    expect(last?.state.snapshot).toMatchObject({ pc: 106, dp: 6, stepsExecuted: 906, halted: true });
  });
});

describe("台本の機械: 入出力と範囲外・上限・解析エラー", () => {
  it("入力をそのまま出力する(0x00 と 0xFF を含む)", async () => {
    const { backend, sessionId } = await open(",.,.", { input: [0x00, 0xff] });

    const result = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    expect(result.stop).toEqual({ kind: "halted" });
    expect(Array.from(bytesOf(result.state.output.dataBase64))).toEqual([0x00, 0xff]);
    expect(result.state.inputConsumed).toBe(2);
    expect(result.state.inputTotal).toBe(2);
  });

  it("入力が尽きた状態の入力命令は現在セルに 0 を入れ、読取位置を進めない", async () => {
    const { backend, sessionId } = await open(",.");

    const result = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    expect(result.stop).toEqual({ kind: "halted" });
    expect(result.state.inputConsumed).toBe(0);
    expect(Array.from(bytesOf(result.state.output.dataBase64))).toEqual([0]);
  });

  it("ポインタが左端を越えると pointerOutOfBounds を返し、状態は直前の値を保つ", async () => {
    const { backend, sessionId } = await open("+>+<<");

    const result = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    expect(result.stop).toEqual({
      kind: "error",
      error: { kind: "pointerOutOfBounds", pc: 4, dp: 0, direction: "left" },
    });
    expect(result.executed).toBe(4);
    expect(result.state.snapshot).toMatchObject({ pc: 4, dp: 0, stepsExecuted: 4, halted: false });
    expect(result.state.lastStop).toBeNull();
    expect(result.state.currentRange?.start).toEqual({ byteOffset: 4, utf16Offset: 4, line: 1, column: 5 });
  });

  it("命令数の上限を超えると stepLimitExceeded を返し、上限と pc を伝える", async () => {
    const { backend, sessionId } = await open("+[]", { limit: 10 });

    const result = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    expect(result.stop).toEqual({ kind: "error", error: { kind: "stepLimitExceeded", limit: 10, pc: 2 } });
    expect(result.state.snapshot.stepsExecuted).toBe(10);
  });

  it("対応しない ] は unmatchedClosingBracket、閉じない [ は unmatchedOpeningBracket として解析エラーにする", async () => {
    const backend = createFakeBackend(createMachineScript().script);
    await expect(backend.loadSource({ source: "+]" })).resolves.toEqual({
      kind: "parseError",
      error: {
        kind: "unmatchedClosingBracket",
        instructionIndex: 1,
        position: { byteOffset: 1, utf16Offset: 1, line: 1, column: 2 },
      },
    });
    await expect(backend.loadSource({ source: "a\n[" })).resolves.toEqual({
      kind: "parseError",
      error: {
        kind: "unmatchedOpeningBracket",
        instructionIndex: 0,
        position: { byteOffset: 2, utf16Offset: 2, line: 2, column: 1 },
      },
    });
  });
});

describe("台本の機械: ブレークポイントとバッチの停止(核の再開規則)", () => {
  it("ブレークポイントで止まり、再開すると同じ位置では止まらずに進む", async () => {
    // "+++" の添字 1(UTF-16 オフセット 1)にブレークポイント
    const { backend, sessionId } = await open("+++", { breakpointOffsets: [1] });

    const first = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });
    expect(first.stop).toEqual({ kind: "breakpoint" });
    expect(first.executed).toBe(1);
    expect(first.state.snapshot.pc).toBe(1);

    const second = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });
    expect(second.stop).toEqual({ kind: "halted" });
    expect(second.state.snapshot.stepsExecuted).toBe(3);
  });

  it("命令の無い位置(末尾より後)は unresolved に返し、設定しない", async () => {
    const backend = createFakeBackend(createMachineScript().script);
    const loaded = await backend.loadSource({ source: "+ " });
    if (loaded.kind !== "ok") throw new Error("load");
    const started = await backend.startSession({
      programId: loaded.programId,
      inputBase64: "",
      limit: { kind: "unlimited" },
      breakpointOffsets: [2, 0],
      view: VIEW,
    });
    expect(started.breakpoints.unresolved).toEqual([2]);
    expect(started.breakpoints.breakpoints.map((b) => b.instructionIndex)).toEqual([0]);
  });
});

describe("台本の機械: 表示窓の応答規則(5.6 の出力先頭の回帰)", () => {
  it("outputFrom が総量以下なら、その位置から末尾を返す(分割を跨いでも先頭が欠けない)", async () => {
    const { backend, sessionId } = await open(HELLO);
    await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    const tail = await backend.getSnapshot({ sessionId, view: { tapeRadius: 2, outputFrom: 5 } });

    expect(tail.output).toMatchObject({ dataStart: 5, totalLen: 13 });
    expect(textOf(tail.output.dataBase64)).toBe(" World!\n");
  });

  it("outputFrom が総量より大きければ、末尾の窓を先頭から返す", async () => {
    const { backend, sessionId } = await open(HELLO);
    await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    const window = await backend.getSnapshot({ sessionId, view: { tapeRadius: 2, outputFrom: 99 } });

    expect(window.output.dataStart).toBe(0);
    expect(textOf(window.output.dataBase64)).toBe("Hello World!\n");
  });

  it("出力が 65,536 バイトを超えると末尾 65,536 バイトだけを返す", async () => {
    // "+[.]" は 2 命令ごとに 1 バイト出力する。上限 200,000 命令で十分な量を作る
    const { backend, sessionId } = await open("+[.]", { limit: 200_000 });

    const result = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    expect(result.stop.kind).toBe("error");
    const total = result.state.output.totalLen;
    expect(total).toBeGreaterThan(65_536);
    expect(result.state.output.dataStart).toBe(total - 65_536);
    expect(bytesOf(result.state.output.dataBase64).length).toBe(65_536);
  });
});

describe("台本の機械: 停止位置の表示・一時停止・Step・Reset の契約", () => {
  it("halted では現在命令の範囲を返さず、停止位置(ブレークポイント)では現在命令の範囲を返す", async () => {
    const halted = await open("+>");
    const done = await halted.backend.runBatch({ sessionId: halted.sessionId, batchSize: ALL, view: VIEW });
    expect(done.state.snapshot.halted).toBe(true);
    expect(done.state.currentRange).toBeNull();

    const stopped = await open("+++", { breakpointOffsets: [1] });
    const atBreak = await stopped.backend.runBatch({ sessionId: stopped.sessionId, batchSize: ALL, view: VIEW });
    expect(atBreak.stop).toEqual({ kind: "breakpoint" });
    expect(atBreak.state.currentRange?.start.utf16Offset).toBe(1);
  });

  it("一時停止は実行中のバッチを cancelled で止め、同じ位置から再開できる", async () => {
    // 譲りの 3 回目で一時停止を発行する(譲りは命令 100 件ごと。停止は次の判定で効く)
    let backend: FakeBackend | null = null;
    let yields = 0;
    const machine = createMachineScript({
      yieldEvery: 100,
      onYield: () => {
        yields += 1;
        if (yields === 3) void backend?.pause({ sessionId: 1 });
      },
    });
    backend = createFakeBackend(machine.script);
    const loaded = await backend.loadSource({ source: "+[.]" });
    if (loaded.kind !== "ok") throw new Error("load");
    await backend.startSession({
      programId: loaded.programId,
      inputBase64: "",
      limit: { kind: "unlimited" },
      breakpointOffsets: [],
      view: VIEW,
    });

    const first = await backend.runBatch({ sessionId: 1, batchSize: ALL, view: VIEW });
    expect(first.stop).toEqual({ kind: "cancelled" });
    expect(first.state.lastStop).toBe("cancelled");
    expect(first.executed).toBeGreaterThan(0);
    expect(first.executed).toBeLessThan(ALL);
    expect(first.state.snapshot.stepsExecuted).toBe(first.executed);

    // 旗は次のバッチの入口で解除され、停止位置から続けて 1,000 命令進む
    const second = await backend.runBatch({ sessionId: 1, batchSize: 1_000, view: VIEW });
    expect(second.stop).toEqual({ kind: "batchReached" });
    expect(second.executed).toBe(1_000);
    expect(second.state.snapshot.stepsExecuted).toBe(first.executed + 1_000);
  });

  it("Step の後の Run は、Step した位置のブレークポイントを照合しない", async () => {
    const { backend, sessionId } = await open("+++", { breakpointOffsets: [1, 2] });
    const first = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });
    expect(first.stop).toEqual({ kind: "breakpoint" });
    expect(first.state.snapshot.pc).toBe(1);

    const stepped = await backend.step({ sessionId, view: VIEW });
    expect(stepped.outcome).toEqual({ kind: "executed" });
    expect(stepped.state.snapshot.pc).toBe(2);
    expect(stepped.state.lastStop).toBeNull();

    // 次の命令(添字 2)にもブレークポイントがあるが、Step の直後の Run は止まらずに終わる
    const rest = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });
    expect(rest.stop).toEqual({ kind: "halted" });
    expect(rest.state.snapshot.stepsExecuted).toBe(3);
  });

  it("Reset は機械を初期状態へ戻し、ブレークポイントは保持して同じ位置で再び止まる", async () => {
    const { backend, sessionId } = await open("+++", { breakpointOffsets: [1] });
    await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });

    const reset = await backend.reset({ sessionId, view: VIEW });
    expect(reset.snapshot).toMatchObject({ pc: 0, dp: 0, stepsExecuted: 0, halted: false });
    expect(reset.lastStop).toBeNull();
    expect(reset.output.totalLen).toBe(0);

    const again = await backend.runBatch({ sessionId, batchSize: ALL, view: VIEW });
    expect(again.stop).toEqual({ kind: "breakpoint" });
    expect(again.executed).toBe(1);
  });
});
