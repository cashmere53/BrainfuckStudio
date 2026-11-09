// 比較画面(要件 10.1・11.1〜11.11・14.3): 方式と繰り返し回数の選択、実行ボタン(無効理由つき)、実行中・古い結果・解析エラー・結果の表示。
// 共有の Workspace を読み(useWorkspace)、通信は ComparisonBackend、状態は ComparisonController に任せる。
// デバッガの状態・実行コントローラには触れない(要件 11.11)。
//
// 方針:
// - 実行の可否と無効理由は domain の要求導出(deriveRequest)と整形(formatInvalidReason)で決め、ここでは組み立てない。
// - 通信の注入: `backend` を省略すると本物の IPC。テストでは台本式の偽実装を渡す(app/screens.ts が本番の実装を登録する)。
// - 古い結果は、実行時スナップショットと現在の共有値の比較で判定する(isResultStale)。

import "./ComparisonScreen.css";
import { useEffect, useId, useMemo, useState, useSyncExternalStore } from "react";
import type { ApiError } from "../generated/ApiError";
import type { ComparisonInfoDto } from "../generated/ComparisonInfoDto";
import { apiErrorMessageJa, normalizeApiError } from "../ipc/errors";
import { useWorkspace, workspaceStore, type WorkspaceStore } from "../state/workspace";
import { MethodOptions } from "./components/MethodOptions";
import { OverallBanner } from "./components/OverallBanner";
import { ParseErrorNotice } from "./components/ParseErrorNotice";
import { ResultTable } from "./components/ResultTable";
import { UnitLegend } from "./components/UnitLegend";
import { formatInvalidReason } from "./domain/comparisonFormat";
import { deriveRequest, type Selection } from "./domain/comparisonRequest";
import { DISCLAIMER_TEXT } from "./domain/definitions";
import { tauriComparisonBackend, type ComparisonBackend } from "./ipc";
import { createComparisonController, isResultStale, type ComparisonController } from "./state/comparisonStore";

export interface ComparisonScreenProps {
  /** 比較のバックエンド。既定は本物の IPC(テストでは偽バックエンドを注入する)。`controller` を渡すときは、その controller と同じバックエンドを渡す */
  backend?: ComparisonBackend;
  /** 共有の Workspace ストア。既定はアプリ全体のストア */
  workspace?: WorkspaceStore;
  /**
   * 比較の状態を持つコントローラ。画面のライフサイクルより長く保持するために、呼び出し側(レジストリ)が作って渡す。
   * 省略時は画面が自分用に作る(テストの既定)。アンマウントは購読の解除のみで、実行中の比較は止めない。
   */
  controller?: ComparisonController;
}

/** 入力欄の文字列を繰り返し回数にする。10 進の数字だけを受け付け、それ以外は範囲外として扱う(要求の導出が拒否する) */
function parseRepetitions(text: string): number {
  return /^[0-9]+$/.test(text) ? Number(text) : Number.NaN;
}

export function ComparisonScreen(props: ComparisonScreenProps = {}) {
  const backend = props.backend ?? tauriComparisonBackend;
  const store = props.workspace ?? workspaceStore;
  const headingId = useId();
  const reasonId = useId();

  const values = useWorkspace(store);
  const ownController = useMemo(() => createComparisonController({ backend, workspace: store }), [backend, store]);
  const controller = props.controller ?? ownController;
  const state = useSyncExternalStore(controller.subscribe, controller.getState, controller.getState);

  const [info, setInfo] = useState<ComparisonInfoDto | null>(null);
  const [infoError, setInfoError] = useState<ApiError | null>(null);
  const [optimized, setOptimized] = useState(true);
  const [wasm, setWasm] = useState(true);
  // null は「既定の繰り返し回数を使う」。情報を取得して既定値が分かってから表示する
  const [repetitionsText, setRepetitionsText] = useState<string | null>(null);

  // 画面表示時に比較の情報(上限値・繰り返しの既定と最大)を取得する
  useEffect(() => {
    let active = true;
    backend.getInfo().then(
      (loaded) => {
        if (active) {
          setInfo(loaded);
        }
      },
      (rejection: unknown) => {
        if (active) {
          setInfoError(normalizeApiError(rejection));
        }
      },
    );
    return () => {
      active = false;
    };
  }, [backend]);

  const running = state.phase === "running";
  const effectiveRepetitionsText = repetitionsText ?? String(info?.defaultRepetitions ?? 1);
  const selection: Selection = { optimized, wasm, repetitions: parseRepetitions(effectiveRepetitionsText) };

  // 実行できない理由: 情報が無い場合はその旨、ある場合は要求の導出が返す理由
  let reasonText: string | null = null;
  if (info === null) {
    reasonText = infoError === null ? "比較の情報を読み込んでいます" : "比較の情報を取得できなかったため実行できません";
  } else {
    const derived = deriveRequest(values, selection, info);
    if (!derived.ok) {
      reasonText = formatInvalidReason(derived.reason);
    }
  }
  const canRun = info !== null && reasonText === null && !running;

  const stale = state.result !== null && isResultStale(state.usedSnapshot, values);

  const handleRun = () => {
    if (info !== null) {
      void controller.run(selection, info);
    }
  };

  return (
    <section className="comparison-screen" aria-labelledby={headingId}>
      <h1 id={headingId} className="comparison-heading">
        比較
      </h1>

      {infoError !== null ? (
        <p className="comparison-alert" role="alert">
          {apiErrorMessageJa(infoError)}
        </p>
      ) : null}

      <div className="comparison-controls">
        {info !== null ? (
          <>
            <MethodOptions
              optimized={optimized}
              wasm={wasm}
              repetitionsText={effectiveRepetitionsText}
              maxRepetitions={info.maxRepetitions}
              disabled={running}
              onOptimizedChange={setOptimized}
              onWasmChange={setWasm}
              onRepetitionsTextChange={setRepetitionsText}
            />
            <p className="comparison-limit-note">{`比較できる命令数の上限は ${info.maxSteps} です`}</p>
          </>
        ) : null}

        <div className="comparison-run">
          <button
            type="button"
            className="comparison-run-button"
            disabled={!canRun}
            aria-describedby={reasonText !== null ? reasonId : undefined}
            onClick={handleRun}
          >
            比較を実行
          </button>
          {reasonText !== null ? (
            <p id={reasonId} className="comparison-run-reason">
              {reasonText}
            </p>
          ) : null}
        </div>
      </div>

      {stale ? (
        <p className="comparison-stale" role="status">
          条件が変更されています(再実行してください)
        </p>
      ) : null}

      <section className="comparison-results" aria-label="方式ごとの結果" aria-busy={running}>
        {running ? <p role="status">実行中です…</p> : null}
        {state.phase === "failed" && state.error !== null ? (
          <p className="comparison-alert" role="alert">
            {apiErrorMessageJa(state.error)}
          </p>
        ) : null}
        {state.result?.kind === "parseError" ? <ParseErrorNotice error={state.result.error} /> : null}
        {state.result?.kind === "completed" ? (
          <>
            <OverallBanner overall={state.result.overall} />
            <ResultTable methods={state.result.methods} />
          </>
        ) : null}
      </section>

      <UnitLegend />
      <p className="comparison-disclaimer">{DISCLAIMER_TEXT}</p>
    </section>
  );
}
