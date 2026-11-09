// スモーク 3.1(Hello World の実行)の webview 版。初期のソースを Run し、出力・状態・実行数の表示を確かめ、
// 読み込み・開始・実行のコマンド(load_source・start_session・run_batch)の要求と完了が実アプリの IPC で観測されることを確かめる(4.2)。
// 完了の観測は応答を待ち受けてから操作する(armResponse は操作の前に await して確定させる)。
import { readFileSync } from "node:fs";
import path from "node:path";
import { expect, test } from "../support/webview/fixtures.ts";
import { REPO_ROOT } from "../support/paths.ts";

const HELLO_SOURCE = readFileSync(
  path.join(REPO_ROOT, "examples", "hello.bf"),
  "utf8",
).trimEnd();

test.describe("スモーク 3.1: Hello World の実行", () => {
  test("スモーク 3.1 Run で出力 Hello World!、正常終了、pc 106・dp 6・実行数 906 になり、load_source・start_session・run_batch の要求と完了が IPC で観測される", async ({
    app,
  }) => {
    const { debuggerPage, ipc } = app;
    expect((await debuggerPage.sourceText()).trimEnd()).toBe(HELLO_SOURCE);
    expect(await ipc.commands()).not.toContain("run_batch");

    // 操作の前に待ち受けを確定させる(確定の前に完了した応答は数えない)
    const loaded = await ipc.armResponse("load_source");
    const started = await ipc.armResponse("start_session");
    const batched = await ipc.armResponse("run_batch");

    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    await expect(debuggerPage.outputText).toHaveText("Hello World!");
    await expect.poll(() => debuggerPage.statusValue("pc")).toBe(106);
    await expect.poll(() => debuggerPage.statusValue("dp")).toBe(6);
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(906);

    // 要求が出たことと、Rust からの応答(runCallback)が返ったことを観測する
    // 完了の成否も確かめる(エラーの応答で解決しても、成功でなければ失敗にする)
    const results = await Promise.all([loaded.completed, started.completed, batched.completed]);
    expect(results.map((result) => result.ok)).toEqual([true, true, true]);
    const commands = await ipc.commands();
    const first = (name: string): number => commands.indexOf(name);
    expect(first("load_source")).toBeGreaterThanOrEqual(0);
    expect(first("start_session")).toBeGreaterThan(first("load_source"));
    expect(first("run_batch")).toBeGreaterThan(first("start_session"));
  });
});
