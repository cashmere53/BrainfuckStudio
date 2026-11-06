import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// jsdom は Range の座標 API を持たない。CodeMirror は停止時のスクロール(scrollIntoView)でこれを呼ぶため、
// 空の矩形を返す最小の実装で補う(座標の正しさは jsdom では検証しない。見た目は手動スモークで確認する)
if (typeof Range !== "undefined") {
  if (typeof Range.prototype.getClientRects !== "function") {
    Range.prototype.getClientRects = function getClientRects() {
      return [] as unknown as DOMRectList;
    };
  }
  if (typeof Range.prototype.getBoundingClientRect !== "function") {
    Range.prototype.getBoundingClientRect = function getBoundingClientRect() {
      return new DOMRect(0, 0, 0, 0);
    };
  }
}

afterEach(() => {
  cleanup();
});
