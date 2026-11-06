// デバッガ表示状態(debuggerStore)からエディタの強調へ写す純粋な変換(要件 7.1〜7.2, 7.9〜7.11)。
// 強調の実体(StateEffect / StateField)は executionDecorations.ts が持ち、ここは値の対応だけを決める。

import type { DebuggerState } from "../state/debuggerStore";
import { type ExecutionKind, type TextRange, parseErrorRange, rangeFromDto } from "./executionDecorations";

/** エディタに出す実行系の強調(現在命令または実行エラーの命令) */
export interface EditorExecution {
  readonly range: TextRange;
  readonly kind: ExecutionKind;
}

/** エディタの表示へ変換した値。SourceEditor がこれを装飾の効果へ写す */
export interface EditorDisplay {
  /** 現在命令(待機・実行中・一時停止処理中)または実行エラー命令。無ければ null */
  readonly execution: EditorExecution | null;
  /** 解析エラーの括弧位置。無ければ null */
  readonly parseError: TextRange | null;
  /** 停止時のスクロール要求(エディタが消費したら降ろす) */
  readonly scrollRequest: boolean;
}

/**
 * デバッガの表示状態からエディタの表示を導く。
 * - 実行エラー: `state.currentRange`(エラー直前の pc)を runtimeError で示す(7.9)
 * - それ以外で現在命令がある: current で示す(7.1、実行中の更新も含む 7.11)
 * - 解析エラー: 括弧の 1 文字を示す(7.10)。セッションは無いので現在命令は出さない
 */
export function editorDisplayOf(state: DebuggerState): EditorDisplay {
  const currentRange = state.state?.currentRange ?? null;
  const error = state.error;

  let execution: EditorExecution | null = null;
  if (currentRange !== null) {
    if (error?.category === "runtime") {
      execution = { range: rangeFromDto(currentRange), kind: "runtimeError" };
    } else if (state.phase !== "noSession") {
      execution = { range: rangeFromDto(currentRange), kind: "current" };
    }
  }

  return {
    execution,
    parseError: error?.category === "parse" ? parseErrorRange(error.error.position) : null,
    scrollRequest: state.scrollRequest,
  };
}
