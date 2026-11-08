// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  composite,
  contrastRatio,
  effectiveBackground,
  recoverRgba,
  relativeLuminance,
  type Rgba,
} from "./contrast.ts";

const WHITE: Rgba = { r: 255, g: 255, b: 255, a: 1 };
const BLACK: Rgba = { r: 0, g: 0, b: 0, a: 1 };

describe("relativeLuminance(WCAG 2.x の相対輝度)", () => {
  it("白は 1、黒は 0", () => {
    expect(relativeLuminance(WHITE)).toBeCloseTo(1, 6);
    expect(relativeLuminance(BLACK)).toBeCloseTo(0, 6);
  });

  it("既知の値: #777777 は約 0.1845、#0a0a0a は線形域で約 0.003035、#808080 は約 0.2159", () => {
    expect(relativeLuminance({ r: 119, g: 119, b: 119, a: 1 })).toBeCloseTo(0.1845, 4);
    expect(relativeLuminance({ r: 10, g: 10, b: 10, a: 1 })).toBeCloseTo(10 / 255 / 12.92, 6);
    expect(relativeLuminance({ r: 128, g: 128, b: 128, a: 1 })).toBeCloseTo(0.2159, 4);
  });

  it("線形化の閾値は 0.04045(WCAG 2.x)。0.03928 〜 0.04045 の間は線形域を使う", () => {
    // 0.04 は 0.03928 より大きく 0.04045 より小さい。正しい閾値なら線形式 c / 12.92 を使う
    // (誤った閾値 0.03928 だとべき乗式になり、差は約 7e-7 で、1e-9 の精度で区別できる)
    const c = 0.04;
    expect(relativeLuminance({ r: c * 255, g: c * 255, b: c * 255, a: 1 })).toBeCloseTo(c / 12.92, 9);
  });

  it("閾値の上下で輝度が単調に増える(10 と 11 の画素値)", () => {
    const at = (v: number) => relativeLuminance({ r: v, g: v, b: v, a: 1 });
    expect(at(10)).toBeLessThan(at(11));
    expect(at(11)).toBeGreaterThan(at(10));
  });

  it("純色の係数(赤 0.2126、緑 0.7152、青 0.0722)を使う", () => {
    expect(relativeLuminance({ r: 255, g: 0, b: 0, a: 1 })).toBeCloseTo(0.2126, 4);
    expect(relativeLuminance({ r: 0, g: 255, b: 0, a: 1 })).toBeCloseTo(0.7152, 4);
    expect(relativeLuminance({ r: 0, g: 0, b: 255, a: 1 })).toBeCloseTo(0.0722, 4);
  });
});

describe("contrastRatio(コントラスト比)", () => {
  it("白黒は 21:1", () => {
    expect(contrastRatio(WHITE, BLACK)).toBeCloseTo(21, 5);
  });

  it("同色は 1:1", () => {
    const gray: Rgba = { r: 119, g: 119, b: 119, a: 1 };
    expect(contrastRatio(gray, gray)).toBeCloseTo(1, 6);
  });

  it("既知の値: #777777 対白は 4.5 未満、#767676 対白は 4.5 以上", () => {
    const gray777 = contrastRatio({ r: 0x77, g: 0x77, b: 0x77, a: 1 }, WHITE);
    const gray767 = contrastRatio({ r: 0x76, g: 0x76, b: 0x76, a: 1 }, WHITE);
    expect(gray777).toBeCloseTo(4.48, 1);
    expect(gray777).toBeLessThan(4.5);
    expect(gray767).toBeCloseTo(4.54, 1);
    expect(gray767).toBeGreaterThanOrEqual(4.5);
  });

  it("前景の明るさが暗い側の境界(10 と 11)を越えても比は単調に変わる", () => {
    const ratio = (v: number) => contrastRatio({ r: v, g: v, b: v, a: 1 }, WHITE);
    expect(ratio(10)).toBeGreaterThan(ratio(11));
    expect(ratio(11)).toBeGreaterThan(ratio(12));
  });

  it("前景と背景の順序によらない", () => {
    const text: Rgba = { r: 40, g: 40, b: 40, a: 1 };
    expect(contrastRatio(text, WHITE)).toBeCloseTo(contrastRatio(WHITE, text), 9);
  });
});

describe("composite(半透明の合成)", () => {
  it("50% の黒を白の上に載せると中間の灰色(127.5)になる", () => {
    const out = composite({ r: 0, g: 0, b: 0, a: 0.5 }, WHITE);
    expect(out.r).toBeCloseTo(127.5, 6);
    expect(out.g).toBeCloseTo(127.5, 6);
    expect(out.b).toBeCloseTo(127.5, 6);
    expect(out.a).toBeCloseTo(1, 6);
  });

  it("不透明な上の色は下の色を隠す", () => {
    expect(composite({ r: 10, g: 20, b: 30, a: 1 }, WHITE)).toEqual({ r: 10, g: 20, b: 30, a: 1 });
  });

  it("透明な上の色は下の色をそのまま返す", () => {
    expect(composite({ r: 0, g: 0, b: 0, a: 0 }, WHITE)).toEqual(WHITE);
  });

  it("両方が半透明のとき、アルファは a1 + a2(1 - a1) になる", () => {
    const out = composite(
      { r: 255, g: 0, b: 0, a: 0.5 },
      { r: 0, g: 0, b: 255, a: 0.5 },
    );
    expect(out.a).toBeCloseTo(0.75, 6);
  });
});

describe("recoverRgba(キャンバスの 2 つの背景の上の画素から色を戻す)", () => {
  it("不透明な色は両方の背景で同じ値になり、そのまま返す", () => {
    const out = recoverRgba([12, 34, 56], [12, 34, 56]);
    expect(out.a).toBeCloseTo(1, 6);
    expect(out.r).toBeCloseTo(12, 6);
    expect(out.g).toBeCloseTo(34, 6);
    expect(out.b).toBeCloseTo(56, 6);
  });

  it("50% の赤は黒の上で (128,0,0)、白の上で (255,128,128) になり、元の赤と 50% に戻す", () => {
    const out = recoverRgba([128, 0, 0], [255, 128, 128]);
    expect(out.a).toBeCloseTo(0.502, 2);
    expect(out.r).toBeCloseTo(255, 0);
    expect(out.g).toBeCloseTo(0, 0);
    expect(out.b).toBeCloseTo(0, 0);
  });

  it("完全に透明な色は a = 0 として返す", () => {
    expect(recoverRgba([0, 0, 0], [255, 255, 255]).a).toBeCloseTo(0, 6);
  });
});

describe("effectiveBackground(祖先の背景の重なり)", () => {
  it("背景が無ければ白(不透明)になる", () => {
    expect(effectiveBackground([])).toEqual(WHITE);
    expect(effectiveBackground([{ r: 0, g: 0, b: 0, a: 0 }])).toEqual(WHITE);
  });

  it("近い祖先の半透明の赤は、遠い祖先の黒の上に合成される", () => {
    // 配列は近い祖先から順。最も遠い黒が基準になる
    const out = effectiveBackground([
      { r: 255, g: 0, b: 0, a: 0.5 },
      BLACK,
    ]);
    expect(out.a).toBeCloseTo(1, 6);
    expect(out.r).toBeCloseTo(127.5, 6);
    expect(out.g).toBeCloseTo(0, 6);
    expect(out.b).toBeCloseTo(0, 6);
  });
});
