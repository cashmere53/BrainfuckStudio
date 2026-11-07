// ブレークポイントの保持・表示・編集追従(要件 6.1〜6.4, 6.8, 6.9, 7.7)。
// 集合の単位は「命令文字 1 文字」。位置はドキュメント先頭からの UTF-16 オフセット。
// 編集で位置を写像し、対象文字が削除/命令でなくなった場合は除去する。
// 位置の解決(スナップ規則)は domain/instructionScan に委譲する。

import {
  type EditorState,
  type Extension,
  Facet,
  RangeSet,
  type Text,
  StateEffect,
  StateField,
} from "@codemirror/state";
import {
  type Command,
  Decoration,
  type DecorationSet,
  EditorView,
  GutterMarker,
  gutter,
  keymap,
} from "@codemirror/view";
import {
  INSTRUCTION_CHARS,
  firstInstructionFrom,
  firstInstructionInLine,
} from "../domain/instructionScan";

/** 拒否の理由。現状は「その位置に命令が無い」のみ */
export type BreakpointRejection = "noInstruction";

export interface BreakpointHandlers {
  /** 集合が変化したときのみ、昇順の UTF-16 位置列を通知する */
  readonly onChange: (offsets: number[]) => void;
  /** 命令が見つからず設定できなかったときに呼ぶ(通知は呼び出し側) */
  readonly onRejected: (reason: BreakpointRejection) => void;
}

const NOOP_HANDLERS: BreakpointHandlers = {
  onChange: () => undefined,
  onRejected: () => undefined,
};

const handlersFacet = Facet.define<BreakpointHandlers, BreakpointHandlers>({
  combine: (values) => values[values.length - 1] ?? NOOP_HANDLERS,
});

/** 命令文字 1 文字を覆う装飾。枠・下線で色以外にも区別する */
const breakpointCharMark = Decoration.mark({ class: "cm-breakpoint-char" });

/** 位置 `pos` にブレークポイントを置く/外すを指示する(位置は変更後のドキュメント基準) */
const toggleEffect = StateEffect.define<number>();
/** 全解除 */
const clearEffect = StateEffect.define<null>();
/** 外部(ワークスペース)からの集合の置換。命令でない位置は除く */
const setEffect = StateEffect.define<readonly number[]>();

function isInstructionAt(doc: Text, from: number): boolean {
  return from >= 0 && from < doc.length && INSTRUCTION_CHARS.includes(doc.sliceString(from, from + 1));
}

function hasBreakpointAt(set: DecorationSet, pos: number): boolean {
  let found = false;
  set.between(pos, pos + 1, (from) => {
    if (from === pos) {
      found = true;
      return false;
    }
    return undefined;
  });
  return found;
}

/** ブレークポイント集合の StateField(命令文字 1 文字ずつの装飾として保持) */
const breakpointField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    let next = value;
    if (tr.docChanged) {
      next = next.map(tr.changes);
      const doc = tr.state.doc;
      // 対象文字の削除(範囲が潰れる)と、命令でなくなった(コメント化)ものを除去する
      next = next.update({
        filter: (from, to) => to - from === 1 && isInstructionAt(doc, from),
      });
    }
    for (const effect of tr.effects) {
      if (effect.is(clearEffect)) {
        next = Decoration.none;
      } else if (effect.is(setEffect)) {
        const doc = tr.state.doc;
        const targets = [...new Set(effect.value)].filter((pos) => isInstructionAt(doc, pos));
        next = Decoration.set(
          targets.map((pos) => breakpointCharMark.range(pos, pos + 1)),
          true,
        );
      } else if (effect.is(toggleEffect)) {
        const pos = effect.value;
        if (!isInstructionAt(tr.state.doc, pos)) {
          continue;
        }
        if (hasBreakpointAt(next, pos)) {
          next = next.update({
            filterFrom: pos,
            filterTo: pos + 1,
            filter: (from) => from !== pos,
          });
        } else {
          next = next.update({
            add: [breakpointCharMark.range(pos, pos + 1)],
            sort: true,
          });
        }
      }
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** 現在のブレークポイント位置(UTF-16、昇順)。 */
export function breakpointOffsets(state: EditorState): number[] {
  const out: number[] = [];
  state.field(breakpointField).between(0, state.doc.length, (from) => {
    out.push(from);
  });
  return out;
}

class BreakpointGutterMarker extends GutterMarker {
  override toDOM(): HTMLElement {
    const el = document.createElement("span");
    el.className = "cm-breakpoint-marker";
    el.setAttribute("aria-label", "ブレークポイント");
    el.textContent = "●";
    return el;
  }
}

const breakpointGutterMarker = new BreakpointGutterMarker();

/** 各ブレークポイントの行頭に丸印を置く(同一行の複数は 1 つにまとめる) */
function gutterMarkers(state: EditorState): RangeSet<GutterMarker> {
  const lineStarts: number[] = [];
  state.field(breakpointField).between(0, state.doc.length, (from) => {
    const lineStart = state.doc.lineAt(from).from;
    if (lineStarts[lineStarts.length - 1] !== lineStart) {
      lineStarts.push(lineStart);
    }
  });
  return RangeSet.of(
    lineStarts.map((pos) => breakpointGutterMarker.range(pos)),
    true,
  );
}

/** 位置 `pos`(null なら命令なし)に対する切替。受理なら true */
function applyToggle(view: EditorView, pos: number | null): boolean {
  // 読み取り専用(実行中・一時停止処理中)は切替を受け付けない(停止中の切替のみ。要件 6.11)
  if (view.state.readOnly) {
    return false;
  }
  if (pos === null) {
    view.state.facet(handlersFacet).onRejected("noInstruction");
    return false;
  }
  view.dispatch({ effects: toggleEffect.of(pos) });
  return true;
}

/** 行番号(1 始まり)の行内で最初の命令にブレークポイントを切り替える(ガタークリック) */
export function toggleBreakpointAtLine(view: EditorView, lineNo: number): boolean {
  return applyToggle(view, firstInstructionInLine(view.state.doc, lineNo));
}

/** カーソル以降で最初の命令にブレークポイントを切り替える(キー操作)。受理なら true */
export const toggleBreakpointAtCursor: Command = (view) =>
  applyToggle(view, firstInstructionFrom(view.state.doc, view.state.selection.main.head));

/** 全ブレークポイントを外す */
export function clearAllBreakpoints(view: EditorView): void {
  if (breakpointOffsets(view.state).length === 0) {
    return;
  }
  view.dispatch({ effects: clearEffect.of(null) });
}

/**
 * 集合をワークスペース側の値へ合わせる(外部からの置換)。命令でない位置は取り除く。
 * 読み取り専用でも適用する(実行開始時の未解決位置の除去などプログラムからの同期のため)。
 * 変化があったときだけ onChange が呼ばれる(呼び出し側は更新中に呼ばないこと)。
 */
export function setBreakpointOffsets(view: EditorView, offsets: readonly number[]): void {
  view.dispatch({ effects: setEffect.of([...offsets]) });
}

const breakpointTheme = EditorView.baseTheme({
  ".cm-breakpoint-char": {
    textDecoration: "underline double",
    outline: "1px solid currentColor",
    outlineOffset: "-1px",
  },
  ".cm-breakpoint-gutter": {
    minWidth: "1.2em",
    cursor: "pointer",
  },
  ".cm-breakpoint-gutter .cm-breakpoint-marker": {
    color: "#c62828",
    fontWeight: "bold",
    textAlign: "center",
  },
});

/**
 * ブレークポイントの拡張一式。`handlers` は変化通知と拒否通知を受け取る。
 * ガター(丸印)、命令文字の枠、F9 による切替、ガタークリックによる切替を含む。
 */
export function breakpointExtensions(handlers: BreakpointHandlers): Extension[] {
  return [
    handlersFacet.of(handlers),
    breakpointField,
    breakpointTheme,
    gutter({
      class: "cm-breakpoint-gutter",
      markers: (view) => gutterMarkers(view.state),
      domEventHandlers: {
        mousedown(view, line) {
          toggleBreakpointAtLine(view, view.state.doc.lineAt(line.from).number);
          return true;
        },
      },
    }),
    // 拒否されても F9 は消費する(他の既定動作へ流さない)
    keymap.of([
      {
        key: "F9",
        run: (view) => {
          toggleBreakpointAtCursor(view);
          return true;
        },
      },
    ]),
    EditorView.updateListener.of((update) => {
      if (update.startState.field(breakpointField) === update.state.field(breakpointField)) {
        return;
      }
      const before = breakpointOffsets(update.startState);
      const after = breakpointOffsets(update.state);
      if (before.length !== after.length || before.some((v, i) => v !== after[i])) {
        update.view.state.facet(handlersFacet).onChange(after);
      }
    }),
  ];
}
