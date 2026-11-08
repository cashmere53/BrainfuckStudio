// スモーク 3.1(Hello World の実行): 初期のソースを Run し、出力・状態・実行数が実際の Rust の応答と一致することを確認する。
// 値は画面の表示だけでなく、ブリッジが返した run_batch の応答(呼び出しの記録)とも突き合わせる。
import { readFileSync } from "node:fs";
import path from "node:path";
import { expect, test } from "../support/fixtures.ts";
import { REPO_ROOT } from "../support/paths.ts";

const HELLO_SOURCE = readFileSync(path.join(REPO_ROOT, "examples", "hello.bf"), "utf8").trimEnd();

/** run_batch の応答のうち、このシナリオが読む部分(apps/desktop/src/generated/BatchResultDto.ts と一致) */
type BatchResult = {
  stop: { kind: string };
  executed: number;
  state: { output: { dataBase64: string } };
};

test.describe("スモーク 3.1: Hello World の実行", () => {
  test("Run で出力 Hello World!、正常終了、pc 106・dp 6・実行数 906 になり、値は run_batch の応答と一致する", async ({
    backend,
    debuggerPage,
  }) => {
    expect((await debuggerPage.sourceText()).trimEnd()).toBe(HELLO_SOURCE);
    expect(backend.calls("run_batch")).toEqual([]);

    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    await expect(debuggerPage.outputText).toHaveText("Hello World!");
    await expect.poll(() => debuggerPage.statusValue("pc")).toBe(106);
    await expect.poll(() => debuggerPage.statusValue("dp")).toBe(6);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(906);

    // 値が実際の Rust から返ったことを確かめる: run_batch の応答がすべて成功し、停止は終端、実行数の合計は 906、
    // 出力のバイト列は画面の表示と同じ Hello World! の改行付きになる
    const batches = backend.calls("run_batch");
    expect(batches.length).toBeGreaterThan(0);
    for (const batch of batches) {
      await expect.poll(() => batch.outcome?.ok ?? null).toBe(true);
    }
    const results = batches.map((batch) => (batch.outcome as { ok: true; value: BatchResult }).value);
    expect(results.reduce((sum, result) => sum + result.executed, 0)).toBe(906);
    expect(results.at(-1)?.stop.kind).toBe("halted");
    const bytes = results.map((result) => Buffer.from(result.state.output.dataBase64, "base64").toString("utf8")).join("");
    expect(bytes).toBe("Hello World!\n");
  });
});
