// フォーカス枠の描画の検証。フォーカス時と、ブラー時(枠が無い状態)の枠の帯の画像を比べ、
// 4 辺すべてで画素が変わっていれば「枠が実際に描かれ、隠れていない」とみなす。
// 算出スタイルの outline-style だけでは、重なりや切り抜きで描画されない不具合を検出できない。
import type { Locator, Page } from "@playwright/test";

export type Band = { x: number; y: number; width: number; height: number };
export type Side = "top" | "right" | "bottom" | "left";
export type Bands = Record<Side, Band>;
export type Box = { x: number; y: number; width: number; height: number };

/**
 * 要素の境界ボックスと outline の幅・オフセット(px)から、枠の 4 辺の帯の矩形を求める。
 * オフセットは要素の辺から外側へ離す量(正の値で外へ)。帯は角の部分も含む。
 */
export function outlineBands(box: Box, outlineWidth: number, outlineOffset: number): Bands {
  const left = box.x - outlineOffset;
  const top = box.y - outlineOffset;
  const right = box.x + box.width + outlineOffset;
  const bottom = box.y + box.height + outlineOffset;
  const outerLeft = left - outlineWidth;
  const outerTop = top - outlineWidth;
  const outerWidth = right - left + 2 * outlineWidth;
  return {
    top: { x: outerLeft, y: outerTop, width: outerWidth, height: outlineWidth },
    bottom: { x: outerLeft, y: bottom, width: outerWidth, height: outlineWidth },
    left: { x: outerLeft, y: top, width: outlineWidth, height: bottom - top },
    right: { x: right, y: top, width: outlineWidth, height: bottom - top },
  };
}

const SIDES: readonly Side[] = ["top", "right", "bottom", "left"];

/** 画像のバイト列が異なる辺の一覧(長さの違いも異なるとみなす) */
export function differingSides(
  before: Record<Side, Uint8Array>,
  after: Record<Side, Uint8Array>,
): Side[] {
  return SIDES.filter((side) => {
    const a = before[side];
    const b = after[side];
    if (a.length !== b.length) {
      return true;
    }
    return a.some((value, index) => value !== b[index]);
  });
}

/**
 * フォーカス時とブラー時の帯の画像を比べ、4 辺すべてが異なることを確かめる。
 * 異なる辺が 1 つでも欠ければ、その辺の名前を含む例外を投げる。
 */
export function assertFocusRingSides(before: Record<Side, Uint8Array>, after: Record<Side, Uint8Array>): void {
  const changed = differingSides(before, after);
  if (changed.length !== SIDES.length) {
    const missing = SIDES.filter((side) => !changed.includes(side));
    throw new Error(`フォーカス枠が描画されていない辺があります: ${missing.join(", ")}`);
  }
}

/**
 * フォーカスした要素の outline の帯(4 辺)を求め、フォーカス時とブラー時の帯の画像が全辺で異なることを検証する。
 * 製品は枠を :focus-visible(キーボード由来のフォーカス)でだけ描くため、枠の算出は必ず focus の後に読む。
 * focus は、要素へフォーカスを移す操作(`focus()` の呼び出し、または `page.keyboard.press("Tab")` など)。
 * 2 回目の focus は行わない(キーボードの移動では、もう一度押すと次の要素へ進んでしまうため)。
 * 枠の描画を確かめられないときは例外にする。
 */
export async function expectFocusRingDrawn(
  page: Page,
  options: { focus: () => Promise<void>; ring: Locator },
): Promise<void> {
  const { focus, ring } = options;
  await focus();

  const style = await ring.evaluate((el) => {
    const s = getComputedStyle(el);
    return { outlineStyle: s.outlineStyle, outlineWidth: parseFloat(s.outlineWidth), outlineOffset: parseFloat(s.outlineOffset) };
  });
  if (style.outlineStyle === "none" || style.outlineWidth <= 0) {
    throw new Error(`フォーカス枠が指定されていません(outline-style: ${style.outlineStyle}, width: ${style.outlineWidth})`);
  }
  const box = await ring.boundingBox();
  if (box === null) {
    throw new Error("フォーカス枠の要素の位置を取得できません");
  }
  const bands = outlineBands(box, style.outlineWidth, style.outlineOffset);

  const shoot = async (): Promise<Record<Side, Uint8Array>> => {
    const entries = await Promise.all(
      SIDES.map(async (side) => {
        const buffer = await page.screenshot({ clip: bands[side], animations: "disabled", caret: "hide" });
        return [side, new Uint8Array(buffer)] as const;
      }),
    );
    return Object.fromEntries(entries) as Record<Side, Uint8Array>;
  };

  // フォーカス時: 枠が描かれた状態の帯の画像
  const focused = await shoot();

  // ブラー時: 枠が無い状態の帯の画像。ブラーの前にフォーカスの要素を控え、検査の後に同じ要素へ戻す
  // (呼び出し側の続きの操作が、フォーカスした要素を前提にできるように。戻すのは focus 操作を繰り返さない)
  const target = await page.evaluateHandle(() => document.activeElement);
  await page.evaluate(() => {
    const active = document.activeElement;
    if (active instanceof HTMLElement) {
      active.blur();
    }
  });
  const blurred = await shoot();
  await target.evaluate((element) => {
    if (element instanceof HTMLElement) {
      element.focus();
    }
  });
  await target.dispose();

  assertFocusRingSides(blurred, focused);
}
