// 画面レジストリ(要件 12.14 の拡張点)。後続の画面はここに 1 件追加する。
// 画面が 2 つ以上になったときだけ App がタブを出す。各画面は `useWorkspace()` で共有の Workspace を読む。
// 比較の状態(結果・実行中)は、画面のマウントより長く生きるコントローラが持つ。タブ切替でアンマウントされても
// 実行は続き、応答は store に反映される(要件 11.4)。古い結果の判定にも同じ store を使う(要件 11.8)。
// デバッガ画面の挙動は変えない。

import { createElement, type ComponentType } from "react";
import { ComparisonScreen, type ComparisonScreenProps } from "../comparison/ComparisonScreen";
import { tauriComparisonBackend, type ComparisonBackend } from "../comparison/ipc";
import { createComparisonController, type ComparisonController } from "../comparison/state/comparisonStore";
import { workspaceStore } from "../state/workspace";
import { DebuggerScreen } from "./DebuggerScreen";

export interface ScreenDefinition {
  /** 画面の識別子(タブの対応づけに使う。一意であること) */
  id: string;
  /** タブに出す名前 */
  title: string;
  component: ComponentType;
}

export interface ScreenDependencies {
  /** 比較のバックエンド(画面の情報取得に使う。`comparisonController` と同じものを渡す) */
  comparisonBackend: ComparisonBackend;
  /** 比較の状態を持つコントローラ(画面のライフサイクルより長く保持される) */
  comparisonController: ComparisonController;
}

/** 画面の登録一覧を、依存を注入して作る。テストは偽バックエンドのコントローラで組み立てる */
export function createScreens(deps: ScreenDependencies): readonly ScreenDefinition[] {
  const ComparisonRoute = () =>
    createElement<ComparisonScreenProps>(ComparisonScreen, {
      backend: deps.comparisonBackend,
      controller: deps.comparisonController,
    });
  return [
    { id: "debugger", title: "デバッガ", component: DebuggerScreen },
    { id: "comparison", title: "比較", component: ComparisonRoute },
  ];
}

/** 本番の比較コントローラ。アプリの生存期間で 1 つだけ作る(画面のマウントごとには作り直さない) */
const productionComparisonController = createComparisonController({
  backend: tauriComparisonBackend,
  workspace: workspaceStore,
});

export const screens: readonly ScreenDefinition[] = createScreens({
  comparisonBackend: tauriComparisonBackend,
  comparisonController: productionComparisonController,
});
