// スモーク 10(開く): CRLF の改行コードが保存後も保たれること(7.5)、不正な UTF-8 の置換文字と通知(7.6)、
// 1 MiB 超の拒否(7.7)、未保存の変更がある間の新規・開くの確認(7.8)。
// ファイルの内容はバイト列で比べる(改行コードの検証のため)。開くの保存先はダイアログを使わず、テストの待ち行列で決める。
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import type { TestInfo } from "@playwright/test";
import { writeCrlfSource, writeOversizedSource, writeShiftJisSource } from "../support/files.ts";
import { expect, test } from "../support/fixtures.ts";

/** テストごとの出力先(ファイルを作る場所。無ければ作る) */
function outputDir(testInfo: TestInfo): string {
  const dir = testInfo.outputPath("files");
  mkdirSync(dir, { recursive: true });
  return dir;
}

test.describe("スモーク 10: 開く", () => {
  test("7.5 CRLF のファイルを開いて保存すると、ファイルの改行コードは CRLF のまま保たれる", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const source = writeCrlfSource(outputDir(testInfo));
    backend.dialogs.enqueue("open", { kind: "path", path: source });

    await debuggerPage.page.keyboard.press("ControlOrMeta+O");
    await expect(debuggerPage.documentStatus).toHaveText("crlf.bf");

    await debuggerPage.page.keyboard.press("ControlOrMeta+S");

    await expect
      .poll(() => backend.calls("save_source_file")[0]?.outcome ?? null)
      .toEqual({ ok: true, value: { kind: "saved", fileName: "crlf.bf" } });
    expect(readFileSync(source).equals(Buffer.from("+\r\n+\r\n.", "latin1"))).toBe(true);
    await expect(debuggerPage.documentStatus).toHaveText("crlf.bf");
  });

  test("7.6 不正な UTF-8 のバイト列を含むファイルを開くと、不正なバイトは置換文字で表示され、「通知」の通知が出る", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const source = writeShiftJisSource(outputDir(testInfo));
    backend.dialogs.enqueue("open", { kind: "path", path: source });

    await debuggerPage.page.keyboard.press("ControlOrMeta+O");

    await expect(debuggerPage.notices).toContainText("ファイルに不正な UTF-8 のバイト列が含まれていたため");
    await expect(debuggerPage.notices).toContainText("通知");
    await expect(debuggerPage.notices).not.toContainText("操作エラー");
    await expect(debuggerPage.documentStatus).toHaveText("shift-jis.bf");
    expect(await debuggerPage.sourceText()).toBe("�e�X�g\n+.");
  });

  test("7.7 1 MiB を超えるファイルは読み込みが拒否されて上限超過のエラーが出て、編集内容は変わらない", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const oversized = writeOversizedSource(outputDir(testInfo));
    await debuggerPage.setSource("+.");
    await expect(debuggerPage.documentStatus).toHaveText("無題(未保存の変更あり)");

    backend.dialogs.enqueue("open", { kind: "path", path: oversized });
    await debuggerPage.page.keyboard.press("ControlOrMeta+O");
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await debuggerPage.confirmDialog.getByRole("button", { name: "破棄して開く", exact: true }).click();

    await expect(debuggerPage.notices).toContainText("操作エラー");
    await expect(debuggerPage.notices).toContainText("ファイルが上限(1 MiB)を超えています");
    expect(backend.calls("open_source_file")[0]?.outcome).toMatchObject({ ok: false });
    expect(await debuggerPage.sourceText()).toBe("+.");
    await expect(debuggerPage.documentStatus).toHaveText("無題(未保存の変更あり)");
  });

  test("7.8 未保存の変更がある間の Ctrl+N は確認を出し、取消で編集が残り、確定で新規文書になる", async ({
    backend,
    debuggerPage,
  }) => {
    await debuggerPage.setSource("+.");
    await expect(debuggerPage.documentStatus).toHaveText("無題(未保存の変更あり)");

    await debuggerPage.page.keyboard.press("ControlOrMeta+N");
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await expect(debuggerPage.confirmDialog).toContainText("未保存の変更があります");

    await debuggerPage.confirmDialog.getByRole("button", { name: "取消", exact: true }).click();
    await expect(debuggerPage.confirmDialog).toBeHidden();
    expect(backend.calls("new_document")).toEqual([]);
    expect(await debuggerPage.sourceText()).toBe("+.");
    await expect(debuggerPage.documentStatus).toHaveText("無題(未保存の変更あり)");

    await debuggerPage.page.keyboard.press("ControlOrMeta+N");
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await debuggerPage.confirmDialog.getByRole("button", { name: "破棄して新規", exact: true }).click();

    await expect(debuggerPage.documentStatus).toHaveText("無題");
    expect(await debuggerPage.sourceText()).toBe("");
    expect(backend.calls("new_document")[0]?.outcome).toEqual({ ok: true, value: null });
  });

  test("7.8 未保存の変更がある間の Ctrl+O は確認を出し、取消では開く操作も編集も行われず、確定で開く操作へ進む", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const other = path.join(outputDir(testInfo), "other.bf");
    writeFileSync(other, "++.");
    await debuggerPage.setSource("+.");
    await expect(debuggerPage.documentStatus).toHaveText("無題(未保存の変更あり)");

    await debuggerPage.page.keyboard.press("ControlOrMeta+O");
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await debuggerPage.confirmDialog.getByRole("button", { name: "取消", exact: true }).click();
    await expect(debuggerPage.confirmDialog).toBeHidden();
    expect(backend.calls("open_source_file")).toEqual([]);
    expect(await debuggerPage.sourceText()).toBe("+.");
    await expect(debuggerPage.documentStatus).toHaveText("無題(未保存の変更あり)");

    backend.dialogs.enqueue("open", { kind: "path", path: other });
    await debuggerPage.page.keyboard.press("ControlOrMeta+O");
    await expect(debuggerPage.confirmDialog).toBeVisible();
    await debuggerPage.confirmDialog.getByRole("button", { name: "破棄して開く", exact: true }).click();

    await expect(debuggerPage.documentStatus).toHaveText("other.bf");
    expect(await debuggerPage.sourceText()).toBe("++.");
    expect(backend.calls("open_source_file")).toHaveLength(1);
  });
});
