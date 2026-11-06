import "./SourceEditor.css";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { Compartment, EditorState, type Extension, type StateEffect } from "@codemirror/state";
import { EditorView, keymap, lineNumbers } from "@codemirror/view";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { bracketMatchingExtensions } from "../editor/bracketMatching";
import {
  type BreakpointRejection,
  breakpointExtensions,
  breakpointOffsets,
  setBreakpointOffsets,
} from "../editor/breakpoints";
import type { EditorDisplay } from "../editor/editorDisplay";
import {
  type TextRange,
  executionDecorations,
  scrollToExecution,
  setExecution,
  setParseError,
} from "../editor/executionDecorations";
import { type WorkspaceStore, workspaceStore as defaultStore } from "../state/workspace";

/**
 * ソースエディタ(要件 2.1, 2.2, 5.10, 6.5, 7.1, 7.2, 7.9〜7.11)。
 * 各拡張(強調・括弧対応・ブレークポイント・履歴)を組み合わせ、Workspace のソースと双方向に同期する。
 *
 * 同期の方針:
 * - 利用者の編集は `updateListener` で Workspace へ書き戻す(未保存になる)。エディタは常に LF(改行の正規化は CodeMirror 既定)で、
 *   渡すソースは `doc.toString()` と一致する。
 * - 文書の置換(新規・開く)は Workspace の `documentRevision` で検出し、エディタの状態を作り直す(履歴とブレークポイントを空にする)。
 *   状態の作り直しは更新を発火しないため、Workspace へ書き戻さない。
 * - ブレークポイントの集合は Workspace と双方向に合わせる。Workspace 側の変更(RunController が未解決の位置を外した場合を含む)は
 *   エディタへ戻す。エディタから戻した値は Workspace と一致するため、往復は 1 回で収束する。
 * - Workspace への書き戻し中は購読からの再同期を止める(更新の中で更新を発行しないため)。
 * - 読み取り専用(実行中・一時停止処理中)は `editable` で切り替える。利用者の切替も読み取り専用中は受理されない。
 *
 * F9 について: ブレークポイントの F9 は本エディタのキーマップで処理し、結果を消費する(既定動作の preventDefault)。
 * 画面側のグローバルショートカット(8.2)は `KeyboardEvent.defaultPrevented` を見て、二重に切り替えないこと。
 */

export interface SourceEditorHandle {
  /** 全ブレークポイントを外す(編集不可のときは何もしない)。8.1 の全解除ボタンが呼ぶ */
  clearBreakpoints(): void;
}

export interface SourceEditorProps {
  /** 同期先の Workspace。既定は共有ストア */
  store?: WorkspaceStore;
  /** 編集可能か(controls.edit)。実行中・一時停止処理中は false */
  editable: boolean;
  /** デバッガ表示状態から導いた強調(editorDisplayOf の結果) */
  display: EditorDisplay;
  /** 停止時のスクロール要求を消費したときに呼ぶ(debuggerStore の scrollConsumed) */
  onScrollConsumed: () => void;
  /** 命令の無い位置にブレークポイントを置こうとしたときに呼ぶ(通知は呼び出し側) */
  onBreakpointRejected?: (reason: BreakpointRejection) => void;
}

/** 読み取り専用の切り替え(編集可否と入力の受け付けを同時に決める) */
function editableExtensions(editable: boolean): Extension[] {
  return [EditorState.readOnly.of(!editable), EditorView.editable.of(editable)];
}

/** 範囲が文書の内側にあるか(範囲外の scrollIntoView を避ける) */
function isWithin(range: TextRange, docLength: number): boolean {
  return range.from >= 0 && range.from < range.to && range.to <= docLength;
}

function sameOffsets(a: readonly number[], b: readonly number[]): boolean {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

export const SourceEditor = forwardRef<SourceEditorHandle, SourceEditorProps>(function SourceEditor(
  { store = defaultStore, editable, display, onScrollConsumed, onBreakpointRejected },
  ref,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const compartment = useRef(new Compartment()).current;
  // Workspace へ書き戻している間は購読からの再同期を止める。CodeMirror のリスナ・ブレークポイントの onChange の中で
  // store を更新すると購読が同期的に走るため、その再入(view.setState / dispatch の入れ子)を防ぐ。
  // sameOffsets は同じ集合への不要な dispatch を避けるための比較(ガードを外しても現状の挙動は収束するが、再入の防止として残す)。
  const applyingRef = useRef(false);
  // 最後に反映した文書の置換の回数
  const revisionRef = useRef(store.getSnapshot().documentRevision);
  // CodeMirror のリスナから参照する最新の値(リスナは一度だけ作るため)
  const latest = useRef({ editable, display, onScrollConsumed, onBreakpointRejected });
  latest.current = { editable, display, onScrollConsumed, onBreakpointRejected };

  const withApplying = (action: () => void): void => {
    applyingRef.current = true;
    try {
      action();
    } finally {
      applyingRef.current = false;
    }
  };

  // 状態の生成: 各拡張の組み合わせ。文書・ブレークポイント・履歴は状態ごとに新しくなる
  const createState = (doc: string): EditorState =>
    EditorState.create({
      doc,
      extensions: [
        lineNumbers(),
        EditorView.contentAttributes.of({ "aria-label": "ソース" }),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        ...bracketMatchingExtensions,
        ...executionDecorations,
        ...breakpointExtensions({
          onChange: (offsets) => {
            withApplying(() => store.setBreakpoints(offsets));
          },
          onRejected: (reason) => latest.current.onBreakpointRejected?.(reason),
        }),
        compartment.of(editableExtensions(latest.current.editable)),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const source = update.state.doc.toString();
            withApplying(() => store.setSource(source));
          }
        }),
      ],
    });

  // マウント: ワークスペースのソースで作り、購読を張る。store が変わったら作り直す
  useEffect(() => {
    const parent = containerRef.current;
    if (!parent) return;
    const view = new EditorView({ state: createState(store.getSnapshot().source), parent });
    viewRef.current = view;
    revisionRef.current = store.getSnapshot().documentRevision;

    const onStoreChange = () => {
      if (applyingRef.current) return;
      const snapshot = store.getSnapshot();
      if (snapshot.documentRevision !== revisionRef.current) {
        revisionRef.current = snapshot.documentRevision;
        view.setState(createState(snapshot.source));
      }
      if (!sameOffsets(breakpointOffsets(view.state), snapshot.breakpoints)) {
        setBreakpointOffsets(view, snapshot.breakpoints);
      }
    };
    const unsubscribe = store.subscribe(onStoreChange);

    return () => {
      unsubscribe();
      view.destroy();
      viewRef.current = null;
    };
    // createState は store と compartment に依存する(どちらもマウント中は変わらない)
  }, [store]);

  // 編集可否の切り替え(読み取り専用と編集可能の両方を一度に切り替える)
  useEffect(() => {
    viewRef.current?.dispatch({ effects: compartment.reconfigure(editableExtensions(editable)) });
  }, [compartment, editable]);

  // 現在命令・実行エラー・解析エラーの強調と、停止時のスクロール。表示が変わったときだけ反映する
  const executionKey = display.execution
    ? `${display.execution.kind}:${display.execution.range.from}:${display.execution.range.to}`
    : "";
  const parseKey = display.parseError ? `${display.parseError.from}:${display.parseError.to}` : "";
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = latest.current.display;
    const docLength = view.state.doc.length;
    const execution =
      current.execution && isWithin(current.execution.range, docLength) ? current.execution : null;
    const parseError =
      current.parseError && isWithin(current.parseError, docLength) ? current.parseError : null;

    const effects: StateEffect<unknown>[] = [
      setExecution.of({ range: execution?.range ?? null, kind: execution?.kind ?? "current" }),
      setParseError.of(parseError),
    ];
    if (current.scrollRequest && execution) {
      effects.push(scrollToExecution(execution.range));
    }
    view.dispatch({ effects });
    if (current.scrollRequest) {
      latest.current.onScrollConsumed();
    }
    // 強調の対象は範囲の文字列(キー)で比べる(同じ範囲の再通知で装飾を作り直さない)
  }, [executionKey, parseKey, display.scrollRequest]);

  useImperativeHandle(
    ref,
    () => ({
      clearBreakpoints() {
        const view = viewRef.current;
        if (!view || !latest.current.editable) return;
        setBreakpointOffsets(view, []);
        // エディタに無い集合が Workspace に残っていても空にする(通常は一致している)
        store.setBreakpoints([]);
      },
    }),
    [store],
  );

  return <div ref={containerRef} className="source-editor" />;
});
