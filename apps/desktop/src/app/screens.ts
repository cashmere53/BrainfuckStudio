// 画面レジストリ(要件 12.14 の拡張点)。後続の画面はここに 1 件追加する。
// 画面が 2 つ以上になったときだけ App がタブを出す。各画面は `useWorkspace()` で共有の Workspace を読む。

import type { ComponentType } from "react";
import { DebuggerScreen } from "./DebuggerScreen";

export interface ScreenDefinition {
  /** 画面の識別子(タブの対応づけに使う。一意であること) */
  id: string;
  /** タブに出す名前 */
  title: string;
  component: ComponentType;
}

export const screens: readonly ScreenDefinition[] = [{ id: "debugger", title: "デバッガ", component: DebuggerScreen }];
