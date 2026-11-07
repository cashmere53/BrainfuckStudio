import { EditorSelection, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView } from "@codemirror/view";

/** 文字位置の半開区間(UTF-16 単位。DTO の utf16Offset をそのまま用いる) */
export interface TextRange {
  readonly from: number;
  readonly to: number;
}

/** SourceRangeDto / SourcePositionDto から必要なフィールドだけを参照する構造型 */
export interface SourceRangeLike {
  readonly start: { readonly utf16Offset: number };
  readonly end: { readonly utf16Offset: number };
}

/** 実行側の強調種別: 現在命令 / 実行エラーを起こした命令 */
export type ExecutionKind = "current" | "runtimeError";

const EXECUTION_GROUP = "execution";

const CLASS_BY_KIND: Record<ExecutionKind, string> = {
  current: "cm-execution-current",
  runtimeError: "cm-execution-runtime-error",
};
const PARSE_ERROR_CLASS = "cm-parse-error";

/** 現在命令・実行エラー位置の装飾を設定する(null で消去) */
export const setExecution = StateEffect.define<{
  range: TextRange | null;
  kind: ExecutionKind;
}>();

/** 解析エラー位置の装飾を設定する(null で消去) */
export const setParseError = StateEffect.define<TextRange | null>();

function isUsable(range: TextRange, docLength: number): boolean {
  return (
    Number.isInteger(range.from) &&
    Number.isInteger(range.to) &&
    range.from >= 0 &&
    range.from < range.to &&
    range.to <= docLength
  );
}

/** 装飾を保持する StateField。実行系と解析エラー系は spec.group で区別する */
export const executionField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    let next = value.map(tr.changes);
    const docLength = tr.state.doc.length;
    for (const effect of tr.effects) {
      if (effect.is(setExecution)) {
        const { range, kind } = effect.value;
        const add =
          range && isUsable(range, docLength)
            ? [
                Decoration.mark({
                  class: CLASS_BY_KIND[kind],
                  group: EXECUTION_GROUP,
                }).range(range.from, range.to),
              ]
            : [];
        next = next.update({
          filter: (_from, _to, deco) => deco.spec.group !== EXECUTION_GROUP,
          add,
        });
      } else if (effect.is(setParseError)) {
        const range = effect.value;
        const add =
          range && isUsable(range, docLength)
            ? [Decoration.mark({ class: PARSE_ERROR_CLASS }).range(range.from, range.to)]
            : [];
        next = next.update({
          filter: (_from, _to, deco) => deco.spec.class !== PARSE_ERROR_CLASS,
          add,
        });
      }
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/**
 * 色以外の手がかり(下線・枠・太字)で現在命令・実行エラー・解析エラーを区別するテーマ。
 */
const executionTheme = EditorView.baseTheme({
  ".cm-execution-current": {
    textDecoration: "underline 2px",
    outline: "2px solid currentColor",
    fontWeight: "bold",
  },
  ".cm-execution-runtime-error": {
    textDecoration: "underline wavy",
    outline: "2px dashed currentColor",
    fontWeight: "bold",
  },
  ".cm-parse-error": {
    textDecoration: "underline wavy",
    outline: "2px dotted currentColor",
    fontWeight: "bold",
  },
});

/** 現在命令・エラー位置の装飾を提供する拡張一式 */
export const executionDecorations = [executionField, executionTheme];

/**
 * 停止時に対象範囲を可視範囲へ入れるスクロール効果。
 * 停止したときだけ dispatch に含めること(実行中の更新では含めない)。
 */
export function scrollToExecution(range: TextRange): ReturnType<typeof EditorView.scrollIntoView> {
  return EditorView.scrollIntoView(EditorSelection.range(range.from, range.to), {
    y: "nearest",
  });
}

/** DTO の半開区間を文字範囲へ写す */
export function rangeFromDto(range: SourceRangeLike): TextRange {
  return { from: range.start.utf16Offset, to: range.end.utf16Offset };
}

/** 解析エラーの括弧位置(1 文字)を文字範囲へ写す */
export function parseErrorRange(position: { readonly utf16Offset: number }): TextRange {
  return { from: position.utf16Offset, to: position.utf16Offset + 1 };
}
