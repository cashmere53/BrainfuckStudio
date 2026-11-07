import { describe, expect, it, vi } from "vitest";
import type { ApiError } from "../generated/ApiError";
import type { OpenFileResult } from "../generated/OpenFileResult";
import type { SaveFileResult } from "../generated/SaveFileResult";
import type { Notice } from "../domain/notice";
import { createFakeBackend, type CommandMethod, type FakeBackend, type FakeScript } from "../ipc/fakeBackend";
import { createWorkspace, type WorkspaceStore } from "../state/workspace";
import { createDocumentFlow, type PendingConfirm } from "./documentFlow";

const IO_FAILURE: ApiError = { kind: "io", message: "failed to write file: access denied", field: null };

/** 文書フローを組み立てる。pending と通知は記録して返す */
function setup(
  script: FakeScript = {},
  options: { dirty?: boolean; source?: string; canEdit?: () => boolean } = {},
) {
  // 文書操作の後は未保存フラグを必ず送り直すため、既定で受理する台本を入れる
  const backend: FakeBackend = createFakeBackend({ setDocumentDirty: () => undefined, ...script });
  const workspace: WorkspaceStore = createWorkspace();
  if (options.source !== undefined) {
    workspace.replaceDocument({ source: options.source, fileName: null, lossy: false });
  }
  if (options.dirty === true) {
    workspace.setSource(`${workspace.getSnapshot().source}+`);
  }
  let pending: PendingConfirm | null = null;
  const notices: Notice[] = [];
  const flow = createDocumentFlow({
    backend,
    workspace,
    notify: (notice) => notices.push(notice),
    canEdit: options.canEdit,
    setPending: (next) => {
      pending = next;
    },
  });
  return {
    backend,
    workspace,
    flow,
    notices,
    pending: () => pending,
  };
}

function calls(backend: FakeBackend, method: CommandMethod): number {
  return backend.calls.filter((call) => call.method === method).length;
}

/** 非同期の後続(dirty 同期・確定後の処理)が届くまで待つ */
async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("文書フロー: 新規(要件 2.6・2.7)", () => {
  it("未保存の変更が無ければ確認せず、空の文書にする", async () => {
    const { flow, backend, workspace, pending } = setup({ newDocument: () => undefined });

    await flow.requestNew();

    expect(calls(backend, "newDocument")).toBe(1);
    expect(workspace.getSnapshot().source).toBe("");
    expect(pending()).toBeNull();
  });

  it("未保存の変更があれば確認を出し、取消では何も変えない", async () => {
    const { flow, backend, workspace, pending } = setup({ newDocument: () => undefined }, { dirty: true });
    const before = workspace.getSnapshot().source;

    await flow.requestNew();
    expect(pending()).toBe("new");
    expect(calls(backend, "newDocument")).toBe(0);

    flow.cancelPending();
    expect(pending()).toBeNull();
    expect(calls(backend, "newDocument")).toBe(0);
    expect(workspace.getSnapshot().source).toBe(before);
    expect(workspace.getSnapshot().document.dirty).toBe(true);
  });

  it("確認で確定すると、破棄して空の文書にする", async () => {
    const { flow, backend, workspace, pending } = setup({ newDocument: () => undefined }, { dirty: true });

    await flow.requestNew();
    await flow.confirmPending();

    expect(pending()).toBeNull();
    expect(calls(backend, "newDocument")).toBe(1);
    expect(workspace.getSnapshot().source).toBe("");
    expect(workspace.getSnapshot().document.dirty).toBe(false);
  });
});

describe("文書フロー: 開く(要件 2.5・2.7・2.12・2.13)", () => {
  const opened = (overrides: Partial<Extract<OpenFileResult, { kind: "opened" }>> = {}): OpenFileResult => ({
    kind: "opened",
    source: "+++.\n",
    fileName: "hello.bf",
    lossy: false,
    ...overrides,
  });

  it("開いた内容で文書を置換し、ファイル名を紐づけ、未保存を解除する(ブレークポイントは空)", async () => {
    const { flow, workspace } = setup({ openSourceFile: () => opened() });
    workspace.setBreakpoints([0]);

    await flow.requestOpen();

    const snapshot = workspace.getSnapshot();
    expect(snapshot.source).toBe("+++.\n");
    expect(snapshot.document).toEqual({ fileName: "hello.bf", dirty: false, lossyNotice: false });
    expect(snapshot.breakpoints).toEqual([]);
  });

  it("未保存の変更があれば確認し、取消では開く操作を発行しない", async () => {
    const { flow, backend, pending } = setup({ openSourceFile: () => opened() }, { dirty: true });

    await flow.requestOpen();
    expect(pending()).toBe("open");

    flow.cancelPending();
    expect(calls(backend, "openSourceFile")).toBe(0);
  });

  it("確認で確定すると、破棄してから開く操作を発行する", async () => {
    const { flow, backend, pending } = setup({ openSourceFile: () => opened() }, { dirty: true });

    await flow.requestOpen();
    await flow.confirmPending();

    expect(pending()).toBeNull();
    expect(calls(backend, "openSourceFile")).toBe(1);
  });

  it("取消(ファイルを選ばなかった)では何も変えない", async () => {
    const { flow, workspace, notices } = setup({ openSourceFile: () => ({ kind: "cancelled" }) }, { source: "+." });

    await flow.requestOpen();

    expect(workspace.getSnapshot().source).toBe("+.");
    expect(notices).toEqual([]);
  });

  it("不正な UTF-8 を置換して読み込んだときは置換の通知を出す(要件 2.12)", async () => {
    const { flow, notices } = setup({ openSourceFile: () => opened({ lossy: true }) });

    await flow.requestOpen();

    expect(notices).toEqual([{ kind: "client", code: "lossyReplaced" }]);
  });

  it("読み込みの失敗は通知し、現在の文書を変えない(要件 2.11・2.13)", async () => {
    const tooLarge: ApiError = { kind: "fileTooLarge", message: "file is too large", field: null };
    const { flow, workspace, notices } = setup({ openSourceFile: () => Promise.reject(tooLarge) }, { source: "+." });

    await flow.requestOpen();

    expect(notices).toEqual([tooLarge]);
    expect(workspace.getSnapshot().source).toBe("+.");
  });
});

describe("文書フロー: 保存(要件 2.8・2.9・2.10・2.11)", () => {
  it("紐づけ先への保存は、保存した内容で未保存を解除し、ファイル名を更新する", async () => {
    const saved: SaveFileResult = { kind: "saved", fileName: "a.bf" };
    const { flow, backend, workspace } = setup({ saveSourceFile: () => saved }, { dirty: true });

    await flow.save(false);

    expect(backend.calls.find((call) => call.method === "saveSourceFile")?.args[0]).toEqual({
      source: workspace.getSnapshot().source,
      saveAs: false,
    });
    expect(workspace.getSnapshot().document).toEqual({ fileName: "a.bf", dirty: false, lossyNotice: false });
  });

  it("名前を付けて保存は saveAs を立てて要求する", async () => {
    const { flow, backend } = setup({ saveSourceFile: () => ({ kind: "saved", fileName: "b.bf" }) }, { dirty: true });

    await flow.save(true);

    expect(backend.calls.find((call) => call.method === "saveSourceFile")?.args[0]).toMatchObject({ saveAs: true });
  });

  it("保存先の選択が取り消されたら、編集内容と未保存の表示を維持する(要件 2.10)", async () => {
    const { flow, workspace, notices } = setup({ saveSourceFile: () => ({ kind: "cancelled" }) }, { dirty: true });
    const before = workspace.getSnapshot();

    await flow.save(false);

    expect(workspace.getSnapshot().document).toEqual(before.document);
    expect(workspace.getSnapshot().source).toBe(before.source);
    expect(notices).toEqual([]);
  });

  it("書き込みの失敗は通知し、未保存の表示を維持する(要件 2.11)", async () => {
    const { flow, workspace, notices } = setup({ saveSourceFile: () => Promise.reject(IO_FAILURE) }, { dirty: true });

    await flow.save(false);

    expect(notices).toEqual([IO_FAILURE]);
    expect(workspace.getSnapshot().document.dirty).toBe(true);
  });

  it("保存中にもう一度保存を要求しても、保存先の選択を二重に開かない", async () => {
    let finish: (result: SaveFileResult) => void = () => undefined;
    const { flow, backend } = setup(
      {
        saveSourceFile: () =>
          new Promise<SaveFileResult>((resolve) => {
            finish = resolve;
          }),
      },
      { dirty: true },
    );

    const first = flow.save(true);
    await flow.save(true);
    finish({ kind: "cancelled" });
    await first;

    expect(calls(backend, "saveSourceFile")).toBe(1);
  });
});

describe("文書フロー: 未保存フラグの同期(Rust 側の閉じる確認に使う)", () => {
  it("未保存の変化のたびに setDocumentDirty を送る", async () => {
    const { workspace, backend } = setup({ setDocumentDirty: () => undefined });

    workspace.setSource("+");
    await settle();
    workspace.markSaved(workspace.getSnapshot().source);
    await settle();

    expect(backend.calls.filter((call) => call.method === "setDocumentDirty").map((call) => call.args[0])).toEqual([
      true,
      false,
    ]);
  });

  it("同期の失敗は通知する", async () => {
    const { workspace, notices } = setup({ setDocumentDirty: () => Promise.reject(IO_FAILURE) });

    workspace.setSource("+");
    await settle();

    expect(notices).toEqual([IO_FAILURE]);
  });

  it("保存の後は、表示上の未保存の値を必ず Rust 側へ送り直す(保存で Rust 側の旗が下がるため)", async () => {
    const { flow, workspace, backend } = setup(
      { setDocumentDirty: () => undefined, saveSourceFile: () => ({ kind: "cancelled" }) },
      { dirty: true },
    );
    await settle();
    const sentBefore = calls(backend, "setDocumentDirty");

    await flow.save(false);
    await settle();

    expect(workspace.getSnapshot().document.dirty).toBe(true);
    expect(calls(backend, "setDocumentDirty")).toBeGreaterThan(sentBefore);
    expect(backend.calls[backend.calls.length - 1]?.args[0]).toBe(true);
  });
});

describe("文書フロー: ウィンドウを閉じる要求(要件 2.7・4.3 の確認)", () => {
  it("未保存なら確認を出し、確定で終了を確定させる", async () => {
    const { flow, backend, pending } = setup({ confirmExit: () => undefined, setDocumentDirty: () => undefined }, { dirty: true });

    backend.emitCloseRequested();
    await settle();
    expect(pending()).toBe("close");

    await flow.confirmPending();
    expect(calls(backend, "confirmExit")).toBe(1);
  });

  it("確認を取り消したら終了しない", async () => {
    const { flow, backend, pending } = setup({ confirmExit: () => undefined }, { dirty: true });

    backend.emitCloseRequested();
    await settle();
    flow.cancelPending();

    expect(pending()).toBeNull();
    expect(calls(backend, "confirmExit")).toBe(0);
  });

  it("未保存でなければ確認せずに終了を確定させる(応答しないと閉じられないため)", async () => {
    const { backend, pending } = setup({ confirmExit: () => undefined });

    backend.emitCloseRequested();
    await settle();

    expect(pending()).toBeNull();
    expect(calls(backend, "confirmExit")).toBe(1);
  });

  it("破棄の後は購読を解除し、以降の閉じる要求と未保存の変化を無視する", async () => {
    const { flow, backend, workspace } = setup({ setDocumentDirty: () => undefined, confirmExit: () => undefined });

    flow.dispose();
    backend.emitCloseRequested();
    workspace.setSource("+");
    await settle();

    expect(calls(backend, "confirmExit")).toBe(0);
    expect(calls(backend, "setDocumentDirty")).toBe(0);
  });
});

describe("文書フロー: 確認が出ている間の要求", () => {
  it("確認の表示中は新規・開くを重ねて受けない", async () => {
    const { flow, backend, pending } = setup({ newDocument: () => undefined, openSourceFile: () => ({ kind: "cancelled" }) }, { dirty: true });

    await flow.requestNew();
    await flow.requestOpen();

    expect(pending()).toBe("new");
    expect(calls(backend, "openSourceFile")).toBe(0);
  });

  it("確認を出したまま、通知の関数は呼ばれない(確認は通知ではない)", async () => {
    const notify = vi.fn();
    const backend = createFakeBackend({});
    const workspace = createWorkspace();
    workspace.setSource("+");
    const flow = createDocumentFlow({ backend, workspace, notify, setPending: () => undefined });

    await flow.requestNew();

    expect(notify).not.toHaveBeenCalled();
    flow.dispose();
  });
});

describe("文書フロー: 未保存フラグ送信の直列化", () => {
  it("先の送信が完了するまで次を送らず、完了の順が変化の順と一致する", async () => {
    const log: string[] = [];
    let releaseFirst: () => void = () => undefined;
    let first = true;
    const { workspace } = setup({
      setDocumentDirty: async (dirty) => {
        log.push(`start:${dirty}`);
        if (first) {
          first = false;
          await new Promise<void>((resolve) => {
            releaseFirst = resolve;
          });
        }
        log.push(`end:${dirty}`);
      },
    });

    workspace.setSource("+");
    workspace.markSaved(workspace.getSnapshot().source);
    await settle();
    expect(log).toEqual(["start:true"]);

    releaseFirst();
    await settle();
    expect(log).toEqual(["start:true", "end:true", "start:false", "end:false"]);
  });
});

describe("文書フロー: 保存中の編集(要件 2.5・2.9)", () => {
  it("保存中に編集していたら、紐づけ先のファイル名は更新し、未保存のまま残す(次の保存は保存先を尋ねない)", async () => {
    let finish: (result: SaveFileResult) => void = () => undefined;
    const { flow, backend, workspace } = setup(
      {
        setDocumentDirty: () => undefined,
        saveSourceFile: () =>
          new Promise<SaveFileResult>((resolve) => {
            finish = resolve;
          }),
      },
      { dirty: true },
    );

    const saving = flow.save(true);
    workspace.setSource(`${workspace.getSnapshot().source}.`);
    finish({ kind: "saved", fileName: "new.bf" });
    await saving;
    await settle();

    expect(workspace.getSnapshot().document).toEqual({ fileName: "new.bf", dirty: true, lossyNotice: false });
    expect(backend.calls.filter((call) => call.method === "setDocumentDirty").map((call) => call.args[0]).pop()).toBe(true);
  });
});

describe("文書フロー: 実行の途中の置換の抑止(最終検証の修正)", () => {
  it("実行の途中(canEdit が false)は新規・開くを受けず、ダイアログも出さない", async () => {
    const { flow, backend, pending } = setup({ newDocument: () => undefined }, { dirty: true, canEdit: () => false });

    await flow.requestNew();
    await flow.requestOpen();

    expect(calls(backend, "newDocument")).toBe(0);
    expect(calls(backend, "openSourceFile")).toBe(0);
    expect(pending()).toBeNull();
  });

  it("確認を出した後に実行が始まっても、確定時点で編集できなければ置換しない", async () => {
    let editable = true;
    const { flow, backend, workspace, pending } = setup(
      { openSourceFile: () => ({ kind: "opened", source: "+.", fileName: "b.bf", lossy: false }) },
      { dirty: true, canEdit: () => editable },
    );
    const sourceBefore = workspace.getSnapshot().source;
    await flow.requestOpen();
    expect(pending()).toBe("open");

    editable = false;
    await flow.confirmPending();
    await settle();

    expect(calls(backend, "openSourceFile")).toBe(0);
    expect(workspace.getSnapshot().source).toBe(sourceBefore);
    expect(pending()).toBeNull();
  });

  it("ファイル操作の進行中は isBusy が true で、完了すると false に戻る", async () => {
    const gate = deferredResult<OpenFileResult>();
    const { flow, backend } = setup({ openSourceFile: () => gate.promise });
    expect(flow.isBusy()).toBe(false);

    const opening = flow.requestOpen();
    await settle();
    expect(calls(backend, "openSourceFile")).toBe(1);
    expect(flow.isBusy()).toBe(true);

    gate.resolve({ kind: "cancelled" });
    await opening;
    expect(flow.isBusy()).toBe(false);
  });
});

function deferredResult<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("文書フロー: 確認の表示中の占有と、実行中の閉じるの確定(9.6 レビュー指摘)", () => {
  it("確認ダイアログの表示中は isBusy が true で、取消で false に戻る", async () => {
    const { flow, pending } = setup({ newDocument: () => undefined }, { dirty: true });
    expect(flow.isBusy()).toBe(false);

    await flow.requestNew();
    expect(pending()).toBe("new");
    expect(flow.isBusy()).toBe(true);

    flow.cancelPending();
    expect(flow.isBusy()).toBe(false);
  });

  it("実行中(canEdit が false)でも、閉じるの確認の確定は終了を握りつぶさず確定させる", async () => {
    const { flow, backend, pending } = setup(
      { confirmExit: () => undefined, setDocumentDirty: () => undefined },
      { dirty: true, canEdit: () => false },
    );

    backend.emitCloseRequested();
    await settle();
    expect(pending()).toBe("close");

    await flow.confirmPending();

    expect(calls(backend, "confirmExit")).toBe(1);
  });
});
