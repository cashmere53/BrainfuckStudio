// スモーク 14(見た目): ライトとダークで主要な文字のコントラスト比が 4.5:1 以上であること(9.7)、
// 強制配色でボタンの枠・フォーカス枠・波線の下線が描画されていること(9.8)、
// 解析エラー・ブレークポイント・括弧の一致がある状態で、波線の下線・枠・マーカー・括弧の一致の表示が描画されていること(9.9)。
// 配色と強制配色はブラウザのエミュレーションで代替する(OS の設定そのものの確認ではない)。
import type { Locator } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";
import type { DebuggerPage, StatusTerm } from "../support/pages/DebuggerPage.ts";
import { measureContrast, MIN_TEXT_CONTRAST } from "../support/visual/contrast.ts";
import { expectFocusRingDrawn } from "../support/visual/focusRing.ts";

/** 要素の描画の寸法と、装飾の算出スタイル(ブラウザが実際に計算した値) */
type Decoration = {
  readonly width: number;
  readonly height: number;
  readonly textDecorationStyle: string;
  readonly textDecorationLine: string;
  readonly outlineStyle: string;
  readonly outlineWidth: number;
};

/** 要素の枠(ボタンの境界線)の算出スタイル */
type Border = { readonly style: string; readonly width: number; readonly color: string };

/** 要素の寸法と装飾の算出スタイルを読む(寸法は境界ボックス、装飾は算出スタイル) */
async function readDecoration(locator: Locator): Promise<Decoration> {
  const box = await locator.boundingBox();
  const style = await locator.evaluate((el) => {
    const computed = getComputedStyle(el);
    return {
      textDecorationStyle: computed.textDecorationStyle,
      textDecorationLine: computed.textDecorationLine,
      outlineStyle: computed.outlineStyle,
      outlineWidth: parseFloat(computed.outlineWidth),
    };
  });
  return { width: box?.width ?? 0, height: box?.height ?? 0, ...style };
}

/** 要素の上の境界線の算出スタイル */
async function readBorder(locator: Locator): Promise<Border> {
  return locator.evaluate((el) => {
    const computed = getComputedStyle(el);
    return {
      style: computed.borderTopStyle,
      width: parseFloat(computed.borderTopWidth),
      color: computed.borderTopColor,
    };
  });
}

/** 文字のコントラスト比を確かめる。比の値を失敗の文言に含める */
async function expectReadableText(locator: Locator, label: string): Promise<void> {
  const ratio = await measureContrast(locator);
  expect(ratio, `${label} のコントラスト比(4.5:1 以上が必要)`).toBeGreaterThanOrEqual(MIN_TEXT_CONTRAST);
}

/** 状態領域の項目名(補助ラベルとして --color-muted で描かれる。全件を測る) */
const STATUS_TERMS: readonly StatusTerm[] = ["pc", "dp", "現在のセル", "実行数", "入力の読み取り済み", "入力の総数"];

/**
 * 解析エラー・ブレークポイント・括弧の一致がある状態を作る。先頭の「]」は対応しない括弧で、Run の解析で解析エラーになる。
 * 末尾の「]」の前にカーソルがあり、「[」と組になって括弧の一致が表示される。
 */
async function showDecorations(debuggerPage: DebuggerPage): Promise<void> {
  await debuggerPage.setSource("]+[-]");
  await debuggerPage.button("Run").click();
  await expect(debuggerPage.parseErrors).toHaveCount(1);
  await debuggerPage.clickBreakpointGutter(1);
  await expect.poll(() => debuggerPage.hasBreakpointAt(1)).toBe(true);
}

for (const scheme of ["light", "dark"] as const) {
  test.describe(`スモーク 14: 配色(${scheme})`, () => {
    test.use({ colorScheme: scheme });

    test(`9.7 ${scheme} の配色で、本文・有効なボタン・状態ラベル・出力・エディタ・通知・エラーの文字が 4.5:1 以上`, async ({
      page,
      debuggerPage,
    }) => {
      // 配色が指定どおりに効いていることを先に確かめる(コントラスト比の検証は配色に依存する)
      const dark = await page.evaluate(() => window.matchMedia("(prefers-color-scheme: dark)").matches);
      expect(dark).toBe(scheme === "dark");

      // 出力を作るため、初期のソース(Hello World)を実行して正常終了させる
      await debuggerPage.button("Run").click();
      await expect(debuggerPage.statusLabel).toHaveText("正常終了");
      await expect(debuggerPage.outputText).toBeVisible();

      await expectReadableText(debuggerPage.documentStatus, "本文(文書の表示)");
      await expectReadableText(debuggerPage.statusLabel, "状態ラベル");
      await expectReadableText(debuggerPage.outputText, "出力");
      await expectReadableText(debuggerPage.editor, "エディタ");
      // 補助の文字(行番号のガター・総バイト数・状態パネルの項目名)は --color-muted で描かれる
      await expectReadableText(debuggerPage.gutterLine(1), "行番号のガター");
      await expectReadableText(debuggerPage.outputTotal, "総バイト数");
      for (const term of STATUS_TERMS) {
        await expectReadableText(debuggerPage.statusTerm(term), `状態の項目名「${term}」`);
      }

      // 有効なボタン: 終了後は Run が無効のため、Reset で未開始に戻して Run を有効にしてから測る
      await debuggerPage.button("Reset").click();
      await expect(debuggerPage.statusLabel).toHaveText("未開始");
      await expect(debuggerPage.button("Run")).toBeEnabled();
      await expectReadableText(debuggerPage.button("Run"), "有効なボタン(Run)");

      // 通知: 命令の無い行(2 行目の空行)にブレークポイントを置こうとして、通知を出す
      await debuggerPage.setSource("+\n");
      await debuggerPage.clickBreakpointGutter(2);
      await expect(debuggerPage.notices).toBeVisible();
      await expectReadableText(debuggerPage.notices, "通知");

      // エラー: 先頭で「<」を実行して、ポインタが範囲外になる実行エラーを出す
      await debuggerPage.setSource("<");
      await debuggerPage.button("Run").click();
      await expect(debuggerPage.errorBanner).toBeVisible();
      await expectReadableText(debuggerPage.errorBanner, "エラー");
      // エラーの見出しは枠の文字色(--color-fg)ではなく --color-error で描かれるため、見出しも単独で測る
      await expect(debuggerPage.errorHeading).toHaveText("実行エラー");
      await expectReadableText(debuggerPage.errorHeading, "エラーの見出し");
    });
  });
}

test.describe("スモーク 14: 強制配色", () => {
  test("9.8 強制配色でボタンの枠が描画されている(境界線の線種と太さ)", async ({ page, debuggerPage }) => {
    await page.emulateMedia({ forcedColors: "active" });
    await expect(debuggerPage.button("Run")).toBeVisible();

    const border = await readBorder(debuggerPage.button("Run"));
    expect(border.style, "ボタンの枠の線種").not.toBe("none");
    expect(border.width, "ボタンの枠の太さ").toBeGreaterThan(0);
  });

  for (const target of ["ボタン(Run)", "エディタ"] as const) {
    test(`9.8 強制配色で ${target} のフォーカス枠が 4 辺とも描画される`, async ({ page, debuggerPage }) => {
      await page.emulateMedia({ forcedColors: "active" });
      const focusTarget = target === "エディタ" ? debuggerPage.editor : debuggerPage.button("Run");
      const ring = target === "エディタ" ? debuggerPage.editorFrame : debuggerPage.button("Run");

      // focusRing.ts は枠の outline を focus の前に読むため、読む時点で枠が出ているようにフォーカスを先に移す
      await focusTarget.focus();
      await expectFocusRingDrawn(page, { focus: () => focusTarget.focus(), ring });
      await expect(focusTarget, `${target} にフォーカスがある`).toBeFocused();
    });
  }

  test("9.8 強制配色で解析エラーの下線が波線として描画される", async ({ page, debuggerPage }) => {
    await page.emulateMedia({ forcedColors: "active" });
    await debuggerPage.setSource("]");
    await debuggerPage.button("Run").click();

    await expect(debuggerPage.parseErrors).toHaveCount(1);
    const decoration = await readDecoration(debuggerPage.parseErrors);
    expect(decoration.textDecorationLine, "解析エラーの下線の種類").toContain("underline");
    expect(decoration.textDecorationStyle, "解析エラーの下線の線種").toBe("wavy");
  });
});

test.describe("スモーク 14: 装飾の描画", () => {
  test("9.9 解析エラーの波線の下線と枠、ブレークポイントのマーカー、括弧の一致の表示が描画されている", async ({
    debuggerPage,
  }) => {
    await showDecorations(debuggerPage);

    // 解析エラー: 波線の下線、枠(線種がある)、大きさが 0 でない
    await expect(debuggerPage.parseErrors).toHaveCount(1);
    const parse = await readDecoration(debuggerPage.parseErrors);
    expect(parse.width, "解析エラーの幅").toBeGreaterThan(0);
    expect(parse.height, "解析エラーの高さ").toBeGreaterThan(0);
    expect(parse.textDecorationLine, "解析エラーの下線の種類").toContain("underline");
    expect(parse.textDecorationStyle, "解析エラーの下線の線種").toBe("wavy");
    expect(parse.outlineStyle, "解析エラーの枠の線種").not.toBe("none");
    expect(parse.outlineWidth, "解析エラーの枠の太さ").toBeGreaterThan(0);

    // ブレークポイント: マーカーが描画され、大きさが 0 でない
    await expect(debuggerPage.breakpointMarkers.first()).toBeVisible();
    const marker = await readDecoration(debuggerPage.breakpointMarkers.first());
    expect(marker.width, "ブレークポイントのマーカーの幅").toBeGreaterThan(0);
    expect(marker.height, "ブレークポイントのマーカーの高さ").toBeGreaterThan(0);

    // 括弧の一致: 対応する両方の括弧が表示され、大きさが 0 でない
    await expect(debuggerPage.bracketMatched.first()).toBeVisible();
    await expect(debuggerPage.bracketMatched).toHaveCount(2);
    for (const bracket of await debuggerPage.bracketMatched.all()) {
      const decoration = await readDecoration(bracket);
      expect(decoration.width, "括弧の一致の幅").toBeGreaterThan(0);
      expect(decoration.height, "括弧の一致の高さ").toBeGreaterThan(0);
      // 色以外の手がかり: 枠(線種がある)と下線(括弧の一致の下線は underline)
      expect(decoration.outlineStyle, "括弧の一致の枠の線種").not.toBe("none");
      expect(decoration.textDecorationLine, "括弧の一致の下線の種類").toContain("underline");
    }

    // ブレークポイントの命令文字: 下線が二重線で、枠がある(要件 9.9)。マーカーの大きさだけでは確かめない
    await expect(debuggerPage.breakpointChars).toHaveCount(1);
    const breakpointChar = await readDecoration(debuggerPage.breakpointChars);
    expect(breakpointChar.textDecorationLine, "ブレークポイントの下線の種類").toContain("underline");
    expect(breakpointChar.textDecorationStyle, "ブレークポイントの下線の線種").toBe("double");
    expect(breakpointChar.outlineStyle, "ブレークポイントの枠の線種").not.toBe("none");
    expect(breakpointChar.outlineWidth, "ブレークポイントの枠の太さ").toBeGreaterThan(0);
  });
});
