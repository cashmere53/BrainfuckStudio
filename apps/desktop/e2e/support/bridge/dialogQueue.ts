// テストが指定したダイアログの結果を先入れ先出しで返す待ち行列。
// 指定が無い・種類が違う問い合わせは unexpected に記録し、取消を返す(後始末で失敗にする)。
// 純粋な関数だけを置く(プロセスや転送路には触れない)。
import type { DialogKind, DialogResult } from "./protocol.ts";

export interface HeldDialog {
  /** 次の同種の問い合わせが届いたときに解決する */
  readonly requested: Promise<void>;
  /** 問い合わせに返す結果を決める。到着の前に呼んでもよい */
  release(result: DialogResult): void;
}

export interface DialogQueue {
  /** 次の同種の問い合わせに返す */
  enqueue(kind: DialogKind, result: DialogResult): void;
  /** 次の同種の問い合わせを release まで待たせる */
  hold(kind: DialogKind): HeldDialog;
  /** BridgeClient の onDialog から呼ぶ */
  take(kind: DialogKind): Promise<DialogResult>;
  /** 結果が無い・種類が違う問い合わせ(到着順) */
  readonly unexpected: readonly DialogKind[];
  /** 使われなかった指定の数 */
  readonly remaining: number;
}

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
};

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

type Entry =
  | { readonly kind: DialogKind; readonly result: DialogResult }
  | {
      readonly kind: DialogKind;
      readonly requested: Deferred<void>;
      readonly outcome: Deferred<DialogResult>;
    };

const CANCEL: DialogResult = { kind: "cancel" };

export function createDialogQueue(): DialogQueue {
  const entries: Entry[] = [];
  const unexpected: DialogKind[] = [];

  return {
    enqueue(kind, result) {
      entries.push({ kind, result });
    },

    hold(kind) {
      const requested = deferred<void>();
      const outcome = deferred<DialogResult>();
      entries.push({ kind, requested, outcome });
      return {
        requested: requested.promise,
        release(result) {
          outcome.resolve(result);
        },
      };
    },

    take(kind) {
      const head = entries[0];
      if (head === undefined || head.kind !== kind) {
        unexpected.push(kind);
        return Promise.resolve(CANCEL);
      }
      entries.shift();
      if ("result" in head) {
        return Promise.resolve(head.result);
      }
      head.requested.resolve();
      return head.outcome.promise;
    },

    get unexpected() {
      return unexpected;
    },

    get remaining() {
      return entries.length;
    },
  };
}
