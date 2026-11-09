// 画面レジストリ(要件 12.14 の拡張点)。後続の画面はここに 1 件追加する。
// 画面が 2 つ以上になったときだけ App がタブを出す。各画面は `useWorkspace()` で共有の Workspace を読む。
// 比較画面は登録時に本番の通信実装(比較のコマンド)を注入する。デバッガ画面の挙動は変えない。

import { createElement, type ComponentType } from "react";
import { ComparisonScreen, type ComparisonScreenProps } from "../comparison/ComparisonScreen";
import { tauriComparisonBackend } from "../comparison/ipc";
import { DebuggerScreen } from "./DebuggerScreen";

export interface ScreenDefinition {
  /** 画面の識別子(タブの対応づけに使う。一意であること) */
  id: string;
  /** タブに出す名前 */
  title: string;
  component: ComponentType;
}

/** 比較画面の登録用: 本番の通信実装を注入する */
const ComparisonRoute = () => createElement<ComparisonScreenProps>(ComparisonScreen, { backend: tauriComparisonBackend });

export const screens: readonly ScreenDefinition[] = [
  { id: "debugger", title: "デバッガ", component: DebuggerScreen },
  { id: "comparison", title: "比較", component: ComparisonRoute },
];
