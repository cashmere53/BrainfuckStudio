import { render } from "@testing-library/react";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it, vi } from "vitest";
import { breakpointOffsets, toggleBreakpointAtLine } from "../editor/breakpoints";
import type { EditorDisplay } from "../editor/editorDisplay";
import { createWorkspace, type WorkspaceStore, workspaceStore } from "../state/workspace";
import { SourceEditor, type SourceEditorHandle, type SourceEditorProps } from "./SourceEditor";

const NONE: EditorDisplay = { execution: null, parseError: null, scrollRequest: false };

interface Harness {
  readonly store: WorkspaceStore;
  readonly handle: () => SourceEditorHandle | null;
  readonly onScrollConsumed: ReturnType<typeof vi.fn>;
  readonly onBreakpointRejected: ReturnType<typeof vi.fn>;
  readonly container: HTMLElement;
  /** 直前の props へ差分を重ねて再描画する */
  readonly update: (next: Partial<Omit<SourceEditorProps, "store">>) => void;
  readonly view: () => EditorView;
  readonly unmount: () => void;
}

/**
 * 新しい Workspace(初期ソースは `source`)と SourceEditor を描画する。
 * ワークスペースの内容はマウント前に置く(マウント後の内容変更は文書の置換で行う)。
 */
function setup(source = ""): Harness {
  const store = createWorkspace();
  if (source !== "") {
    store.replaceDocument({ source, fileName: null, lossy: false });
  }
  let handle: SourceEditorHandle | null = null;
  const onScrollConsumed = vi.fn();
  const onBreakpointRejected = vi.fn();
  let props: SourceEditorProps = {
    store,
    editable: true,
    display: NONE,
    onScrollConsumed,
    onBreakpointRejected,
  };
  const ui = (p: SourceEditorProps) => (
    <SourceEditor
      ref={(h) => {
        handle = h;
      }}
      {...p}
    />
  );
  const { container, rerender, unmount } = render(ui(props));
  const view = () => {
    const el = container.querySelector<HTMLElement>(".cm-editor");
    const found = el ? EditorView.findFromDOM(el) : null;
    if (!found) throw new Error("editor view not mounted");
    return found;
  };
  return {
    store,
    handle: () => handle,
    onScrollConsumed,
    onBreakpointRejected,
    container,
    update: (next) => {
      props = { ...props, ...next };
      rerender(ui(props));
    },
    view,
    unmount,
  };
}

describe("SourceEditor: ワークスペースとの双方向同期", () => {
  it("ワークスペースのソースで初期化する", () => {
    const { view } = setup("+-.");
    expect(view().state.doc.toString()).toBe("+-.");
  });

  it("文字入力は未保存状態としてワークスペースへ反映される", () => {
    const { store, view } = setup("+");
    view().dispatch({ changes: { from: 1, insert: "-" } });
    const snapshot = store.getSnapshot();
    expect(snapshot.source).toBe("+-");
    expect(snapshot.document.dirty).toBe(true);
  });

  it("CRLF の貼り付けは LF に正規化され、ワークスペースも同じ文字列になる", () => {
    const { store, view } = setup("+");
    view().dispatch({ changes: { from: 0, insert: "+\r\n-" } });
    expect(view().state.doc.toString()).toBe("+\n-+");
    expect(store.getSnapshot().source).toBe(view().state.doc.toString());
  });

  it("文書の置換(新規・開く)で内容を差し替え、ブレークポイントを空にし、未保存を解除する", () => {
    const { store, view } = setup("+-");
    store.setBreakpoints([0]);
    expect(breakpointOffsets(view().state)).toEqual([0]);

    store.replaceDocument({ source: "-->", fileName: "a.bf", lossy: false });
    expect(view().state.doc.toString()).toBe("-->");
    expect(breakpointOffsets(view().state)).toEqual([]);
    expect(store.getSnapshot().breakpoints).toEqual([]);
    expect(store.getSnapshot().document.dirty).toBe(false);
    expect(store.getSnapshot().source).toBe(view().state.doc.toString());
  });

  it("エディタでの切替はワークスペースのブレークポイントへ反映される", () => {
    const { store, view } = setup("+-");
    toggleBreakpointAtLine(view(), 1);
    expect(store.getSnapshot().breakpoints).toEqual([0]);
  });

  it("ワークスペース側の集合は、命令でない位置を除いてエディタと一致し、収束する", () => {
    const { store, view } = setup("+x-");
    store.setBreakpoints([0, 1]);
    expect(breakpointOffsets(view().state)).toEqual([0]);
    expect(store.getSnapshot().breakpoints).toEqual([0]);
  });
});

describe("SourceEditor: 実行中の編集不可", () => {
  it("編集不可のとき読み取り専用になり、利用者の切替も受理されない", () => {
    const { store, view, update } = setup("+-");
    update({ editable: false });
    expect(view().state.readOnly).toBe(true);
    expect(view().contentDOM.getAttribute("contenteditable")).toBe("false");

    expect(toggleBreakpointAtLine(view(), 1)).toBe(false);
    expect(store.getSnapshot().breakpoints).toEqual([]);
  });

  it("編集可に戻すと再び編集できる", () => {
    const { store, view, update } = setup("+");
    update({ editable: false });
    update({ editable: true });
    expect(view().state.readOnly).toBe(false);
    view().dispatch({ changes: { from: 1, insert: "-" } });
    expect(store.getSnapshot().document.dirty).toBe(true);
  });

  it("全解除ハンドルは編集不可のときは何もしない", () => {
    const { store, view, handle, update } = setup("+-");
    store.setBreakpoints([0]);
    expect(breakpointOffsets(view().state)).toEqual([0]);
    update({ editable: false });
    handle()?.clearBreakpoints();
    expect(breakpointOffsets(view().state)).toEqual([0]);
    expect(store.getSnapshot().breakpoints).toEqual([0]);
  });
});

describe("SourceEditor: 全解除ハンドル", () => {
  it("clearBreakpoints でエディタとワークスペースの集合を空にする", () => {
    const { store, view, handle } = setup("+-");
    store.setBreakpoints([0]);
    handle()?.clearBreakpoints();
    expect(breakpointOffsets(view().state)).toEqual([]);
    expect(store.getSnapshot().breakpoints).toEqual([]);
  });
});

describe("SourceEditor: 表示状態の反映", () => {
  it("現在命令を cm-execution-current で、解析エラー位置を cm-parse-error で示す", () => {
    const { container, update } = setup("+-.");
    update({ display: { execution: { range: { from: 1, to: 2 }, kind: "current" }, parseError: null, scrollRequest: false } });
    expect(container.querySelector(".cm-execution-current")?.textContent).toBe("-");

    update({ display: { execution: null, parseError: { from: 2, to: 3 }, scrollRequest: false } });
    expect(container.querySelector(".cm-execution-current")).toBeNull();
    expect(container.querySelector(".cm-parse-error")?.textContent).toBe(".");
  });

  it("実行エラー位置は cm-execution-runtime-error で示す", () => {
    const { container, update } = setup("+-.");
    update({ display: { execution: { range: { from: 0, to: 1 }, kind: "runtimeError" }, parseError: null, scrollRequest: false } });
    expect(container.querySelector(".cm-execution-runtime-error")?.textContent).toBe("+");
  });

  it("表示が消えたら装飾も消える(リセット・編集後の未開始)", () => {
    const { container, update } = setup("+-.");
    update({ display: { execution: { range: { from: 1, to: 2 }, kind: "current" }, parseError: null, scrollRequest: false } });
    update({ display: NONE });
    expect(container.querySelector(".cm-execution-current")).toBeNull();
  });

  it("スクロール要求を消費し、消費したことを通知する", () => {
    const { onScrollConsumed, update } = setup("+-.");
    update({ display: { execution: { range: { from: 2, to: 3 }, kind: "current" }, parseError: null, scrollRequest: true } });
    expect(onScrollConsumed).toHaveBeenCalledTimes(1);
  });

  it("スクロール要求が無いときは通知しない", () => {
    const { onScrollConsumed } = setup("+-.");
    expect(onScrollConsumed).not.toHaveBeenCalled();
  });
});

describe("SourceEditor: 既定ストアと改行の正規化", () => {
  it("既定ストアでマウントすると、文書とワークスペースのソースが一致し未保存にならない", () => {
    const { container } = render(<SourceEditor editable display={NONE} onScrollConsumed={vi.fn()} />);
    const el = container.querySelector<HTMLElement>(".cm-editor");
    const view = el ? EditorView.findFromDOM(el) : null;
    expect(view).not.toBeNull();
    const snapshot = workspaceStore.getSnapshot();
    expect(snapshot.source).not.toContain("\r");
    expect(view?.state.doc.toString()).toBe(snapshot.source);
    expect(snapshot.document.dirty).toBe(false);
  });

  it("CRLF と CR を含む文書の置換後も、文書とワークスペースのソースが一致し未保存にならない", () => {
    const { store, view } = setup("+");
    store.replaceDocument({ source: "+\r\n-\r.", fileName: "a.bf", lossy: false });
    expect(view().state.doc.toString()).toBe("+\n-\n.");
    expect(store.getSnapshot().source).toBe(view().state.doc.toString());
    expect(store.getSnapshot().document.dirty).toBe(false);
  });

  it("編集領域に「ソース」の aria-label を付ける", () => {
    const { view } = setup("+");
    expect(view().contentDOM.getAttribute("aria-label")).toBe("ソース");
  });

  it("1 回の編集・切替でワークスペースへの通知はちょうど 1 回ずつで、同期の往復は起きない", () => {
    const { store, view } = setup("+-");
    const listener = vi.fn();
    store.subscribe(listener);
    view().dispatch({ changes: { from: 1, insert: "-" } });
    expect(listener).toHaveBeenCalledTimes(1);
    toggleBreakpointAtLine(view(), 1);
    expect(listener).toHaveBeenCalledTimes(2);
  });
});

describe("SourceEditor: 後始末と購読解除", () => {
  it("アンマウント時にエディタを破棄する", () => {
    const { view, unmount } = setup("+-");
    const destroy = vi.spyOn(view(), "destroy");
    unmount();
    expect(destroy).toHaveBeenCalledTimes(1);
  });

  it("アンマウント後はストアの変更がエディタへ反映されない(購読を解除している)", () => {
    const { store, view, unmount } = setup("+-");
    const editor = view();
    unmount();
    const setState = vi.spyOn(editor, "setState");
    const dispatch = vi.spyOn(editor, "dispatch");
    store.replaceDocument({ source: "-->", fileName: null, lossy: false });
    store.setBreakpoints([0]);
    expect(setState).not.toHaveBeenCalled();
    expect(dispatch).not.toHaveBeenCalled();
  });
});

describe("SourceEditor: スクロール要求の消費", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("scrollRequest が true のときだけ現在命令へスクロールを発行する", () => {
    const { update } = setup("+-.");
    const scroll = vi.spyOn(EditorView, "scrollIntoView");
    const execution = { range: { from: 1, to: 2 }, kind: "current" as const };
    update({ display: { execution, parseError: null, scrollRequest: false } });
    expect(scroll).not.toHaveBeenCalled();
    update({ display: { execution, parseError: null, scrollRequest: true } });
    expect(scroll).toHaveBeenCalledTimes(1);
  });

  it("同じ範囲のまま scrollRequest が false から true へ戻ったら、再び消費する", () => {
    const { onScrollConsumed, update } = setup("+-.");
    const execution = { range: { from: 1, to: 2 }, kind: "current" as const };
    update({ display: { execution, parseError: null, scrollRequest: true } });
    expect(onScrollConsumed).toHaveBeenCalledTimes(1);
    update({ display: { execution, parseError: null, scrollRequest: false } });
    update({ display: { execution, parseError: null, scrollRequest: true } });
    expect(onScrollConsumed).toHaveBeenCalledTimes(2);
  });
});

describe("SourceEditor: 拒否の通知", () => {
  it("命令のない行の切替は onBreakpointRejected を呼ぶ", () => {
    const { view, onBreakpointRejected } = setup("+\nabc");
    expect(toggleBreakpointAtLine(view(), 2)).toBe(false);
    expect(onBreakpointRejected).toHaveBeenCalledWith("noInstruction");
  });
});
