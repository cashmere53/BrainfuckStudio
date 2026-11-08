// スモーク 10(保存): 保存先を指定した保存と名前を付けて保存(7.1)、保存先の選択の取消(7.2)、
// 読み取り専用・存在しないディレクトリへの保存の失敗(7.3)、保存先の選択を保留している間の編集(7.4)。
// ファイルの内容は文字列で比べる(保存する文字列は LF のみ)。
// 保存先の選択はダイアログを使わず、テストの待ち行列(enqueue / hold)で結果を決める。
import { existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import type { TestInfo } from "@playwright/test";
import { missingDirectoryPath, writeReadOnlyFile } from "../support/files.ts";
import { type E2eBackend, expect, test } from "../support/fixtures.ts";

/** 未保存の変更がある間の文書の表示の接尾辞 */
const DIRTY_SUFFIX = "(未保存の変更あり)";

/** 無題で未保存の表示 */
const UNTITLED_DIRTY = `無題${DIRTY_SUFFIX}`;

/** テストごとの出力先(ファイルを作る場所。無ければ作る) */
function outputDir(testInfo: TestInfo): string {
  const dir = testInfo.outputPath("files");
  mkdirSync(dir, { recursive: true });
  return dir;
}

/** 最後に送られた未保存の旗(set_document_dirty の引数)。送られていなければ undefined */
function lastDirtyFlag(backend: E2eBackend): unknown {
  const calls = backend.calls("set_document_dirty");
  const last = calls[calls.length - 1];
  return (last?.payload as { args: { dirty: boolean } } | undefined)?.args.dirty;
}

/**
 * 保存の応答が返った後(startedAt が保存の endedAt 以降)に送られた未保存の旗の並び。保存が未完了なら空。
 * 保存で Rust 側の旗が下がるため、保存の後に表示上の値を送り直したかをこれで見る。
 */
function dirtyFlagsSentAfterSave(backend: E2eBackend): unknown[] {
  const endedAt = backend.calls("save_source_file")[0]?.endedAt ?? null;
  if (endedAt === null) return [];
  return backend
    .calls("set_document_dirty")
    .filter((call) => call.startedAt >= endedAt)
    .map((call) => (call.payload as { args: { dirty: boolean } }).args.dirty);
}

test.describe("スモーク 10: 保存", () => {
  test("7.1 無題の文書を Ctrl+S で保存すると保存先の選択が出て、ファイルに内容が書かれ、未保存の表示が消えてファイル名だけが出る", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const target = path.join(outputDir(testInfo), "saved.bf");
    backend.dialogs.enqueue("save", { kind: "path", path: target });

    await debuggerPage.setSource("+.");
    await expect(debuggerPage.documentStatus).toHaveText(UNTITLED_DIRTY);

    await debuggerPage.page.keyboard.press("ControlOrMeta+S");

    await expect(debuggerPage.documentStatus).toHaveText("saved.bf");
    expect(readFileSync(target, "utf8")).toBe("+.");
    expect(backend.calls("save_source_file").map((call) => call.payload)).toEqual([
      { args: { source: "+.", saveAs: false } },
    ]);
    expect(backend.calls("save_source_file")[0]?.outcome).toEqual({
      ok: true,
      value: { kind: "saved", fileName: "saved.bf" },
    });
    await expect.poll(() => lastDirtyFlag(backend)).toBe(false);
  });

  test("7.1 名前を付けて保存は保存先の選択を出し、既存のファイルを変えずに新しいファイルへ書き、ファイル名の表示を新しい名前にする", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const dir = outputDir(testInfo);
    const first = path.join(dir, "first.bf");
    const second = path.join(dir, "second.bf");

    backend.dialogs.enqueue("save", { kind: "path", path: first });
    await debuggerPage.setSource("+.");
    await debuggerPage.page.keyboard.press("ControlOrMeta+S");
    await expect(debuggerPage.documentStatus).toHaveText("first.bf");

    backend.dialogs.enqueue("save", { kind: "path", path: second });
    await debuggerPage.setSource("++.");
    await expect(debuggerPage.documentStatus).toHaveText(`first.bf${DIRTY_SUFFIX}`);
    await debuggerPage.page.keyboard.press("ControlOrMeta+Shift+S");

    await expect(debuggerPage.documentStatus).toHaveText("second.bf");
    expect(readFileSync(second, "utf8")).toBe("++.");
    expect(readFileSync(first, "utf8")).toBe("+.");
    expect(backend.calls("save_source_file").map((call) => call.payload)).toEqual([
      { args: { source: "+.", saveAs: false } },
      { args: { source: "++.", saveAs: true } },
    ]);
    await expect.poll(() => lastDirtyFlag(backend)).toBe(false);
  });

  test("7.2 保存先の選択を取り消すとファイルは作られず、未保存の表示と編集は残り、通知は出ない", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const dir = outputDir(testInfo);
    backend.dialogs.enqueue("save", { kind: "cancel" });

    await debuggerPage.setSource("+.");
    await debuggerPage.page.keyboard.press("ControlOrMeta+S");

    await expect
      .poll(() => backend.calls("save_source_file")[0]?.outcome ?? null)
      .toEqual({ ok: true, value: { kind: "cancelled" } });
    expect(readdirSync(dir).filter((name) => name.endsWith(".bf"))).toEqual([]);
    await expect(debuggerPage.documentStatus).toHaveText(UNTITLED_DIRTY);
    expect(await debuggerPage.sourceText()).toBe("+.");
    await expect(debuggerPage.notices).toBeHidden();
  });

  test("7.3 読み取り専用のファイルへの保存は原因つきの書き込みの失敗を操作エラーに出し、編集内容を残す", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const readOnly = writeReadOnlyFile(outputDir(testInfo));
    test.skip(readOnly === null, "読み取り専用の属性が効かない環境(root で動く環境など)では、この検査を行えない");
    if (readOnly === null) return;

    backend.dialogs.enqueue("save", { kind: "path", path: readOnly });
    await debuggerPage.setSource("+.");
    await debuggerPage.page.keyboard.press("ControlOrMeta+S");

    await expect(debuggerPage.notices).toContainText("操作エラー");
    await expect(debuggerPage.notices).toContainText("ファイルの書き込みに失敗しました(原因: ");
    expect(backend.calls("save_source_file")[0]?.outcome).toMatchObject({ ok: false });
    expect(await debuggerPage.sourceText()).toBe("+.");
    await expect(debuggerPage.documentStatus).toHaveText(UNTITLED_DIRTY);
    expect(readFileSync(readOnly, "utf8")).toBe("+.");
  });

  test("7.3 存在しないディレクトリへの保存は原因つきの書き込みの失敗を操作エラーに出し、ファイルは作られず、編集内容を残す", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const missing = missingDirectoryPath(outputDir(testInfo));
    backend.dialogs.enqueue("save", { kind: "path", path: missing });

    await debuggerPage.setSource("+.");
    await debuggerPage.page.keyboard.press("ControlOrMeta+S");

    await expect(debuggerPage.notices).toContainText("操作エラー");
    await expect(debuggerPage.notices).toContainText("ファイルの書き込みに失敗しました(原因: ");
    expect(backend.calls("save_source_file")[0]?.outcome).toMatchObject({ ok: false });
    expect(existsSync(missing)).toBe(false);
    expect(await debuggerPage.sourceText()).toBe("+.");
    await expect(debuggerPage.documentStatus).toHaveText(UNTITLED_DIRTY);
  });

  test("7.4 保存先の選択を保留したまま編集すると、解放後も未保存の表示が残り、ファイルには保存を要求した時点の内容が書かれる", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const target = path.join(outputDir(testInfo), "held.bf");
    await debuggerPage.setSource("+.");

    const held = backend.dialogs.hold("save");
    await debuggerPage.page.keyboard.press("ControlOrMeta+S");
    await held.requested;

    // 保存先の選択の保留中に 1 文字追加する(保存を要求した時点の "+." と食い違わせる)
    await debuggerPage.editor.click();
    await debuggerPage.page.keyboard.press("ControlOrMeta+End");
    await debuggerPage.page.keyboard.type("+");
    await expect.poll(() => debuggerPage.sourceText()).toBe("+.+");

    held.release({ kind: "path", path: target });

    await expect
      .poll(() => backend.calls("save_source_file")[0]?.outcome ?? null)
      .toEqual({ ok: true, value: { kind: "saved", fileName: "held.bf" } });
    expect(backend.calls("save_source_file")[0]?.payload).toEqual({ args: { source: "+.", saveAs: false } });
    expect(readFileSync(target, "utf8")).toBe("+.");
    await expect(debuggerPage.documentStatus).toHaveText(`held.bf${DIRTY_SUFFIX}`);
    expect(await debuggerPage.sourceText()).toBe("+.+");
    await expect.poll(() => lastDirtyFlag(backend)).toBe(true);

    // 保存の応答の後に未保存の旗を送り直し、その最後の値は true(Rust 側の旗が下がったままにならない)
    await expect.poll(() => dirtyFlagsSentAfterSave(backend).at(-1)).toBe(true);

    // 旗が true に戻っているため、解放の後に閉じる要求をすると確認ダイアログが出る
    expect(await backend.requestWindowClose()).toBe("eventEmitted");
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await debuggerPage.confirmDialog.getByRole("button", { name: "取消", exact: true }).click();
    await expect(debuggerPage.confirmDialog).toBeHidden();
    expect(backend.calls("confirm_exit")).toEqual([]);
  });
});
