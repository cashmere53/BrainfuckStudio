import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { getHelloSource } from "./hello";

describe("getHelloSource", () => {
  it("returns the examples/hello.bf contents imported at build time without copying the file", () => {
    // npm スクリプトは apps/desktop で実行される前提(jsdom 下では import.meta.url が file: でないため cwd を使う)
    const original = readFileSync(resolve(process.cwd(), "../../examples/hello.bf"), "utf8");
    expect(getHelloSource()).toBe(original);
    expect(getHelloSource()).toContain("+");
  });
});
