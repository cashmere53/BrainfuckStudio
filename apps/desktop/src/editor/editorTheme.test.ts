import { describe, expect, it } from "vitest";
import { editorThemeSpec } from "./editorTheme";

// 任意のセレクタで引けるように広い型で見る(テストの比較専用)
const spec: Record<string, Record<string, string> | undefined> = editorThemeSpec;

// ソースエディタの配色はテーマトークン(:root の --color-*)を参照し、ライト/ダークの切替に追従させる(要件 14.1)
describe("editorThemeSpec", () => {
  it("行番号のガターの背景・文字色・境界をテーマトークンで指定する(白い固定色にしない)", () => {
    const gutters = spec[".cm-gutters"];
    expect(gutters).toBeDefined();
    expect(gutters?.backgroundColor).toBe("var(--color-bg)");
    expect(gutters?.color).toBe("var(--color-muted)");
    expect(gutters?.borderRight).toBe("1px solid var(--color-border)");
  });

  it("ガターの既定の固定色(白系)を含まない", () => {
    const serialized = JSON.stringify(editorThemeSpec);
    expect(serialized).not.toMatch(/#f5f5f5|#6c6c6c|#ddd/i);
  });

  it("ブレークポイントのマーカーのスタイルを上書きしない(別の拡張の設定に任せる)", () => {
    expect(spec[".cm-gutters .cm-breakpoint-marker"]).toBeUndefined();
  });
});
