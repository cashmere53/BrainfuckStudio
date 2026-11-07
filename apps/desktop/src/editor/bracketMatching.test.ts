import { type MatchResult, bracketMatching, matchBrackets } from "@codemirror/language";
import { EditorSelection, EditorState } from "@codemirror/state";
import { Decoration, EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { MAX_SOURCE_BYTES } from "../domain/limits";
import {
  BRACKET_MATCHING_CONFIG,
  bracketMatchingExtensions,
  renderBracketMatch,
} from "./bracketMatching";

const MATCHED_CLASS = "cm-bracket-matched";
const UNMATCHED_CLASS = "cm-bracket-unmatched";

function stateOf(doc: string): EditorState {
  return EditorState.create({ doc, extensions: bracketMatchingExtensions });
}

/**
 * 文字位置 index の括弧(`[` または `]`)の対応を求める。
 * 対応先が無い場合は partner を undefined にする。括弧でない位置は null。
 */
function pairOf(
  state: EditorState,
  index: number,
  config: Parameters<typeof matchBrackets>[3] = BRACKET_MATCHING_CONFIG,
): { partner: number | undefined; matched: boolean } | null {
  const ch = state.doc.sliceString(index, index + 1);
  let result: MatchResult | null;
  if (ch === "[") {
    result = matchBrackets(state, index, 1, config);
  } else if (ch === "]") {
    result = matchBrackets(state, index + 1, -1, config);
  } else {
    return null;
  }
  // 括弧だが対応先が無い場合、ライブラリは null を返すことがあるため非対応として扱う
  if (!result) return { partner: undefined, matched: false };
  return { partner: result.end?.from, matched: result.matched && result.end !== undefined };
}

function markClassesOf(ranges: readonly { value: Decoration; from: number; to: number }[]): string[] {
  return ranges.map((r) => String((r.value.spec as { class?: string }).class));
}

describe("bracketMatching の設定", () => {
  it("走査上限はソース上限(1 MiB)と一致し、対象は [ ] のみ", () => {
    expect(BRACKET_MATCHING_CONFIG.maxScanDistance).toBe(MAX_SOURCE_BYTES);
    expect(BRACKET_MATCHING_CONFIG.brackets).toBe("[]");
  });

  it("拡張一式を EditorState に組み込める", () => {
    expect(() => stateOf("[+]")).not.toThrow();
    expect(bracketMatching(BRACKET_MATCHING_CONFIG)).toBeDefined();
  });
});

describe("括弧対応の固定コーパス", () => {
  it("ネストした括弧は内側から外側まで正しい相手を指す", () => {
    const doc = "[>[-]<]";
    const state = stateOf(doc);
    expect(pairOf(state, 0)).toEqual({ partner: 6, matched: true });
    expect(pairOf(state, 2)).toEqual({ partner: 4, matched: true });
    expect(pairOf(state, 4)).toEqual({ partner: 2, matched: true });
    expect(pairOf(state, 6)).toEqual({ partner: 0, matched: true });
  });

  it("言語モードを設定しないため、コメント内の括弧も命令として対応づける", () => {
    // "note [ ] + ] end": 1 組目は コメント内の "[" と "]"、2 個目の "]" は対応する "[" が無い
    const doc = "note [ ] + ] end";
    const state = stateOf(doc);
    const open = doc.indexOf("[");
    const firstClose = doc.indexOf("]");
    const secondClose = doc.lastIndexOf("]");
    expect(pairOf(state, open)).toEqual({ partner: firstClose, matched: true });
    expect(pairOf(state, firstClose)).toEqual({ partner: open, matched: true });
    expect(pairOf(state, secondClose)).toEqual({ partner: undefined, matched: false });
  });

  it("対応する閉じ括弧が無い開き括弧と、対応する開き括弧が無い閉じ括弧を非対応として返す", () => {
    const state = stateOf("[[]");
    expect(pairOf(state, 0)).toEqual({ partner: undefined, matched: false });
    expect(pairOf(state, 1)).toEqual({ partner: 2, matched: true });
    expect(pairOf(state, 2)).toEqual({ partner: 1, matched: true });

    const lone = stateOf("a]");
    expect(pairOf(lone, 1)).toEqual({ partner: undefined, matched: false });
  });

  it("マルチバイト・サロゲートペア・CRLF を含んでも文書上の位置で対応を返す", () => {
    const state = stateOf("あ😀[\r\n+\r\n]");
    // EditorState は改行を LF に正規化するため、位置は正規化後の文書から求める
    const doc = state.doc.toString();
    const open = doc.indexOf("[");
    const close = doc.indexOf("]");
    expect(pairOf(state, open)).toEqual({ partner: close, matched: true });
    expect(pairOf(state, close)).toEqual({ partner: open, matched: true });
  });

  it("括弧以外の位置は対象外として null を返す", () => {
    const state = stateOf("+-<>.,");
    for (let i = 0; i < state.doc.length; i++) {
      expect(pairOf(state, i)).toBeNull();
    }
  });
});

describe("長い対応括弧の走査上限", () => {
  // CodeMirror は走査距離の上限をチャンク(行・葉)の境界でだけ判定するため、長い対応は複数行の文書で確認する
  const LINE = `${"+".repeat(999)}\n`;

  it("既定の走査距離(1 万文字)を超える対応を、設定では見つける", () => {
    const doc = `[${LINE.repeat(20)}]`;
    expect(doc.length).toBeGreaterThan(20_000);
    const state = stateOf(doc);
    expect(pairOf(state, 0)).toEqual({ partner: doc.length - 1, matched: true });
    expect(pairOf(state, doc.length - 1)).toEqual({ partner: 0, matched: true });
  });

  it("既定値のままでは同じ対応を見つけられないことを確認し、設定の必要性を示す", () => {
    const doc = `[${LINE.repeat(20)}]`;
    const state = stateOf(doc);
    expect(pairOf(state, 0, { brackets: "[]" })).toEqual({ partner: undefined, matched: false });
  });

  it("ソース上限いっぱい(1 MiB)の文書でも両端の対応を見つける", () => {
    // 先頭・末尾の括弧を含めて MAX_SOURCE_BYTES 文字ちょうど
    const body = LINE.repeat(1_048);
    const doc = `[${body}${"+".repeat(MAX_SOURCE_BYTES - 2 - body.length)}]`;
    expect(doc.length).toBe(MAX_SOURCE_BYTES);
    const state = stateOf(doc);
    expect(pairOf(state, 0)).toEqual({ partner: MAX_SOURCE_BYTES - 1, matched: true });
  });
});

describe("括弧の強調表現", () => {
  it("対応ありと対応なしで異なる装飾クラスを使い、対応なしは開き・閉じどちらも 1 文字だけを覆う", () => {
    const matched: MatchResult = { start: { from: 0, to: 1 }, end: { from: 4, to: 5 }, matched: true };
    const unmatched: MatchResult = { start: { from: 2, to: 3 }, matched: false };

    const matchedRanges = renderBracketMatch(matched);
    const unmatchedRanges = renderBracketMatch(unmatched);

    expect(matchedRanges).toHaveLength(2);
    expect(unmatchedRanges).toHaveLength(1);
    const matchedClass = markClassesOf(matchedRanges)[0];
    const unmatchedClass = markClassesOf(unmatchedRanges)[0];
    expect(matchedClass).toBeDefined();
    expect(unmatchedClass).toBeDefined();
    expect(matchedClass).not.toBe(unmatchedClass);
    expect(unmatchedRanges[0]).toMatchObject({ from: 2, to: 3 });
  });

  it("実際の matchBrackets の対応ありの結果は対応ありの装飾クラスになる", () => {
    const state = stateOf("[]");
    const result = matchBrackets(state, 0, 1, BRACKET_MATCHING_CONFIG);
    expect(result).not.toBeNull();
    if (!result) return;
    expect(result.matched).toBe(true);
    const classes = markClassesOf(renderBracketMatch(result));
    expect(classes).toEqual([MATCHED_CLASS, MATCHED_CLASS]);
  });

  it("実際の matchBrackets の対応なしの結果は対応なしの装飾クラスになる", () => {
    const state = stateOf("[+");
    const result = matchBrackets(state, 0, 1, BRACKET_MATCHING_CONFIG);
    expect(result).not.toBeNull();
    if (!result) return;
    expect(result.matched).toBe(false);
    const ranges = renderBracketMatch(result);
    expect(ranges).toHaveLength(1);
    expect(markClassesOf(ranges)).toEqual([UNMATCHED_CLASS]);
  });
});

// ---- 拡張を実際に載せた統合テスト ----

interface Highlight {
  from: number;
  to: number;
  cls: string;
}

/**
 * bracketMatchingExtensions を載せた EditorView を作り、カーソル位置 cursor で
 * 実際に表示される括弧強調の装飾(範囲とクラス名)を読み出す。
 * bracketMatching は ViewPlugin で装飾を供給するため、view を渡して facet の値を評価する。
 */
function highlightsAt(doc: string, cursor: number): Highlight[] {
  const state = EditorState.create({
    doc,
    selection: EditorSelection.cursor(cursor),
    extensions: bracketMatchingExtensions,
  });
  const parent = document.createElement("div");
  document.body.appendChild(parent);
  const view = new EditorView({ state, parent });
  try {
    const out: Highlight[] = [];
    for (const source of view.state.facet(EditorView.decorations)) {
      const set = typeof source === "function" ? source(view) : source;
      set.between(0, view.state.doc.length, (from, to, value) => {
        out.push({ from, to, cls: String((value.spec as { class?: string }).class) });
      });
    }
    return out.sort((a, b) => a.from - b.from || a.to - b.to);
  } finally {
    view.destroy();
    parent.remove();
  }
}

describe("拡張を載せたエディタでの括弧強調(統合)", () => {
  it("(a) ネストした括弧: カーソルが外側の [ の直後なら両端が matched になる", () => {
    // 0:[ 1:> 2:[ 3:- 4:] 5:< 6:]
    expect(highlightsAt("[>[-]<]", 0)).toEqual([
      { from: 0, to: 1, cls: MATCHED_CLASS },
      { from: 6, to: 7, cls: MATCHED_CLASS },
    ]);
  });

  it("(a) ネストした括弧: 内側の [ の直後・] の直後でも正しい相手を強調する", () => {
    // カーソル 3 は 2:[ の直後 → 相手は 4:]
    expect(highlightsAt("[>[-]<]", 3)).toEqual([
      { from: 2, to: 3, cls: MATCHED_CLASS },
      { from: 4, to: 5, cls: MATCHED_CLASS },
    ]);
    // カーソル 5 は 4:] の直後 → 相手は 2:[
    expect(highlightsAt("[>[-]<]", 5)).toEqual([
      { from: 2, to: 3, cls: MATCHED_CLASS },
      { from: 4, to: 5, cls: MATCHED_CLASS },
    ]);
  });

  it("(a) カーソルが括弧の直前(直後の [ )と直後(直前の ] )の両方にある場合も両方が強調される", () => {
    // 0:[ 1:] — カーソル 1 は直前が [、直後が ]
    expect(highlightsAt("[]", 1)).toEqual([
      { from: 0, to: 1, cls: MATCHED_CLASS },
      { from: 1, to: 2, cls: MATCHED_CLASS },
    ]);
  });

  it("(a) コメント文字に挟まれた括弧も命令として対応づける", () => {
    // 0-3:note 4:' ' 5:[ 6:' ' 7:] 8:' ' 9:+ 10:' ' 11:] 12:' ' 13-15:end
    const doc = "note [ ] + ] end";
    expect(highlightsAt(doc, 5)).toEqual([
      { from: 5, to: 6, cls: MATCHED_CLASS },
      { from: 7, to: 8, cls: MATCHED_CLASS },
    ]);
    expect(highlightsAt(doc, 8)).toEqual([
      { from: 5, to: 6, cls: MATCHED_CLASS },
      { from: 7, to: 8, cls: MATCHED_CLASS },
    ]);
  });

  it("(b) 対応のない [ は、その 1 文字だけに unmatched が 1 つ付く", () => {
    // 0:[ 1:[ 2:] — 外側の [ (0) には対応する ] が無い
    expect(highlightsAt("[[]", 0)).toEqual([{ from: 0, to: 1, cls: UNMATCHED_CLASS }]);
  });

  it("(b) 孤立した ] は、その 1 文字だけに unmatched が 1 つ付く", () => {
    // 0:a 1:] — 直前の ] に対応する [ が無い
    expect(highlightsAt("a]", 2)).toEqual([{ from: 1, to: 2, cls: UNMATCHED_CLASS }]);
  });

  it("(b) 不整合の ] (コメント内の [ ] の後) は unmatched、対応のある組は matched のまま", () => {
    // 0-3:note 4:' ' 5:[ 6:' ' 7:] 8:' ' 9:+ 10:' ' 11:] 12:' ' 13-15:end
    expect(highlightsAt("note [ ] + ] end", 12)).toEqual([
      { from: 11, to: 12, cls: UNMATCHED_CLASS },
    ]);
  });

  it("(c) 1 万文字を超えるソースでも対応先が文書の末尾の ] と正しく対応する", () => {
    // 1 行 1000 文字 × 20 行 + 括弧 2 個 = 20,002 文字(既定の走査距離 1 万文字を超える)
    const LINE = `${"+".repeat(999)}\n`;
    const doc = `[${LINE.repeat(20)}]`;
    expect(doc.length).toBeGreaterThan(20_000);
    const last = doc.length - 1;
    expect(highlightsAt(doc, 0)).toEqual([
      { from: 0, to: 1, cls: MATCHED_CLASS },
      { from: last, to: last + 1, cls: MATCHED_CLASS },
    ]);
    expect(highlightsAt(doc, doc.length)).toEqual([
      { from: 0, to: 1, cls: MATCHED_CLASS },
      { from: last, to: last + 1, cls: MATCHED_CLASS },
    ]);
  });

  it("(d) 括弧でない位置にカーソルがあれば装飾は出ない", () => {
    const doc = "+-<>.,";
    for (let cursor = 0; cursor <= doc.length; cursor++) {
      expect(highlightsAt(doc, cursor)).toEqual([]);
    }
  });

  it("(d) 括弧の隣でも、括弧以外の文字(丸括弧など)は強調対象にならない", () => {
    // Brainfuck の括弧命令 [ ] のみ対象。丸括弧 ( ) は強調されない
    expect(highlightsAt("(+)", 1)).toEqual([]);
    expect(highlightsAt("(+)", 3)).toEqual([]);
  });
});
