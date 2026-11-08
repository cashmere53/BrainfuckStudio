// スモーク 2(編集): 未保存の表示と Ctrl+Z(4.2)、括弧の一致と不一致の表示(4.3)、
// 日本語・絵文字を含む行の後ろの命令の現在命令(4.4)、CRLF を貼り付けた後の Step の現在命令(4.5)。
// 現在命令の検証は「装飾の文字がソースの pc 番目の命令である」で行う(pc は命令の番号。コメントの文字は数えない)。
import { readFileSync } from "node:fs";
import path from "node:path";
import { expect, test } from "../support/fixtures.ts";
import { REPO_ROOT } from "../support/paths.ts";

const HELLO_SOURCE = readFileSync(path.join(REPO_ROOT, "examples", "hello.bf"), "utf8").trimEnd();

/** Brainfuck の命令の文字(これ以外の文字はコメントとして読み飛ばされる) */
const INSTRUCTION_CHARS = new Set(["+", "-", "<", ">", "[", "]", ".", ","]);

/** ソースの命令の列(文字の配列。pc はこの列の添字) */
function instructionsOf(source: string): string[] {
  return Array.from(source).filter((ch) => INSTRUCTION_CHARS.has(ch));
}

/** 「未保存の変更」の時の文書の表示 */
const DIRTY_STATUS = "無題(未保存の変更あり)";

test.describe("スモーク 2: 編集", () => {
  test("4.2 入力と削除で未保存の変更の表示が出て、Ctrl+Z で入力前の内容に戻る", async ({
    backend,
    debuggerPage,
  }) => {
    const { page, documentStatus } = debuggerPage;
    await expect(documentStatus).toHaveText("無題");

    // 入力: 末尾に 1 文字入れると未保存の表示が出る。Ctrl+Z で入力前の内容に戻る
    await debuggerPage.editor.click();
    await page.keyboard.press("ControlOrMeta+End");
    await page.keyboard.type("+");
    await expect(documentStatus).toHaveText(DIRTY_STATUS);
    await expect.poll(() => backend.calls("set_document_dirty")[0]?.payload).toEqual({ args: { dirty: true } });
    await page.keyboard.press("ControlOrMeta+Z");
    await expect.poll(async () => (await debuggerPage.sourceText()).trimEnd()).toBe(HELLO_SOURCE);

    // 削除: 先頭の 1 文字を消すと未保存の表示が出る。Ctrl+Z で削除した文字が戻る
    await page.keyboard.press("ControlOrMeta+Home");
    await page.keyboard.press("Delete");
    await expect(documentStatus).toHaveText(DIRTY_STATUS);
    await expect.poll(async () => (await debuggerPage.sourceText()).trimEnd()).not.toBe(HELLO_SOURCE);
    await page.keyboard.press("ControlOrMeta+Z");
    await expect.poll(async () => (await debuggerPage.sourceText()).trimEnd()).toBe(HELLO_SOURCE);

    for (const call of backend.calls("set_document_dirty")) {
      expect(call.outcome).toEqual({ ok: true, value: null });
    }
  });

  // 製品の不具合の記録: 編集の取り消しで保存時点と同じ内容に戻っても、未保存の表示が残る
  // (setSource は編集のたびに dirty を立て、解除は保存(markSaved)だけで行うため)。
  // 上流 desktop-gui の要件 2.2 は「内容が最後に開いた/保存した内容と異なる間」だけ未保存を示すと定めている。
  test.fixme(
    "4.2 取り消しで保存時点と同じ内容に戻ったら、未保存の表示も消える",
    {
      annotation: {
        type: "issue",
        description:
          "製品の不具合: Ctrl+Z で保存時点と同じ内容に戻っても documentStatus が「無題(未保存の変更あり)」のまま。" +
          "原因の候補: apps/desktop/src/state/workspace.ts の setSource が dirty を無条件に true にする。" +
          "上流の要件: .kiro/specs/desktop-gui/requirements.md の 2.2。e2e 側の要件: .kiro/specs/desktop-gui-e2e-browser/requirements.md の 4.2。" +
          "追跡先: .kiro/specs/desktop-gui-e2e-browser/traceability.md の「見つけた製品の不具合」",
      },
    },
    async ({ debuggerPage }) => {
      const { page, documentStatus } = debuggerPage;
      await debuggerPage.editor.click();
      await page.keyboard.press("ControlOrMeta+End");
      await page.keyboard.type("+");
      await expect(documentStatus).toHaveText(DIRTY_STATUS);
      await page.keyboard.press("ControlOrMeta+Z");
      await expect(documentStatus).toHaveText("無題");
    },
  );

  test("4.3 カーソルが対応のある括弧に接すると、対応する括弧の両方が一致の強調になる", async ({ debuggerPage }) => {
    const { page } = debuggerPage;
    await debuggerPage.setSource("+[-]");
    await page.keyboard.press("ControlOrMeta+Home");
    // カーソルは「+」と「[」の間(「[」に接する)
    await page.keyboard.press("ArrowRight");

    await expect(debuggerPage.bracketMatched).toHaveText(["[", "]"]);
    await expect(debuggerPage.bracketUnmatched).toHaveCount(0);
  });

  test("4.3 対応の無い括弧は一致とは別の表示(別のクラス・太字)になる", async ({ debuggerPage }) => {
    const { page } = debuggerPage;
    await debuggerPage.setSource("+[+");
    await page.keyboard.press("ControlOrMeta+Home");
    // カーソルは「+」と「[」の間(対応する「]」が無い)
    await page.keyboard.press("ArrowRight");

    await expect(debuggerPage.bracketUnmatched).toHaveText("[");
    await expect(debuggerPage.bracketMatched).toHaveCount(0);
    await expect(debuggerPage.bracketUnmatched).toHaveCSS("font-weight", "700");
  });

  test("4.4 日本語・絵文字(サロゲートペア)を含む行の後ろの命令まで Step し、現在命令の文字がその命令になる", async ({
    debuggerPage,
  }) => {
    // 命令は + > + - の 4 個。1 行目は絵文字と日本語の後に命令が続き、2 行目は絵文字の後ろの命令
    const source = "日本語😀+😀>+\n🎉-";
    const instructions = instructionsOf(source);
    await debuggerPage.setSource(source);

    for (let pc = 1; pc < instructions.length; pc += 1) {
      await debuggerPage.button("Step").click();
      await expect.poll(() => debuggerPage.statusValue("pc")).toBe(pc);
      await expect(debuggerPage.currentInstruction).toHaveText(instructions[pc] ?? "");
    }
  });

  test("4.5 CRLF を含むテキストを貼り付けた後も、Step のたびに現在命令の文字がその命令になる", async ({
    debuggerPage,
  }) => {
    // 貼り付けたテキストは CRLF。命令は + > - + > の 5 個で、行をまたいだ後ろの命令まで Step する
    const pasted = "日本語😀+\r\n>-\r\n+😀>";
    const instructions = instructionsOf(pasted);
    await debuggerPage.setSource("");
    await debuggerPage.pasteIntoEditor(pasted);
    await expect.poll(async () => (await debuggerPage.sourceText()).includes("😀>")).toBe(true);

    for (let pc = 1; pc < instructions.length; pc += 1) {
      await debuggerPage.button("Step").click();
      await expect.poll(() => debuggerPage.statusValue("pc")).toBe(pc);
      await expect(debuggerPage.currentInstruction).toHaveText(instructions[pc] ?? "");
    }
  });
});
