import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";

const BASE_PROPS = {
  title: "未保存の変更",
  message: "変更を破棄しますか?",
  confirmLabel: "破棄する",
  cancelLabel: "取消",
};

describe("ConfirmDialog", () => {
  it("open が false のときは何も描画しない", () => {
    render(<ConfirmDialog {...BASE_PROPS} open={false} onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("タイトルと本文を持つ alertdialog として描画する", () => {
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={vi.fn()} onCancel={vi.fn()} />);
    const dialog = screen.getByRole("alertdialog", { name: "未保存の変更" });
    expect(dialog.contains(screen.getByText("変更を破棄しますか?"))).toBe(true);
  });

  it("取消ボタンで onCancel が呼ばれ、onConfirm は呼ばれない", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={onConfirm} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("確定ボタンで onConfirm が呼ばれ、onCancel は呼ばれない", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={onConfirm} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "破棄する" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("Escape キーで取消として onCancel が呼ばれる", () => {
    const onCancel = vi.fn();
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={vi.fn()} onCancel={onCancel} />);
    fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("開いたとき初期フォーカスはダイアログ内の取消ボタンにある", () => {
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "取消" }));
  });

  it("末尾で Tab を押すと先頭へ、先頭で Shift+Tab を押すと末尾へ循環する", () => {
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={vi.fn()} onCancel={vi.fn()} />);
    const cancel = screen.getByRole("button", { name: "取消" });
    const confirm = screen.getByRole("button", { name: "破棄する" });

    confirm.focus();
    fireEvent.keyDown(confirm, { key: "Tab" });
    expect(document.activeElement).toBe(cancel);

    fireEvent.keyDown(cancel, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(confirm);
  });

  it("ダイアログ外にフォーカスが移ったら、ダイアログ内へ戻す", () => {
    render(
      <>
        <button type="button">外側</button>
        <ConfirmDialog {...BASE_PROPS} open onConfirm={vi.fn()} onCancel={vi.fn()} />
      </>,
    );
    const outside = screen.getByRole("button", { name: "外側" });
    outside.focus();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "取消" }));
  });

  it("閉じたとき、開く前にフォーカスされていた要素へフォーカスを戻す", () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            開く
          </button>
          <ConfirmDialog
            {...BASE_PROPS}
            open={open}
            onConfirm={() => setOpen(false)}
            onCancel={() => setOpen(false)}
          />
        </>
      );
    }
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "開く" });
    opener.focus();
    fireEvent.click(opener);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "取消" }));

    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("確定ボタンは取消ボタンと区別できる手がかり(クラスと data 属性)を持つ", () => {
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={vi.fn()} onCancel={vi.fn()} />);
    const cancel = screen.getByRole("button", { name: "取消" });
    const confirm = screen.getByRole("button", { name: "破棄する" });
    expect(confirm.classList.contains("confirm-dialog-confirm")).toBe(true);
    expect(confirm.getAttribute("data-cue")).toBe("destructive");
    expect(cancel.classList.contains("confirm-dialog-confirm")).toBe(false);
    expect(cancel.hasAttribute("data-cue")).toBe(false);
  });

  it("確定ボタンを連打しても onConfirm は 1 回しか呼ばれない(親が閉じないままでも)", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={onConfirm} onCancel={onCancel} />);
    const confirm = screen.getByRole("button", { name: "破棄する" });
    fireEvent.click(confirm);
    fireEvent.click(confirm);
    fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("確定後に Escape や取消を押しても onCancel は呼ばれない", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={onConfirm} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "破棄する" }));
    fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("取消後に確定を押しても onConfirm は呼ばれない", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ConfirmDialog {...BASE_PROPS} open onConfirm={onConfirm} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    fireEvent.click(screen.getByRole("button", { name: "破棄する" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("閉じてから開き直すと再び確定を受け付ける", () => {
    const onConfirm = vi.fn();
    const { rerender } = render(
      <ConfirmDialog {...BASE_PROPS} open onConfirm={onConfirm} onCancel={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "破棄する" }));
    rerender(<ConfirmDialog {...BASE_PROPS} open={false} onConfirm={onConfirm} onCancel={vi.fn()} />);
    rerender(<ConfirmDialog {...BASE_PROPS} open onConfirm={onConfirm} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "破棄する" }));
    expect(onConfirm).toHaveBeenCalledTimes(2);
  });
});
