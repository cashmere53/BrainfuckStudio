// @vitest-environment node
// フィクスチャの純粋な部分の単体テスト。転送関数の記録、閉じる要求、後始末の判定、終了状態の読み取りを、
// 偽の BridgeClient で確かめる(実際のブリッジとブラウザは使わない)。
import { describe, expect, it, vi } from "vitest";
import { BridgeExitedError, type BridgeClient, type InvokeOutcome } from "./bridge/bridgeClient.ts";
import {
  createBackendRecorder,
  requestCloseWith,
  settleExit,
  teardownReport,
} from "./fixtures.ts";

/** invoke だけを持つ偽のクライアント。応答は関数で決める */
function fakeClient(invoke: (cmd: string, payload: unknown) => Promise<InvokeOutcome>): Pick<BridgeClient, "invoke"> {
  return { invoke };
}

describe("createBackendRecorder: 転送と記録", () => {
  it("応答をそのまま返し、記録に cmd・payload・outcome を残す", async () => {
    const client = fakeClient(async () => ({ ok: true, value: 42 }));
    const recorder = createBackendRecorder(client);

    const outcome = await recorder.forward("load_source", { args: { text: "+" } });

    expect(outcome).toEqual({ ok: true, value: 42 });
    const calls = recorder.calls();
    expect(calls).toHaveLength(1);
    expect(calls[0]).toMatchObject({
      cmd: "load_source",
      payload: { args: { text: "+" } },
      outcome: { ok: true, value: 42 },
    });
    expect(calls[0]?.endedAt).not.toBeNull();
  });

  it("calls(cmd) は指定したコマンドの記録だけを返す", async () => {
    const client = fakeClient(async () => ({ ok: true, value: null }));
    const recorder = createBackendRecorder(client);
    await recorder.forward("set_document_dirty", { args: { dirty: true } });
    await recorder.forward("load_source", { args: {} });

    expect(recorder.calls("set_document_dirty").map((c) => c.cmd)).toEqual(["set_document_dirty"]);
    expect(recorder.calls().map((c) => c.cmd)).toEqual(["set_document_dirty", "load_source"]);
  });

  it("ブリッジが異常終了したら、成功の封筒ではなく既存の文言の失敗を返し、要求は止まらない", async () => {
    const client = fakeClient(async () => {
      throw new BridgeExitedError("ブリッジが終了しました(終了コード 1、シグナル なし)。");
    });
    const recorder = createBackendRecorder(client);

    const outcome = await recorder.forward("load_source", { args: {} });

    expect(outcome).toEqual({ ok: false, error: "bridge exited" });
    expect(recorder.calls()[0]?.outcome).toEqual({ ok: false, error: "bridge exited" });
  });

  it("異常終了以外の例外は、そのメッセージを失敗の文言にする", async () => {
    const client = fakeClient(async () => {
      throw new Error("想定外の応答です");
    });
    const recorder = createBackendRecorder(client);

    expect(await recorder.forward("load_source", {})).toEqual({ ok: false, error: "想定外の応答です" });
  });

  it("制御コマンド(e2e:)は中継するが記録しない", async () => {
    const invoke = vi.fn(async () => ({ ok: true, value: "pong" }) as const);
    const recorder = createBackendRecorder(fakeClient(invoke));

    const outcome = await recorder.forward("e2e:ping", null);

    expect(outcome).toEqual({ ok: true, value: "pong" });
    expect(invoke).toHaveBeenCalledWith("e2e:ping", null);
    expect(recorder.calls()).toEqual([]);
  });
});

describe("requestCloseWith: 閉じる要求", () => {
  it("確認が不要なら発火せず、確認なしで閉じた扱いを返す", async () => {
    const emit = vi.fn(async () => {});
    const invoke = vi.fn(async () => ({ ok: true, value: { confirmationRequired: false } }) as const);

    expect(await requestCloseWith(fakeClient(invoke), emit)).toBe("closedWithoutConfirmation");
    expect(emit).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("e2e:close_requested", null);
  });

  it("確認が必要なら閉じる要求のイベントを一度だけ発火し、eventEmitted を返す", async () => {
    const emit = vi.fn(async () => {});
    const client = fakeClient(async () => ({ ok: true, value: { confirmationRequired: true } }));

    expect(await requestCloseWith(client, emit)).toBe("eventEmitted");
    expect(emit).toHaveBeenCalledTimes(1);
  });

  it("確認の問い合わせが失敗したら例外にし、発火しない", async () => {
    const emit = vi.fn(async () => {});
    const client = fakeClient(async () => ({ ok: false, error: "x" }));

    await expect(requestCloseWith(client, emit)).rejects.toThrow("閉じる要求の確認が失敗しました");
    expect(emit).not.toHaveBeenCalled();
  });
});

describe("teardownReport: 後始末の問題", () => {
  const clean = { exit: null, stderrTail: "", unexpected: [], remaining: 0 } as const;

  it("問題が無ければ問題の一覧は空で、添付も無い", () => {
    expect(teardownReport(clean)).toEqual({ problems: [], stderrToAttach: null });
  });

  it("テスト中の異常終了を問題にし、終了コードと標準エラーの末尾を文面と添付に入れる", () => {
    const report = teardownReport({
      ...clean,
      exit: { code: 1, signal: null },
      stderrTail: "panic: 想定外の状態",
    });

    expect(report.problems).toHaveLength(1);
    expect(report.problems[0]).toContain("異常終了");
    expect(report.problems[0]).toContain("終了コード 1");
    expect(report.problems[0]).toContain("panic: 想定外の状態");
    expect(report.stderrToAttach).toBe("panic: 想定外の状態");
  });

  it("シグナルで終了した場合はシグナル名を文面に入れる", () => {
    const report = teardownReport({ ...clean, exit: { code: null, signal: "SIGKILL" } });

    expect(report.problems[0]).toContain("シグナル SIGKILL");
  });

  it("標準エラーが空なら、異常終了は問題にするが添付は作らない", () => {
    const report = teardownReport({ ...clean, exit: { code: 0, signal: null }, stderrTail: "  \n" });

    expect(report.problems).toHaveLength(1);
    expect(report.stderrToAttach).toBeNull();
  });

  it("終了していなければ異常終了の問題は出ない", () => {
    expect(teardownReport({ ...clean, stderrTail: "注意の出力" }).problems).toEqual([]);
  });

  it("予期しないダイアログを問題にし、種類を文面に入れる(添付は標準エラーがあるときだけ)", () => {
    const report = teardownReport({ ...clean, unexpected: ["open", "save"] });

    expect(report.problems).toEqual(["予期しないダイアログが開かれました: open, save"]);
    expect(report.stderrToAttach).toBeNull();
  });

  it("使われなかったダイアログを問題にし、件数を文面に入れる", () => {
    const report = teardownReport({ ...clean, remaining: 2 });

    expect(report.problems).toEqual(["指定したが使われなかったダイアログが 2 件あります"]);
  });

  it("問題が複数あれば、すべてを挙げる", () => {
    const report = teardownReport({
      exit: { code: 2, signal: null },
      stderrTail: "エラー",
      unexpected: ["open"],
      remaining: 1,
    });

    expect(report.problems).toHaveLength(3);
    expect(report.stderrToAttach).toBe("エラー");
  });
});

describe("settleExit: 終了の読み取り", () => {
  it("待つ間に終了が届けば、その終了状態を返す(exit を同期で読むと見逃す競合を避ける)", async () => {
    const state: { exit: { code: number | null; signal: NodeJS.Signals | null } | null } = { exit: null };
    setTimeout(() => {
      state.exit = { code: 3, signal: null };
    }, 5);

    expect(await settleExit({ get exit() { return state.exit; } }, 100)).toEqual({ code: 3, signal: null });
  });

  it("待っても終了しなければ null を返す", async () => {
    expect(await settleExit({ exit: null }, 5)).toBeNull();
  });
});
