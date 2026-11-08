// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { Locator, Page } from "@playwright/test";
import {
  assertFocusRingSides,
  differingSides,
  expectFocusRingDrawn,
  outlineBands,
  type Band,
  type Side,
} from "./focusRing.ts";

describe("outlineBands(outline の 4 辺の帯)", () => {
  const box = { x: 10, y: 20, width: 100, height: 50 };

  it("offset 0 のとき、帯は要素の外側に width の厚さで 4 辺に並ぶ", () => {
    const bands = outlineBands(box, 2, 0);
    expect(bands.top).toEqual({ x: 8, y: 18, width: 104, height: 2 });
    expect(bands.bottom).toEqual({ x: 8, y: 70, width: 104, height: 2 });
    expect(bands.left).toEqual({ x: 8, y: 20, width: 2, height: 50 });
    expect(bands.right).toEqual({ x: 110, y: 20, width: 2, height: 50 });
  });

  it("offset は要素の辺から外側へ離す(帯は offset の分だけ外に動く)", () => {
    const bands = outlineBands(box, 2, 3);
    expect(bands.top).toEqual({ x: 5, y: 15, width: 110, height: 2 });
    expect(bands.bottom).toEqual({ x: 5, y: 73, width: 110, height: 2 });
    expect(bands.left).toEqual({ x: 5, y: 17, width: 2, height: 56 });
    expect(bands.right).toEqual({ x: 113, y: 17, width: 2, height: 56 });
  });

  it("どの帯も要素の内側には入らない", () => {
    const bands = outlineBands(box, 3, 0);
    const inside = (b: { x: number; y: number; width: number; height: number }) =>
      b.x < box.x + box.width &&
      b.x + b.width > box.x &&
      b.y < box.y + box.height &&
      b.y + b.height > box.y;
    for (const band of Object.values(bands)) {
      expect(inside(band)).toBe(false);
    }
  });
});

describe("differingSides(フォーカス時とブラー時の帯の比較)", () => {
  const same = new Uint8Array([1, 2, 3]);
  const other = new Uint8Array([9, 9, 9]);

  it("全辺が異なれば 4 辺すべてを返す", () => {
    const before = { top: same, right: same, bottom: same, left: same };
    const after = { top: other, right: other, bottom: other, left: other };
    expect(differingSides(before, after)).toEqual(["top", "right", "bottom", "left"]);
  });

  it("描画されていない辺(同じ画素)は含めない", () => {
    const before = { top: same, right: same, bottom: same, left: same };
    const after = { top: other, right: same, bottom: other, left: other };
    expect(differingSides(before, after)).toEqual(["top", "bottom", "left"]);
  });

  it("バイト列の長さが違うものは異なるとみなす", () => {
    const before = { top: same, right: same, bottom: same, left: same };
    const after = { ...before, top: new Uint8Array([1, 2]) };
    expect(differingSides(before, after)).toEqual(["top"]);
  });

  it("後ろが長く前が接頭辞なら異なるとみなす([1,2] と [1,2,3])", () => {
    const before = { top: new Uint8Array([1, 2]), right: same, bottom: same, left: same };
    const after = { ...before, top: new Uint8Array([1, 2, 3]) };
    expect(differingSides(before, after)).toEqual(["top"]);
  });

  it("前が長く後ろが接頭辞でも異なるとみなす", () => {
    const before = { top: new Uint8Array([1, 2, 3]), right: same, bottom: same, left: same };
    const after = { ...before, top: new Uint8Array([1, 2]) };
    expect(differingSides(before, after)).toEqual(["top"]);
  });

  it("1 バイトだけが異なる辺も検出する", () => {
    const before = { top: new Uint8Array([5, 5, 5, 5]), right: same, bottom: same, left: same };
    const after = { ...before, top: new Uint8Array([5, 5, 6, 5]) };
    expect(differingSides(before, after)).toEqual(["top"]);
  });

  it("全辺が同じなら空の一覧を返す", () => {
    const before = { top: same, right: same, bottom: same, left: same };
    const after = { top: new Uint8Array([1, 2, 3]), right: new Uint8Array([1, 2, 3]), bottom: new Uint8Array([1, 2, 3]), left: new Uint8Array([1, 2, 3]) };
    expect(differingSides(before, after)).toEqual([]);
  });
});

describe("assertFocusRingSides(4 辺すべての描画の判定)", () => {
  const same = new Uint8Array([1, 2, 3]);
  const other = new Uint8Array([9, 9, 9]);

  it("全辺が異なれば例外にしない", () => {
    const before = { top: same, right: same, bottom: same, left: same };
    const after = { top: other, right: other, bottom: other, left: other };
    expect(() => assertFocusRingSides(before, after)).not.toThrow();
  });

  it("1 辺でも同じなら、その辺の名前を含む例外を投げる", () => {
    const before = { top: same, right: same, bottom: same, left: same };
    const after = { top: other, right: other, bottom: other, left: same };
    expect(() => assertFocusRingSides(before, after)).toThrow(/left/);
    expect(() => assertFocusRingSides(before, after)).not.toThrow(/top/);
  });

  it("欠けた辺が複数あれば、すべての名前を含む", () => {
    const before = { top: same, right: same, bottom: same, left: same };
    const after = { top: other, right: same, bottom: same, left: other };
    expect(() => assertFocusRingSides(before, after)).toThrow(/right, bottom/);
  });
});

// 以下は expectFocusRingDrawn の偽物の Locator と Page による検証。
// 偽物の page は、focus() が呼ばれた後の screenshot に「描画あり」の画像を返す。
type Style = { outlineStyle: string; outlineWidth: number; outlineOffset: number };
const BOX = { x: 10, y: 20, width: 100, height: 50 };

function fakeRing(style: Style, box: typeof BOX | null): Locator {
  return {
    evaluate: async () => style,
    boundingBox: async () => box,
  } as unknown as Locator;
}

function fakePage(options: { drawnSides: readonly Side[]; style: Style }) {
  const bands = outlineBands(BOX, options.style.outlineWidth, options.style.outlineOffset);
  let focused = false;
  const page = {
    evaluate: async () => {
      focused = false;
    },
    screenshot: async (opts: { clip: Band }) => {
      const side = (Object.keys(bands) as Side[]).find((s) => bands[s].x === opts.clip.x && bands[s].y === opts.clip.y);
      if (side === undefined) {
        throw new Error("想定外の clip");
      }
      const drawn = focused && options.drawnSides.includes(side);
      return drawn ? new Uint8Array([9, 9, 9]).buffer : new Uint8Array([1, 2, 3]).buffer;
    },
  };
  const focus = async () => {
    focused = true;
  };
  return { page: page as unknown as Page, focus };
}

describe("expectFocusRingDrawn(偽物の Locator と Page による検証)", () => {
  const style: Style = { outlineStyle: "solid", outlineWidth: 2, outlineOffset: 0 };

  it("4 辺すべてが描かれていれば成功する", async () => {
    const { page, focus } = fakePage({ drawnSides: ["top", "right", "bottom", "left"], style });
    await expect(expectFocusRingDrawn(page, { focus, ring: fakeRing(style, BOX) })).resolves.toBeUndefined();
  });

  it("1 辺でも描かれなければ、その辺の名前を含む例外を投げる", async () => {
    const { page, focus } = fakePage({ drawnSides: ["top", "right", "bottom"], style });
    await expect(expectFocusRingDrawn(page, { focus, ring: fakeRing(style, BOX) })).rejects.toThrow(/left/);
  });

  it("outline-style が none なら例外にする", async () => {
    const none: Style = { ...style, outlineStyle: "none" };
    const { page, focus } = fakePage({ drawnSides: ["top", "right", "bottom", "left"], style });
    await expect(expectFocusRingDrawn(page, { focus, ring: fakeRing(none, BOX) })).rejects.toThrow(/outline-style: none/);
  });

  it("outline の幅が 0 なら例外にする", async () => {
    const zero: Style = { ...style, outlineWidth: 0 };
    const { page, focus } = fakePage({ drawnSides: ["top", "right", "bottom", "left"], style });
    await expect(expectFocusRingDrawn(page, { focus, ring: fakeRing(zero, BOX) })).rejects.toThrow(/フォーカス枠が指定/);
  });

  it("boundingBox が null なら例外にする", async () => {
    const { page, focus } = fakePage({ drawnSides: ["top", "right", "bottom", "left"], style });
    await expect(expectFocusRingDrawn(page, { focus, ring: fakeRing(style, null) })).rejects.toThrow(/位置を取得/);
  });
});
