// スモーク 7(ブレークポイント): ガターのクリックと F9 での付け外し、停止と再開、入力欄での F9 の扱いを確認する。
// 停止位置の値(pc)と停止理由は、画面の表示と、ブリッジが返した run_batch の応答の両方で突き合わせる。
import { expect, test } from "../support/fixtures.ts";

/** run_batch の応答のうち、このシナリオが読む部分(apps/desktop/src/generated/BatchResultDto.ts と一致) */
type BatchResult = {
  stop: { kind: string };
  executed: number;
  state: { output: { dataBase64: string } };
};

/**
 * hello.bf の 52 文字目(0 始まりで 51)の出力命令 "." にカーソルを置く。全文字が命令なので pc は 51 になる。
 * 外側のループの後ろにあり一度だけ実行されるため、再開しても同じ位置には戻らない(ループの中の位置では
 * 再開後に次の周回で同じ位置に止まるため、再開の検証には使えない)。
 */
const BREAKPOINT_INDEX = 51;

test.describe("スモーク 7: ブレークポイント", () => {
  test("ガターのクリックで行の最初の命令に丸印が付き、もう一度のクリックで外れる", async ({ debuggerPage }) => {
    await expect(debuggerPage.breakpointMarkers).toHaveCount(0);

    await debuggerPage.clickBreakpointGutter(1);
    await expect(debuggerPage.breakpointMarkers).toHaveCount(1);
    expect(await debuggerPage.hasBreakpointAt(1)).toBe(true);

    await debuggerPage.clickBreakpointGutter(1);
    await expect(debuggerPage.breakpointMarkers).toHaveCount(0);
    expect(await debuggerPage.hasBreakpointAt(1)).toBe(false);
  });

  test("F9 はカーソル以降で最初の命令に付き、カーソルのある行の命令には付かない", async ({ debuggerPage }) => {
    // 1 行目は命令が無く、2 行目の "+" が最初の命令。カーソルは文書の先頭(1 行目)に置く
    await debuggerPage.setSource("a\n+");
    await debuggerPage.page.keyboard.press("ControlOrMeta+Home");

    await debuggerPage.page.keyboard.press("F9");
    await expect(debuggerPage.breakpointMarkers).toHaveCount(1);
    expect(await debuggerPage.hasBreakpointAt(1)).toBe(false);
    expect(await debuggerPage.hasBreakpointAt(2)).toBe(true);

    // もう一度 F9 で外れる(同じ位置で切り替わる)
    await debuggerPage.page.keyboard.press("F9");
    await expect(debuggerPage.breakpointMarkers).toHaveCount(0);
  });

  test("F9 をカーソルの位置から押すと、カーソル以降の最初の命令に付き、文書の先頭の行には付かない", async ({
    debuggerPage,
  }) => {
    // 2 行目の 2 文字目(途中)にカーソルを置く。先頭の行は命令だけなので、文書の先頭を基準にすると 1 行目に付いてしまう
    await debuggerPage.setSource("++\n++\n++");
    await debuggerPage.editor.focus();
    await debuggerPage.page.keyboard.press("ControlOrMeta+Home");
    await debuggerPage.page.keyboard.press("ArrowDown");
    await debuggerPage.page.keyboard.press("ArrowRight");

    await debuggerPage.page.keyboard.press("F9");

    await expect(debuggerPage.breakpointMarkers).toHaveCount(1);
    expect(await debuggerPage.hasBreakpointAt(2)).toBe(true);
    expect(await debuggerPage.hasBreakpointAt(1)).toBe(false);
  });

  test("ブレークポイントで Run すると同じ位置で停止し、ラベルが「ブレークポイントで停止」になり、続けて Run すると最後まで進む", async ({
    backend,
    debuggerPage,
  }) => {
    await debuggerPage.editor.focus();
    await debuggerPage.page.keyboard.press("ControlOrMeta+Home");
    for (let i = 0; i < BREAKPOINT_INDEX; i += 1) {
      await debuggerPage.page.keyboard.press("ArrowRight");
    }
    await debuggerPage.page.keyboard.press("F9");
    await expect(debuggerPage.breakpointMarkers).toHaveCount(1);

    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("ブレークポイントで停止");
    await expect.poll(() => debuggerPage.statusValue("pc")).toBe(BREAKPOINT_INDEX);
    // 停止理由は実際の応答の停止理由(breakpoint)と一致し、停止の前に出力は無い
    await expect.poll(() => backend.calls("run_batch").at(-1)?.outcome?.ok ?? null).toBe(true);
    const firstRun = backend
      .calls("run_batch")
      .map((batch) => (batch.outcome as { ok: true; value: BatchResult }).value);
    expect(firstRun.at(-1)?.stop.kind).toBe("breakpoint");
    expect(
      firstRun.map((result) => Buffer.from(result.state.output.dataBase64, "base64").toString("utf8")).join(""),
    ).toBe("");

    // 続けて Run: 同じ位置では止まらず、最後まで進んで正常終了する
    const callsBeforeResume = backend.calls("run_batch").length;
    await debuggerPage.button("Run").click();

    await expect(debuggerPage.statusLabel).toHaveText("正常終了");
    await expect(debuggerPage.outputText).toHaveText("Hello World!");
    const resumed = backend
      .calls("run_batch")
      .slice(callsBeforeResume)
      .map((batch) => (batch.outcome as { ok: true; value: BatchResult }).value);
    expect(resumed.length).toBeGreaterThan(0);
    expect(resumed.some((result) => result.stop.kind === "breakpoint")).toBe(false);
    expect(resumed.at(-1)?.stop.kind).toBe("halted");
  });

  test("入力欄にフォーカスがあるときの F9 では、ブレークポイントの数も位置も変わらない", async ({ debuggerPage }) => {
    await debuggerPage.editor.focus();
    await debuggerPage.page.keyboard.press("ControlOrMeta+Home");
    await debuggerPage.clickBreakpointGutter(1);
    await expect(debuggerPage.breakpointMarkers).toHaveCount(1);

    // カーソルは 1 行目の最初の命令の上にある。エディタで F9 を押すと外れるため、入力欄での F9 の差が出る
    await debuggerPage.inputBox.focus();
    await expect(debuggerPage.inputBox).toBeFocused();
    await debuggerPage.page.keyboard.press("F9");

    await expect(debuggerPage.breakpointMarkers).toHaveCount(1);
    expect(await debuggerPage.hasBreakpointAt(1)).toBe(true);
  });
});
