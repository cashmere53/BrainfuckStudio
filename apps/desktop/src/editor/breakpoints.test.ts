import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  type BreakpointRejection,
  breakpointExtensions,
  breakpointOffsets,
  clearAllBreakpoints,
  toggleBreakpointAtCursor,
  toggleBreakpointAtLine,
} from "./breakpoints";

interface Harness {
  readonly view: EditorView;
  readonly changes: number[][];
  readonly rejected: BreakpointRejection[];
}

let views: EditorView[] = [];

/** テスト用のエディタを作る。onChange / onRejected の呼び出しを記録する */
function setup(doc: string, cursor = 0): Harness {
  const changes: number[][] = [];
  const rejected: BreakpointRejection[] = [];
  const state = EditorState.create({
    doc,
    selection: EditorSelection.cursor(cursor),
    extensions: breakpointExtensions({
      onChange: (offsets) => changes.push([...offsets]),
      onRejected: (reason) => rejected.push(reason),
    }),
  });
  const view = new EditorView({ state, parent: document.body });
  views.push(view);
  return { view, changes, rejected };
}

beforeEach(() => {
  views = [];
});

afterEach(() => {
  for (const view of views) {
    view.destroy();
  }
  views = [];
});

describe("ガタークリック(行内で最初の命令)", () => {
  it("行内の最初の命令にブレークポイントを置き、もう一度で外す", () => {
    const { view, changes } = setup("ab+-\n+");
    expect(toggleBreakpointAtLine(view, 1)).toBe(true);
    expect(breakpointOffsets(view.state)).toEqual([2]);
    expect(changes).toEqual([[2]]);

    expect(toggleBreakpointAtLine(view, 1)).toBe(true);
    expect(breakpointOffsets(view.state)).toEqual([]);
    expect(changes).toEqual([[2], []]);
  });

  it("命令の無い行(コメントのみ・空行)では拒否し、集合と通知を変えない", () => {
    const { view, changes, rejected } = setup("hello\n\n+");
    expect(toggleBreakpointAtLine(view, 1)).toBe(false);
    expect(toggleBreakpointAtLine(view, 2)).toBe(false);
    expect(rejected).toEqual(["noInstruction", "noInstruction"]);
    expect(breakpointOffsets(view.state)).toEqual([]);
    expect(changes).toEqual([]);
  });

  it("次の行の命令へ正規化しない(後続行に命令があっても拒否する)", () => {
    const { view, rejected } = setup("comment\n+");
    expect(toggleBreakpointAtLine(view, 1)).toBe(false);
    expect(rejected).toEqual(["noInstruction"]);
    expect(breakpointOffsets(view.state)).toEqual([]);
  });
});

describe("キーによる切替(カーソル以降の最初の命令)", () => {
  it("カーソル位置が命令でなければ後続の最初の命令(改行をまたぐ)へ置く", () => {
    // 0:'a' 1:'\n' 2:'b' 3:'\n' 4:'-'
    const { view, changes } = setup("a\nb\n-", 0);
    expect(toggleBreakpointAtCursor(view)).toBe(true);
    expect(breakpointOffsets(view.state)).toEqual([4]);
    expect(changes).toEqual([[4]]);
  });

  it("カーソル以降に命令が無ければ拒否する", () => {
    const { view, rejected, changes } = setup("+ trailing text", 2);
    expect(toggleBreakpointAtCursor(view)).toBe(false);
    expect(rejected).toEqual(["noInstruction"]);
    expect(changes).toEqual([]);
  });

  it("既存のブレークポイントに当たる場合は外す", () => {
    const { view, changes } = setup("+>", 0);
    toggleBreakpointAtCursor(view);
    toggleBreakpointAtCursor(view);
    expect(breakpointOffsets(view.state)).toEqual([]);
    expect(changes).toEqual([[0], []]);
  });
});

describe("編集への追従", () => {
  it("前方への挿入で位置が移動する", () => {
    const { view, changes } = setup("+", 0);
    toggleBreakpointAtLine(view, 1);
    view.dispatch({ changes: { from: 0, insert: "abc" } });
    expect(breakpointOffsets(view.state)).toEqual([3]);
    expect(view.state.sliceDoc(3, 4)).toBe("+");
    expect(changes).toEqual([[0], [3]]);
  });

  it("対象の命令文字を削除すると除去され、通知される", () => {
    const { view, changes } = setup("a+b", 0);
    toggleBreakpointAtLine(view, 1);
    view.dispatch({ changes: { from: 1, to: 2 } });
    expect(breakpointOffsets(view.state)).toEqual([]);
    expect(changes).toEqual([[1], []]);
  });

  it("対象の命令文字をコメント文字に置き換える(コメント化)と除去される", () => {
    const { view, changes } = setup("+>", 0);
    toggleBreakpointAtLine(view, 1);
    view.dispatch({ changes: { from: 0, to: 1, insert: "x" } });
    expect(breakpointOffsets(view.state)).toEqual([]);
    expect(changes).toEqual([[0], []]);
  });

  it("マルチバイト・サロゲートペア・CRLF を含む文書でも UTF-16 位置を保つ", () => {
    // EditorState は改行を LF に正規化する: "あ😀+\nー-" → '+' は 3 (あ=1, 😀=2)、'-' は 6
    const { view } = setup("あ😀+\r\nー-", 0);
    toggleBreakpointAtLine(view, 1);
    toggleBreakpointAtLine(view, 2);
    expect(breakpointOffsets(view.state)).toEqual([3, 6]);
    expect(view.state.sliceDoc(3, 4)).toBe("+");
    expect(view.state.sliceDoc(6, 7)).toBe("-");
  });

  it("命令でない文字への変更が無関係な位置の BP を消さない", () => {
    // 0:'+' 1:' ' 2:'x' 3:' ' 4:'-'
    const { view } = setup("+ x -", 0);
    toggleBreakpointAtLine(view, 1); // 位置 0
    view.dispatch({ selection: { anchor: 1 } });
    toggleBreakpointAtCursor(view); // カーソル(1)以降の最初の命令 '-' (位置 4)
    expect(breakpointOffsets(view.state)).toEqual([0, 4]);
    view.dispatch({ changes: { from: 2, to: 3, insert: "y" } });
    expect(breakpointOffsets(view.state)).toEqual([0, 4]);
  });
});

describe("通知", () => {
  it("集合が変化したときだけ位置列を通知する(後方への挿入では通知しない)", () => {
    const { view, changes } = setup("+", 0);
    toggleBreakpointAtLine(view, 1);
    view.dispatch({ changes: { from: 1, insert: "xyz" } });
    expect(breakpointOffsets(view.state)).toEqual([0]);
    expect(changes).toEqual([[0]]);
  });

  it("文字入力でブレークポイントが無い場合は通知しない", () => {
    const { view, changes } = setup("", 0);
    view.dispatch({ changes: { from: 0, insert: "+-" } });
    expect(changes).toEqual([]);
  });
});

describe("全解除", () => {
  it("すべてのブレークポイントを外し、1 回通知する", () => {
    const { view, changes } = setup("+-\n.", 0);
    toggleBreakpointAtLine(view, 1);
    toggleBreakpointAtLine(view, 2);
    clearAllBreakpoints(view);
    expect(breakpointOffsets(view.state)).toEqual([]);
    expect(changes).toEqual([[0], [0, 3], []]);
  });

  it("ブレークポイントが無いときは通知しない", () => {
    const { view, changes } = setup("+", 0);
    clearAllBreakpoints(view);
    expect(changes).toEqual([]);
  });
});
