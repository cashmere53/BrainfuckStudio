import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  type BreakpointRejection,
  breakpointExtensions,
  breakpointOffsets,
  setBreakpointOffsets,
  toggleBreakpointAtLine,
} from "./breakpoints";

interface Harness {
  readonly view: EditorView;
  readonly changes: number[][];
  readonly rejected: BreakpointRejection[];
}

let views: EditorView[] = [];

function setup(doc: string, readOnly = false): Harness {
  const changes: number[][] = [];
  const rejected: BreakpointRejection[] = [];
  const state = EditorState.create({
    doc,
    extensions: [
      breakpointExtensions({
        onChange: (offsets) => changes.push([...offsets]),
        onRejected: (reason) => rejected.push(reason),
      }),
      EditorState.readOnly.of(readOnly),
    ],
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

describe("setBreakpointOffsets(外部からの集合の置換)", () => {
  it("命令文字の位置だけを昇順で設定し、変化を通知する", () => {
    const { view, changes } = setup("+-[]");
    setBreakpointOffsets(view, [3, 1]);
    expect(breakpointOffsets(view.state)).toEqual([1, 3]);
    expect(changes).toEqual([[1, 3]]);
  });

  it("命令でない位置・範囲外の位置は取り除いて設定する(通知は取り除いた後の集合)", () => {
    const { view, changes } = setup("+x-");
    setBreakpointOffsets(view, [0, 1, 2, 99]);
    expect(breakpointOffsets(view.state)).toEqual([0, 2]);
    expect(changes).toEqual([[0, 2]]);
  });

  it("既に同じ集合なら何もせず、通知もしない", () => {
    const { view, changes } = setup("+-");
    setBreakpointOffsets(view, [0]);
    setBreakpointOffsets(view, [0]);
    expect(changes).toEqual([[0]]);
  });

  it("空の配列で全解除できる", () => {
    const { view, changes } = setup("+-");
    setBreakpointOffsets(view, [0, 1]);
    setBreakpointOffsets(view, []);
    expect(breakpointOffsets(view.state)).toEqual([]);
    expect(changes).toEqual([[0, 1], []]);
  });

  it("読み取り専用(実行中)でも外部からの置換は適用される", () => {
    const { view, changes } = setup("+-", true);
    setBreakpointOffsets(view, [1]);
    expect(breakpointOffsets(view.state)).toEqual([1]);
    expect(changes).toEqual([[1]]);
  });
});

describe("読み取り専用中の利用者操作", () => {
  it("ガタークリック相当の切替は受理されず、集合も通知も変わらない", () => {
    const { view, changes, rejected } = setup("+-", true);
    expect(toggleBreakpointAtLine(view, 1)).toBe(false);
    expect(breakpointOffsets(view.state)).toEqual([]);
    expect(changes).toEqual([]);
    expect(rejected).toEqual([]);
  });
});
