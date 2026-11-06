// デバッガ画面(要件 1.1・1.3・1.4 / 14.5)。5 領域(操作・ソース・入力・テープ・出力・状態/エラー)を組み立て、
// 実行制御(RunController)・Workspace ストア・debuggerStore の reducer を配線する。
//
// 方針:
// - 表示状態は外部ストア(`createDebuggerHost`)に持ち、reducer を同期的に適用する。RunController は同期的に
//   状態を読むため、React の useReducer(非同期の dispatch)をそのまま渡すと読み取りが遅れる。
//   React へは useSyncExternalStore で購読する。
// - RunController は効果(useEffect)で作り、アンマウントで必ず dispose する(StrictMode の二重マウントでも
//   古いコントローラの購読と予約を残さない)。
// - 派生値(操作の有効/無効・状態ラベル・エディタの強調)は domain / editor の純粋関数に任せ、ここでは組み立てない。
// - ファイル操作(新規・開く・保存)と各ショートカットは 8.2 で接続する。8.1 では押せる見た目だけを置く。

import "./DebuggerScreen.css";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { ErrorBanner } from "../components/ErrorBanner";
import { InputPanel } from "../components/InputPanel";
import { NoticeList } from "../components/NoticeList";
import { OutputView } from "../components/OutputView";
import { SourceEditor, type SourceEditorHandle } from "../components/SourceEditor";
import { StatusPanel } from "../components/StatusPanel";
import { StepLimitField } from "../components/StepLimitField";
import { TapeView } from "../components/TapeView";
import { Toolbar } from "../components/Toolbar";
import { availability, type UiState } from "../domain/controls";
import { parseInput } from "../domain/inputBytes";
import type { ClientNotice } from "../domain/notice";
import { editorDisplayOf } from "../editor/editorDisplay";
import { tauriBackend, type Backend } from "../ipc/backend";
import { debuggerReducer, initialDebuggerState, type DebuggerAction, type DebuggerState } from "../state/debuggerStore";
import { createRunController, type RunController, type RunHost } from "../state/runController";
import { useWorkspace, workspaceStore, type WorkspaceStore } from "../state/workspace";

export interface DebuggerScreenProps {
  /** 実行のバックエンド。既定は本物の IPC(テストでは偽バックエンドを注入する) */
  backend?: Backend;
  /** 共有の Workspace ストア。既定はアプリ全体のストア */
  workspace?: WorkspaceStore;
}

/** RunController に渡す表示状態の外部ストア(同期的な読み書きと React への購読口) */
type DebuggerHost = RunHost & {
  subscribe(listener: () => void): () => void;
};

// 外部ストアにする理由: RunController は dispatch の直後に getState で最新の表示状態を読む(Run ループの
// 継続判定・Pause の要否・停止位置の反映)。React の useReducer は dispatch を次の描画まで反映しないため、
// 同期的に状態を更新できる外部ストアを使い、React へは useSyncExternalStore で購読を渡す。
function createDebuggerHost(): DebuggerHost {
  let state: DebuggerState = initialDebuggerState();
  const listeners = new Set<() => void>();
  return {
    getState: () => state,
    dispatch(action: DebuggerAction) {
      const next = debuggerReducer(state, action);
      if (next === state) {
        return;
      }
      state = next;
      for (const listener of [...listeners]) {
        listener();
      }
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/** ブレークポイントを命令の無い位置に置けなかったことの通知(UI が検出する。文面は noticeMessageJa が決める) */
const BREAKPOINT_REJECTED: ClientNotice = { kind: "client", code: "breakpointRejected" };

export function DebuggerScreen(props: DebuggerScreenProps = {}) {
  const backend = props.backend ?? tauriBackend;
  const workspace = props.workspace ?? workspaceStore;
  const snapshot = useWorkspace(workspace);

  const [host] = useState(createDebuggerHost);
  const display = useSyncExternalStore(host.subscribe, host.getState, host.getState);

  const editorRef = useRef<SourceEditorHandle>(null);
  const controllerRef = useRef<RunController | null>(null);

  // 実行制御の生成と破棄。StrictMode の二重マウントでは、最初のコントローラを破棄してから作り直す
  useEffect(() => {
    const controller = createRunController({ backend, workspace, host });
    controllerRef.current = controller;
    return () => {
      controller.dispose();
      if (controllerRef.current === controller) {
        controllerRef.current = null;
      }
    };
  }, [backend, workspace, host]);

  const inputValid = parseInput(snapshot.input).ok;
  const ui: UiState & { stoppedByPause: boolean } = {
    phase: display.phase,
    inputValid,
    stoppedByPause: display.localPaused,
  };
  const controls = availability(ui);

  return (
    <div className="debugger-screen">
      <div className="debugger-area debugger-area-toolbar">
        {/* ファイル操作(新規・開く・保存)は 8.2 で接続する */}
        <Toolbar
          controls={controls}
          onRun={() => void controllerRef.current?.run()}
          onPause={() => void controllerRef.current?.pause()}
          onStep={() => void controllerRef.current?.step()}
          onReset={() => void controllerRef.current?.reset()}
          onNew={noop}
          onOpen={noop}
          onSave={noop}
          onSaveAs={noop}
          onClearBreakpoints={() => editorRef.current?.clearBreakpoints()}
        />
      </div>

      <div className="debugger-area debugger-area-editor">
        <SourceEditor
          ref={editorRef}
          store={workspace}
          editable={controls.edit}
          display={editorDisplayOf(display)}
          onScrollConsumed={() => host.dispatch({ type: "scrollConsumed" })}
          onBreakpointRejected={() => host.dispatch({ type: "operationFailed", error: BREAKPOINT_REJECTED })}
        />
      </div>

      <div className="debugger-area debugger-area-input">
        <InputPanel
          input={snapshot.input}
          onChange={workspace.setInput}
          onSwitchRejected={(reason) => host.dispatch({ type: "operationFailed", error: { kind: "client", code: "inputSwitchRejected", reason } })}
          disabled={!controls.edit}
        />
        <StepLimitField
          text={snapshot.limit.text}
          unlimited={snapshot.limit.unlimited}
          onTextChange={workspace.setLimitText}
          onUnlimitedChange={workspace.setUnlimited}
          disabled={!controls.edit}
        />
      </div>

      <div className="debugger-area debugger-area-tape">
        <TapeView
          tapeWindow={display.state?.tapeWindow ?? null}
          radius={snapshot.tapeRadius}
          onRadiusChange={workspace.setTapeRadius}
        />
      </div>

      <div className="debugger-area debugger-area-output">
        <OutputView output={display.output} mode={snapshot.outputMode} onModeChange={workspace.setOutputMode} />
      </div>

      <div className="debugger-area debugger-area-status">
        <StatusPanel ui={ui} state={display.state} />
        <ErrorBanner error={display.error} />
        <NoticeList notices={display.notices} />
      </div>
    </div>
  );
}

function noop(): void {
  // 8.2 で接続するまでの既定(押せるが何もしない)
}
