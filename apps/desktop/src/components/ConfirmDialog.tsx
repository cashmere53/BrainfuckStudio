import "./ConfirmDialog.css";
import { type KeyboardEvent, useEffect, useId, useRef } from "react";

export interface ConfirmDialogProps {
  /** 表示中かどうか。false のときは何も描画せず、フォーカス管理も行わない。 */
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel?: string;
  /**
   * 確定ボタン・確定操作で呼ばれる。
   * 契約: 1 回の表示(open が true になってから次に false になるまで)につき、
   * 確定・取消のどちらか最初の一回だけ呼ばれ、以降の操作は無視される(連打・二重発火の防止)。
   * 親が閉じるまで表示が残るため、非同期処理を伴う確定でも二重に走らない。
   */
  onConfirm: () => void;
  /** 取消ボタン・Escape キーで呼ばれる。onConfirm と同じく 1 回の表示につき一回だけ。 */
  onCancel: () => void;
}

const FOCUSABLE_SELECTOR = "button:not(:disabled)";

/**
 * 未保存の変更を破棄してよいかを問うモーダル(要件 2.7)。
 * 初期フォーカスは安全側の「取消」に置き、Tab / Shift+Tab はダイアログ内で循環させる。
 * 閉じたときは開く前にフォーカスされていた要素へフォーカスを戻す。
 */
export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel = "取消",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const titleId = useId();
  const messageId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  // 1 回の表示で確定・取消のどちらかが済んだか(二重発火の防止)
  const settledRef = useRef(false);

  useEffect(() => {
    if (!open) return;
    settledRef.current = false;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    cancelRef.current?.focus();

    // ダイアログ外へフォーカスが移ったら、ダイアログ内へ戻す
    const onFocusIn = (event: FocusEvent) => {
      const dialog = dialogRef.current;
      if (dialog && event.target instanceof Node && !dialog.contains(event.target)) {
        cancelRef.current?.focus();
      }
    };
    document.addEventListener("focusin", onFocusIn);
    return () => {
      document.removeEventListener("focusin", onFocusIn);
      previous?.focus();
    };
  }, [open]);

  if (!open) return null;

  // 確定・取消は 1 回の表示で最初の一回だけ通す
  const settle = (action: () => void) => {
    if (settledRef.current) return;
    settledRef.current = true;
    action();
  };
  const handleCancel = () => settle(onCancel);
  const handleConfirm = () => settle(onConfirm);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      handleCancel();
      return;
    }
    if (event.key !== "Tab") return;
    const focusables = dialogRef.current?.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
    const first = focusables?.[0];
    const last = focusables?.[focusables.length - 1];
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <div className="confirm-dialog-backdrop">
      <div
        ref={dialogRef}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={messageId}
        className="confirm-dialog"
        onKeyDown={onKeyDown}
      >
        <h2 id={titleId}>{title}</h2>
        <p id={messageId}>{message}</p>
        <div className="confirm-dialog-actions">
          <button ref={cancelRef} type="button" onClick={handleCancel}>
            {cancelLabel}
          </button>
          <button
            type="button"
            className="confirm-dialog-confirm cue-marker"
            data-cue="destructive"
            onClick={handleConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
