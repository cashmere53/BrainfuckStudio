// 見た目の検証の補助: 色の合成・相対輝度・コントラスト比(WCAG 2.x)と、ページからの色の取得。
// 純粋関数(composite・relativeLuminance・contrastRatio・recoverRgba・effectiveBackground)は
// Node の単体テストで確かめる。色の取得はページ内のキャンバスで sRGB に正規化するため、
// CSS の色の書式(16 進・oklch など)に依存しない。
import type { Locator } from "@playwright/test";

/** 0..255 の sRGB と 0..1 のアルファ */
export type Rgba = { r: number; g: number; b: number; a: number };

/** 本文のコントラストの下限(WCAG 2.x 達成基準 1.4.3) */
export const MIN_TEXT_CONTRAST = 4.5;

/** 上の色 `top` を下の色 `bottom` の上に合成する(Porter-Duff の source-over) */
export function composite(top: Rgba, bottom: Rgba): Rgba {
  const a = top.a + bottom.a * (1 - top.a);
  if (a === 0) {
    return { r: 0, g: 0, b: 0, a: 0 };
  }
  const mix = (t: number, b: number) => (t * top.a + b * bottom.a * (1 - top.a)) / a;
  return { r: mix(top.r, bottom.r), g: mix(top.g, bottom.g), b: mix(top.b, bottom.b), a };
}

/** sRGB の 1 チャンネルを線形化する */
function linearize(channel: number): number {
  const c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** WCAG 2.x の相対輝度(0 が黒、1 が白)。不透明な色に対して使う */
export function relativeLuminance(color: Rgba): number {
  return 0.2126 * linearize(color.r) + 0.7152 * linearize(color.g) + 0.0722 * linearize(color.b);
}

/** WCAG 2.x のコントラスト比(1 から 21 まで)。引数の順序によらない */
export function contrastRatio(foreground: Rgba, background: Rgba): number {
  const l1 = relativeLuminance(foreground);
  const l2 = relativeLuminance(background);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

/**
 * 同じ色を黒と白の 2 つの不透明な背景の上に描いた画素から、元の色とアルファを戻す。
 * 合成式 out = a·c + (1 − a)·bg を、bg = 0 と bg = 255 の 2 つで解く。
 */
export function recoverRgba(onBlack: readonly [number, number, number], onWhite: readonly [number, number, number]): Rgba {
  const [b0, b1, b2] = onBlack;
  const [w0, w1, w2] = onWhite;
  // 白の上と黒の上の差が (1 − a)·255 になる。3 チャンネルの平均を取る
  const a = (3 - (w0 - b0 + w1 - b1 + w2 - b2) / 255) / 3;
  if (a <= 1e-6) {
    return { r: 0, g: 0, b: 0, a: 0 };
  }
  const channel = (v: number) => Math.min(255, Math.max(0, v / a));
  return { r: channel(b0), g: channel(b1), b: channel(b2), a: Math.min(1, Math.max(0, a)) };
}

/**
 * 近い祖先から順に並んだ背景を、不透明な白を基準にして合成した 1 つの背景色を返す。
 * 背景が 1 つも無ければ白。
 */
export function effectiveBackground(backgrounds: readonly Rgba[]): Rgba {
  let base: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const background of [...backgrounds].reverse()) {
    base = composite(background, base);
  }
  return base;
}

/**
 * 要素の文字色と、祖先をたどった背景色の重なりを、ページ内のキャンバスで sRGB の RGBA に正規化して返す。
 * `backgrounds` は近い祖先から順(要素自身を含む)。
 */
export async function readEffectiveColors(target: Locator): Promise<{ foreground: Rgba; backgrounds: Rgba[] }> {
  const raw = await target.evaluate((el) => {
    const canvas = document.createElement("canvas");
    canvas.width = 1;
    canvas.height = 1;
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (ctx === null) {
      throw new Error("2D キャンバスを取得できません");
    }
    // 黒と白の不透明な背景の上に同じ色を描き、その画素を返す(後で合成式を解く)
    const sample = (css: string): [[number, number, number], [number, number, number]] => {
      const pixel = (backdrop: string): [number, number, number] => {
        ctx.globalCompositeOperation = "source-over";
        ctx.fillStyle = backdrop;
        ctx.fillRect(0, 0, 1, 1);
        ctx.fillStyle = css;
        ctx.fillRect(0, 0, 1, 1);
        const d = ctx.getImageData(0, 0, 1, 1).data;
        return [d[0] ?? 0, d[1] ?? 0, d[2] ?? 0];
      };
      return [pixel("#000"), pixel("#fff")];
    };
    const style = getComputedStyle(el);
    const backgrounds: [[number, number, number], [number, number, number]][] = [];
    for (let node: Element | null = el; node !== null; node = node.parentElement) {
      backgrounds.push(sample(getComputedStyle(node).backgroundColor));
    }
    return { foreground: sample(style.color), backgrounds };
  });
  return {
    foreground: recoverRgba(raw.foreground[0], raw.foreground[1]),
    // 近い祖先から順
    backgrounds: raw.backgrounds.map(([onBlack, onWhite]) => recoverRgba(onBlack, onWhite)),
  };
}

/** 要素の文字色と実際の背景のコントラスト比。半透明の文字色は背景に合成してから比べる */
export async function measureContrast(target: Locator): Promise<number> {
  const { foreground, backgrounds } = await readEffectiveColors(target);
  const background = effectiveBackground(backgrounds);
  return contrastRatio(composite(foreground, background), background);
}
