import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import {
  executionDecorations,
  executionField,
  parseErrorRange,
  rangeFromDto,
  scrollToExecution,
  setExecution,
  setParseError,
} from "./executionDecorations";

// マルチバイト(BMP・サロゲートペア)と CRLF / CR / LF を含む文書。
// EditorState は改行を LF に正規化するため、位置は正規化後の doc.toString() から求める。
const SOURCE = "あ😀+\r\n[-]\r-\n.";

function createState(doc: string = SOURCE): EditorState {
  return EditorState.create({ doc, extensions: [executionDecorations] });
}

function instructionPositions(state: EditorState): number[] {
  const text = state.doc.toString();
  const out: number[] = [];
  for (let i = 0; i < text.length; i++) {
    if ("+-<>[].,".includes(text[i] ?? "")) out.push(i);
  }
  return out;
}

function marks(state: EditorState): { from: number; to: number; cls: string }[] {
  const out: { from: number; to: number; cls: string }[] = [];
  state.field(executionField).between(0, state.doc.length, (from, to, value) => {
    out.push({ from, to, cls: String((value.spec as { class?: string }).class) });
  });
  return out;
}

describe("executionDecorations", () => {
  it("初期状態では装飾を出さない", () => {
    expect(marks(createState())).toEqual([]);
  });

  it("現在命令は対象の命令文字 1 文字だけを覆う(マルチバイト・CRLF・CR・LF を含む文書)", () => {
    const base = createState();
    const positions = instructionPositions(base);
    expect(positions.length).toBeGreaterThan(0);

    for (const pos of positions) {
      const state = base.update({
        effects: setExecution.of({ range: { from: pos, to: pos + 1 }, kind: "current" }),
      }).state;
      const applied = marks(state);
      expect(applied).toHaveLength(1);
      expect(applied[0]).toMatchObject({ from: pos, to: pos + 1 });
      expect(state.doc.sliceString(pos, pos + 1)).toMatch(/[+\-<>[\].,]/);
      expect(applied[0]?.cls).toContain("cm-execution-current");
    }
  });

  it("実行エラーの位置は別の装飾クラスで表示する", () => {
    const state = createState();
    const pos = instructionPositions(state)[0] ?? 0;
    const next = state.update({
      effects: setExecution.of({ range: { from: pos, to: pos + 1 }, kind: "runtimeError" }),
    }).state;
    const applied = marks(next);
    expect(applied).toHaveLength(1);
    expect(applied[0]?.cls).toContain("cm-execution-runtime-error");
    expect(applied[0]?.cls).not.toContain("cm-execution-current");
  });

  it("range が null なら現在命令の装飾だけを消し、解析エラーの装飾は残す", () => {
    const state = createState();
    const [first, second] = instructionPositions(state);
    const withBoth = state.update({
      effects: [
        setParseError.of({ from: second ?? 0, to: (second ?? 0) + 1 }),
        setExecution.of({ range: { from: first ?? 0, to: (first ?? 0) + 1 }, kind: "current" }),
      ],
    }).state;
    expect(marks(withBoth)).toHaveLength(2);

    const cleared = withBoth.update({
      effects: setExecution.of({ range: null, kind: "current" }),
    }).state;
    const remaining = marks(cleared);
    expect(remaining).toHaveLength(1);
    expect(remaining[0]).toMatchObject({ from: second, to: (second ?? 0) + 1 });
    expect(remaining[0]?.cls).toContain("cm-parse-error");
  });

  it("解析エラーの範囲を null にすると解析エラーの装飾を消す", () => {
    const state = createState();
    const pos = instructionPositions(state)[0] ?? 0;
    const withError = state.update({
      effects: setParseError.of({ from: pos, to: pos + 1 }),
    }).state;
    expect(marks(withError)).toHaveLength(1);
    expect(marks(withError.update({ effects: setParseError.of(null) }).state)).toEqual([]);
  });

  it("文書長を超える範囲と空の範囲は無視する", () => {
    const state = createState();
    const len = state.doc.length;
    const beyond = state.update({
      effects: setExecution.of({ range: { from: len, to: len + 1 }, kind: "current" }),
    }).state;
    expect(marks(beyond)).toEqual([]);

    const empty = state.update({
      effects: setExecution.of({ range: { from: 1, to: 1 }, kind: "current" }),
    }).state;
    expect(marks(empty)).toEqual([]);
  });

  it("編集で装飾の位置が追従する", () => {
    const state = createState();
    const pos = instructionPositions(state)[0] ?? 0;
    const highlighted = state.update({
      effects: setExecution.of({ range: { from: pos, to: pos + 1 }, kind: "current" }),
    }).state;

    const edited = highlighted.update({ changes: { from: 0, insert: "+" } }).state;
    const applied = marks(edited);
    expect(applied).toHaveLength(1);
    expect(applied[0]).toMatchObject({ from: pos + 1, to: pos + 2 });
    expect(edited.doc.sliceString(pos + 1, pos + 2)).toBe(state.doc.sliceString(pos, pos + 1));
  });
});

describe("範囲の変換", () => {
  it("SourceRangeDto の utf16Offset をそのまま半開区間にする", () => {
    expect(
      rangeFromDto({
        start: { utf16Offset: 3 },
        end: { utf16Offset: 4 },
      }),
    ).toEqual({ from: 3, to: 4 });
  });

  it("解析エラーの位置は括弧 1 文字の範囲にする", () => {
    expect(parseErrorRange({ utf16Offset: 7 })).toEqual({ from: 7, to: 8 });
  });
});

describe("scrollToExecution", () => {
  it("停止位置を nearest 方向で可視範囲へ入れるスクロール効果を返す", () => {
    const effect = scrollToExecution({ from: 5, to: 6 });
    expect(effect.value).toMatchObject({
      range: { from: 5, to: 6 },
      y: "nearest",
    });
  });
});
