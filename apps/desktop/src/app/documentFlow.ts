// 文書操作のフロー(要件 2.5〜2.13・2.7)。新規・開く・保存・閉じるを Backend と Workspace の上で組み立てる。React に依存しない。
//
// 方針:
// - 未保存の変更がある間の新規・開く・閉じるは、破棄の確認(pending)を経てから実行する。取消では何も変えない。
//   確認の表示中は他の要求を受けない(確認は modal のため)。閉じる要求は確認中でも受け付け、確認の種類を閉じるへ置き換える
//   (応答しないと Rust 側がウィンドウを閉じないため)。
// - ファイル操作(新規・開く・保存)は直列にする(busy)。保存先の選択ダイアログを重ねて開かない。
// - 未保存フラグ(Workspace の dirty)は変化のたびに `setDocumentDirty` で Rust 側へ送る。保存・開く・新規の後は
//   表示上の値を必ず送り直す(Rust 側の旗は保存・開く・新規で下がるため、表示と食い違わせない)。
// - 保存は保存時点のソースで未保存を解除する。保存中に編集されていたら未保存のまま残す(編集分を失わない)。
// - 失敗と成功の通知(置換読み込み・同期の失敗など)は `notify` へ流す。画面側は noticeAdded として表示する
//   (操作エラーではないため、実行ループの状態 phase を変えない)。
// - 閉じる要求: 未保存なら確認、未保存でなければ確認なしで終了を確定させる(Rust 側は応答が無いと閉じないため)。

import type { ApiError } from "../generated/ApiError";
import type { OpenFileResult } from "../generated/OpenFileResult";
import type { SaveFileResult } from "../generated/SaveFileResult";
import type { ClientNotice, Notice } from "../domain/notice";
import type { Backend } from "../ipc/backend";
import { normalizeApiError } from "../ipc/errors";
import type { WorkspaceStore } from "../state/workspace";

/** 確認ダイアログの種類(破棄を伴う操作) */
export type PendingConfirm = "new" | "open" | "close";

/** 確認ダイアログの文面(種類ごと) */
export const CONFIRM_TEXT: Record<PendingConfirm, { title: string; message: string; confirmLabel: string }> = {
  new: {
    title: "未保存の変更",
    message: "未保存の変更があります。破棄して新規文書を作成しますか。",
    confirmLabel: "破棄して新規",
  },
  open: {
    title: "未保存の変更",
    message: "未保存の変更があります。破棄してファイルを開きますか。",
    confirmLabel: "破棄して開く",
  },
  close: {
    title: "未保存の変更",
    message: "未保存の変更があります。破棄してウィンドウを閉じますか。",
    confirmLabel: "破棄して閉じる",
  },
};

/** 開いたファイルを置換文字で読み込んだことの通知(要件 2.12。成功の通知) */
export const LOSSY_REPLACED_NOTICE: ClientNotice = { kind: "client", code: "lossyReplaced" };

export type DocumentFlowDeps = {
  backend: Backend;
  workspace: WorkspaceStore;
  /** 失敗と成功の通知の出口(画面は noticeAdded として表示する。phase は変えない) */
  notify: (notice: Notice) => void;
  /** 確認ダイアログの表示対象が変わったときに呼ぶ(null は非表示) */
  setPending: (pending: PendingConfirm | null) => void;
  /**
   * 文書を新規・置換してよいか(実行の途中は false)。要求の時点と確定の時点で判定する。既定は常に true。
   * 実行中に置換すると古いプログラムのセッションと食い違うため、実行を止めて行わせる。
   */
  canEdit?: () => boolean;
};

export type DocumentFlow = {
  /** 新規。未保存なら確認を出す */
  requestNew(): Promise<void>;
  /** 開く。未保存なら確認を出し、ファイルの選択を Rust 側に任せる */
  requestOpen(): Promise<void>;
  /** 保存(`saveAs` が true なら保存先を尋ねる) */
  save(saveAs: boolean): Promise<void>;
  /** 確認ダイアログの確定: 破棄して、確認の対象の操作を行う */
  confirmPending(): Promise<void>;
  /** 確認ダイアログの取消: 何も変えない */
  cancelPending(): void;
  /** ファイル操作の進行中(保存先の選択・確認の表示を含む)か。実行の開始を控えるかの判定に使う */
  isBusy(): boolean;
  /** 購読を解除する(アンマウント・StrictMode の作り直し) */
  dispose(): void;
};

export function createDocumentFlow({
  backend,
  workspace,
  notify,
  setPending,
  canEdit = () => true,
}: DocumentFlowDeps): DocumentFlow {
  let pending: PendingConfirm | null = null;
  let busy = false;
  let disposed = false;

  const showPending = (next: PendingConfirm | null): void => {
    pending = next;
    setPending(next);
  };

  const fail = (rejection: unknown): void => {
    const error: ApiError = normalizeApiError(rejection);
    notify(error);
  };

  // 未保存フラグの同期。Rust 側の送信は順番に行う(直前の送信の完了を待ってから次を送る)
  let lastSentDirty = workspace.getSnapshot().document.dirty;
  let syncTail: Promise<void> = Promise.resolve();
  const sendDirty = (dirty: boolean): void => {
    syncTail = syncTail.then(async () => {
      try {
        await backend.setDocumentDirty(dirty);
      } catch (rejection: unknown) {
        if (!disposed) fail(rejection);
      }
    });
  };
  // 表示上の値を送り直す(保存・開く・新規の後。Rust 側の旗がそれらの操作で下がるため)
  const resyncDirty = (): void => {
    sendDirty(workspace.getSnapshot().document.dirty);
  };

  const unsubscribeWorkspace = workspace.subscribe(() => {
    if (disposed) return;
    const dirty = workspace.getSnapshot().document.dirty;
    if (dirty === lastSentDirty) return;
    lastSentDirty = dirty;
    sendDirty(dirty);
  });

  // 直列化: ファイル操作が走っている間は次を受けない
  const exclusive = async (task: () => Promise<void>): Promise<void> => {
    if (busy || pending !== null || disposed) return;
    busy = true;
    try {
      await task();
    } finally {
      busy = false;
    }
  };

  const doNew = async (): Promise<void> => {
    try {
      await backend.newDocument();
    } catch (rejection: unknown) {
      fail(rejection);
      resyncDirty();
      return;
    }
    workspace.newDocument();
    resyncDirty();
  };

  const doOpen = async (): Promise<void> => {
    let result: OpenFileResult;
    try {
      result = await backend.openSourceFile();
    } catch (rejection: unknown) {
      fail(rejection);
      resyncDirty();
      return;
    }
    if (result.kind === "cancelled") {
      resyncDirty();
      return;
    }
    // 置換は通知より先に行う(置換は通知を消すため。置換の後に示す)
    workspace.replaceDocument({ source: result.source, fileName: result.fileName, lossy: result.lossy });
    if (result.lossy) {
      notify(LOSSY_REPLACED_NOTICE);
    }
    resyncDirty();
  };

  const doSave = async (saveAs: boolean): Promise<void> => {
    const sent = workspace.getSnapshot().source;
    let result: SaveFileResult;
    try {
      result = await backend.saveSourceFile({ source: sent, saveAs });
    } catch (rejection: unknown) {
      fail(rejection);
      resyncDirty();
      return;
    }
    if (result.kind === "saved") {
      // 紐づけ先のファイル名は常に更新する(Rust 側は保存で新しいパスへ紐づけ済みのため、画面と食い違わせない)。
      // 未保存の解除は保存時点のソース(sent)と現在のソースの一致で決まる(保存中の編集は未保存のまま残る)
      workspace.markSaved(sent, result.fileName);
    }
    resyncDirty();
  };

  const doClose = async (): Promise<void> => {
    try {
      await backend.confirmExit();
    } catch (rejection: unknown) {
      fail(rejection);
    }
  };

  const requestNew = async (): Promise<void> => {
    if (busy || pending !== null || disposed || !canEdit()) return;
    if (workspace.getSnapshot().document.dirty) {
      showPending("new");
      return;
    }
    await exclusive(doNew);
  };

  const requestOpen = async (): Promise<void> => {
    if (busy || pending !== null || disposed || !canEdit()) return;
    if (workspace.getSnapshot().document.dirty) {
      showPending("open");
      return;
    }
    await exclusive(doOpen);
  };

  const save = (saveAs: boolean): Promise<void> => exclusive(() => doSave(saveAs));

  const confirmPending = async (): Promise<void> => {
    const action = pending;
    if (action === null || disposed) return;
    showPending(null);
    // 確認の表示後に実行が始まっていたら、置換せず取り消す(確定の時点で判定する)
    if (action !== "close" && !canEdit()) return;
    if (action === "new") {
      await exclusive(doNew);
    } else if (action === "open") {
      await exclusive(doOpen);
    } else {
      await doClose();
    }
  };

  const cancelPending = (): void => {
    if (pending === null) return;
    showPending(null);
  };

  const isBusy = (): boolean => busy || pending !== null;

  // 閉じる要求(Rust の close-requested)。未保存なら確認、未保存でなければ確認なしで終了を確定させる
  const onCloseRequested = (): void => {
    if (disposed) return;
    if (!workspace.getSnapshot().document.dirty) {
      void doClose();
      return;
    }
    showPending("close");
  };

  // 購読は非同期に解除関数を返す。破棄の後に届いた場合は即座に解除する
  let unlistenClose: (() => void) | null = null;
  backend.onCloseRequested(onCloseRequested).then(
    (unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        unlistenClose = unlisten;
      }
    },
    (rejection: unknown) => {
      if (!disposed) fail(rejection);
    },
  );

  const dispose = (): void => {
    disposed = true;
    unsubscribeWorkspace();
    if (unlistenClose !== null) {
      unlistenClose();
      unlistenClose = null;
    }
  };

  return { requestNew, requestOpen, save, confirmPending, cancelPending, isBusy, dispose };
}
