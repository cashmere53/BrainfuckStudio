import { type Config, type MatchResult, bracketMatching } from "@codemirror/language";
import type { Range } from "@codemirror/state";
import { Decoration, EditorView } from "@codemirror/view";
import { MAX_SOURCE_BYTES } from "../domain/limits";

const MATCHED_CLASS = "cm-bracket-matched";
const UNMATCHED_CLASS = "cm-bracket-unmatched";

const matchedMark = Decoration.mark({ class: MATCHED_CLASS });
const unmatchedMark = Decoration.mark({ class: UNMATCHED_CLASS });

/**
 * 対応する括弧を強調する。対応なしの括弧は別クラス(波線+太字)で 1 文字だけを覆う。
 * 色だけに頼らないよう、対応ありは細い実線の枠、対応なしは波線の下線で区別する。
 */
export function renderBracketMatch(match: MatchResult): readonly Range<Decoration>[] {
  const mark = match.matched ? matchedMark : unmatchedMark;
  const ranges: Range<Decoration>[] = [mark.range(match.start.from, match.start.to)];
  if (match.end) {
    ranges.push(mark.range(match.end.from, match.end.to));
  }
  return ranges;
}

/**
 * 括弧対応の設定。言語モードを使わないため、コメント内の括弧も命令として扱う。
 * 対象は Brainfuck の括弧命令 [ ] のみ(既定の "()[]{}" は使わない)。
 * 走査上限はソース上限と一致させ、どのソースでも対応を見つけられるようにする。
 */
export const BRACKET_MATCHING_CONFIG: Config = {
  brackets: "[]",
  maxScanDistance: MAX_SOURCE_BYTES,
  renderMatch: renderBracketMatch,
};

const bracketTheme = EditorView.baseTheme({
  [`.${MATCHED_CLASS}`]: {
    outline: "1px solid currentColor",
    textDecoration: "underline dotted",
  },
  [`.${UNMATCHED_CLASS}`]: {
    textDecoration: "underline wavy",
    fontWeight: "bold",
  },
});

/** 括弧対応の強調を提供する拡張一式 */
export const bracketMatchingExtensions = [bracketMatching(BRACKET_MATCHING_CONFIG), bracketTheme];
