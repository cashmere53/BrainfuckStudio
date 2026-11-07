import { EditorView } from "@codemirror/view";

/**
 * ソースエディタの配色(テーマ拡張)。CodeMirror の既定の行番号ガターは明るい固定色のため、
 * :root のテーマトークン(--color-*)を参照して、ライト/ダークの切替に合わせる(要件 14.1)。
 * ブレークポイントのマーカーと現在命令の強調は、それぞれの拡張の設定をそのまま使う。
 */
export const editorThemeSpec = {
  ".cm-gutters": {
    backgroundColor: "var(--color-bg)",
    color: "var(--color-muted)",
    borderRight: "1px solid var(--color-border)",
  },
} as const;

export const editorTheme = EditorView.theme(editorThemeSpec);
