// スモーク 1(起動): 画面を開いた直後の表示と、全経路(シム → 中継 → ブリッジ → 応答)の確認。
// 起動時の呼び出しは無い(load_source は Run の開始時、set_document_dirty は旗が変わったときだけ呼ばれる)ため、
// 全経路の確認は操作を起点に行う。
import { readFileSync } from "node:fs";
import path from "node:path";
import { expect, test } from "../support/fixtures.ts";
import { REPO_ROOT } from "../support/paths.ts";

const HELLO_SOURCE = readFileSync(path.join(REPO_ROOT, "examples", "hello.bf"), "utf8").trimEnd();

test.describe("スモーク 1: 起動", () => {
  test("文書のタイトル、Hello World と無題、未開始、Run・Step の有効と Pause・Reset の無効", async ({
    page,
    debuggerPage,
  }) => {
    await expect(page).toHaveTitle("Brainfuck Studio");
    expect((await debuggerPage.sourceText()).trimEnd()).toBe(HELLO_SOURCE);
    await expect(debuggerPage.documentStatus).toHaveText("無題");
    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    await expect(debuggerPage.button("Run")).toBeEnabled();
    await expect(debuggerPage.button("Step")).toBeEnabled();
    await expect(debuggerPage.button("Pause")).toBeDisabled();
    await expect(debuggerPage.button("Reset")).toBeDisabled();
  });

  test("全経路: 起動時の呼び出しは無く、1 文字の入力で set_document_dirty が記録されて応答が成功する", async ({
    backend,
    debuggerPage,
  }) => {
    expect(backend.calls()).toEqual([]);

    await debuggerPage.editor.click();
    await debuggerPage.page.keyboard.type("+");

    await expect
      .poll(() => backend.calls("set_document_dirty")[0]?.outcome ?? null)
      .toEqual({ ok: true, value: null });
    const calls = backend.calls("set_document_dirty");
    expect(calls).toHaveLength(1);
    expect(calls[0]?.payload).toEqual({ args: { dirty: true } });
  });
});

// フィクスチャ自身の約束の検査(記録・閉じる要求・後始末)。失敗にすべきテストは test.fail() で囲み、
// 後始末が失敗を出すことを確かめる(後始末が効かなくなると、囲んだテストが「失敗するはずが成功した」で落ちる)。
test.describe("フィクスチャの検査", () => {
  test("閉じる要求: 未保存でなければ確認なしで閉じた扱いになり、続く入力でも閉じる要求は記録されず確認は出ない", async ({
    backend,
    debuggerPage,
  }) => {
    expect(await backend.requestWindowClose()).toBe("closedWithoutConfirmation");

    await debuggerPage.editor.click();
    await debuggerPage.page.keyboard.type("+");
    await expect
      .poll(() => backend.calls("set_document_dirty")[0]?.outcome ?? null)
      .toEqual({ ok: true, value: null });

    expect(backend.calls().map((call) => call.cmd)).toEqual(["set_document_dirty"]);
    await expect(debuggerPage.confirmDialog).toBeHidden();
  });

  test("閉じる要求: 未保存なら閉じる要求のイベントを発火して確認を出し、制御コマンドは記録されない", async ({
    backend,
    debuggerPage,
  }) => {
    await debuggerPage.editor.click();
    await debuggerPage.page.keyboard.type("+");
    await expect
      .poll(() => backend.calls("set_document_dirty")[0]?.outcome ?? null)
      .toEqual({ ok: true, value: null });

    expect(await backend.requestWindowClose()).toBe("eventEmitted");

    await expect(debuggerPage.confirmDialog).toBeVisible();
    expect(backend.calls().map((call) => call.cmd)).toEqual(["set_document_dirty"]);
  });

  test("制御コマンドは中継されるが、呼び出しの記録には残らない", async ({ backend }) => {
    expect(await backend.forward("e2e:ping", null)).toEqual({ ok: true, value: "pong" });
    expect(backend.calls()).toEqual([]);
  });

  test("後始末: 使われなかったダイアログの指定は失敗になる", async ({ backend, debuggerPage }) => {
    test.fail();
    backend.dialogs.enqueue("open", { kind: "cancel" });
    await expect(debuggerPage.editor).toBeVisible();
  });

  test("後始末: 指定していないダイアログ(「開く」)は予期しない問い合わせとして失敗になる", async ({
    backend,
    debuggerPage,
  }) => {
    test.fail();
    await debuggerPage.button("開く").click();
    await expect.poll(() => backend.dialogs.unexpected.length).toBe(1);
  });
});
