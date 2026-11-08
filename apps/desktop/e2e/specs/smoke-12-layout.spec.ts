// スモーク 12(レイアウト): 800×600 と 1040×768 で横方向のはみ出しが無く、各領域の右端が表示領域の内側にあり、
// スクロールで全ボタンと全領域に到達できること(9.1)、800×600 でツールバーのボタンが 2 行以上に折り返されること(9.2)、
// 1920×1080 で横方向のはみ出しが無く各領域が重ならないこと(9.3)を、ピクセルの位置ではなく矩形の関係で確かめる。
// 9.1 では、最小の 800×600 で、広い内容(長いファイル名・最大の表示半径・長い出力・400 文字の入力)を入れた状態も確かめる。
import path from "node:path";
import { mkdirSync } from "node:fs";
import type { Locator, Page, TestInfo } from "@playwright/test";
import type { DebuggerPage, ToolbarButton } from "../support/pages/DebuggerPage.ts";
import { expect, test } from "../support/fixtures.ts";

type Size = { readonly width: number; readonly height: number };

/** 位置と大きさ(表示領域の座標系) */
type Box = {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
};

/** 画面の領域(操作・ソース・入力・テープ・出力・状態)。ソースと入力は DebuggerPage の要素で代表させる */
type RegionName = "操作" | "ソース" | "入力" | "テープ" | "出力" | "状態";

/** ツールバーのボタン(全部。操作の 4 つと、ファイルと編集の 5 つ。この順に並ぶ) */
const TOOLBAR_BUTTONS: readonly ToolbarButton[] = [
  "Run",
  "Pause",
  "Step",
  "Reset",
  "新規",
  "開く",
  "保存",
  "名前を付けて保存",
  "全ブレークポイント解除",
];

const SIZE_640_480: Size = { width: 640, height: 480 };
const SIZE_800_600: Size = { width: 800, height: 600 };
const SIZE_1040_768: Size = { width: 1040, height: 768 };
const SIZE_1920_1080: Size = { width: 1920, height: 1080 };

/** 各領域を代表する要素。領域そのもの(区切りの枠)は DebuggerPage の公開の形だけで特定する */
function regionsOf(debuggerPage: DebuggerPage): Record<RegionName, Locator> {
  return {
    操作: debuggerPage.region("操作"),
    ソース: debuggerPage.editorFrame,
    入力: debuggerPage.inputBox,
    テープ: debuggerPage.region("テープ"),
    出力: debuggerPage.region("出力"),
    状態: debuggerPage.region("状態"),
  };
}

/** 要素の位置と大きさ。描画されていなければ失敗にする */
async function boxOf(locator: Locator): Promise<Box> {
  const box = await locator.boundingBox();
  if (box === null) {
    throw new Error("要素の位置を取得できません(描画されていない)");
  }
  return box;
}

/**
 * 行の折り返し。上端の位置の順に並べ、前の行の高さの範囲に上端が入らない要素から次の行とみなす
 * (中央そろえのずれを行の区切りにしない)。
 */
function splitRows(boxes: readonly Box[]): Box[][] {
  const sorted = [...boxes].sort((a, b) => a.y - b.y || a.x - b.x);
  const rows: Box[][] = [];
  let rowBottom = Number.NEGATIVE_INFINITY;
  for (const box of sorted) {
    if (box.y >= rowBottom) {
      rows.push([]);
      rowBottom = box.y + box.height;
    }
    rows[rows.length - 1]?.push(box);
  }
  return rows;
}

/** 2 つの矩形が重なるか(辺が接するだけなら重ならない) */
function overlaps(a: Box, b: Box): boolean {
  return (
    a.x < b.x + b.width &&
    b.x < a.x + a.width &&
    a.y < b.y + b.height &&
    b.y < a.y + a.height
  );
}

/** 横方向のはみ出し: 文書の幅と、画面を囲む表示域(.app-shell に相当する main の親)の内容の幅が表示域に収まること */
async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  const viewport = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    innerWidth: window.innerWidth,
  }));
  expect(viewport.scrollWidth, "文書の横方向の内容の幅").toBeLessThanOrEqual(
    viewport.innerWidth,
  );

  const shell = page.getByRole("main").locator("xpath=..");
  const shellOverflow = await shell.evaluate(
    (element) => element.scrollWidth - element.clientWidth,
  );
  expect(shellOverflow, "表示域の横方向のはみ出し(px)").toBeLessThanOrEqual(0);

  // main の右端も表示領域の内側にある(領域の右端の検査と合わせて、はみ出しを二重に確かめる)
  const main = await boxOf(page.getByRole("main"));
  expect(main.x + main.width, "main の右端").toBeLessThanOrEqual(
    viewport.innerWidth,
  );
}

/** 各領域の右端が表示領域の内側にあり、左端も外へ出ていないこと */
async function expectRegionsInsideViewport(
  debuggerPage: DebuggerPage,
  size: Size,
): Promise<void> {
  const regions = regionsOf(debuggerPage);
  for (const [name, locator] of Object.entries(regions) as [
    RegionName,
    Locator,
  ][]) {
    await expect(locator, `${name} の描画`).toBeVisible();
    const box = await boxOf(locator);
    expect(box.x, `${name} の左端`).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width, `${name} の右端`).toBeLessThanOrEqual(size.width);
  }

  // ツールバーのボタンも、右端が表示領域の内側にある(折り返しで外へ出ないこと)
  for (const name of TOOLBAR_BUTTONS) {
    const box = await boxOf(debuggerPage.button(name));
    expect(box.x + box.width, `ボタン「${name}」の右端`).toBeLessThanOrEqual(
      size.width,
    );
  }
}

/** テストごとの出力先(長いファイル名の保存に使う。無ければ作る) */
function outputDir(testInfo: TestInfo): string {
  const dir = testInfo.outputPath("files");
  mkdirSync(dir, { recursive: true });
  return dir;
}

test.describe("スモーク 12: レイアウト", () => {
  for (const size of [SIZE_800_600, SIZE_1040_768]) {
    test(`9.1 ${size.width}×${size.height}: 横方向のはみ出しが無く、各領域の右端が表示領域の内側にある`, async ({
      debuggerPage,
    }) => {
      await debuggerPage.page.setViewportSize(size);

      await expectNoHorizontalOverflow(debuggerPage.page);
      await expectRegionsInsideViewport(debuggerPage, size);
    });
  }

  // 変異の結果: 広い内容の検査は、折り返しを外す変異(出力の pre-wrap を pre にする)を検出する。
  // 「.debugger-screen > * の min-width: 0 を外す」変異は検出されない。各領域は .debugger-area(min-width: 0・max-width: 100%)で
  // 既に同じ制約を受けるため、等価な変異として扱う
  test("9.1 800×600: 広い内容(長いファイル名・最大の表示半径・長い出力・400 文字の入力)でも横方向のはみ出しが無く、各領域が表示領域の内側にある", async ({
    backend,
    debuggerPage,
  }, testInfo) => {
    const { page } = debuggerPage;
    await page.setViewportSize(SIZE_800_600);

    // 長いファイル名: 名前を付けて保存で、ファイル名の表示(文書の状態)を長くする
    const fileName = `${"a".repeat(80)}.bf`;
    backend.dialogs.enqueue("save", { kind: "path", path: path.join(outputDir(testInfo), fileName) });
    await debuggerPage.button("名前を付けて保存").click();
    await expect(debuggerPage.documentStatus).toHaveText(fileName);

    // 最大の表示半径(テープの窓が最も広い)
    await debuggerPage.radiusSelect.selectOption("64");
    // 400 文字の入力(折り返して入力欄の中に収まり、横へのスクロールを要らないこと)
    await debuggerPage.inputBox.fill("a".repeat(400));
    // 長い出力: 65 バイトを 1 行に続けて出す(値 10 の改行の前までの長い行が、折り返されることを確かめる)
    await debuggerPage.setSource("++++++++[>++++++++<-]>+[.-]");
    await debuggerPage.button("Run").click();
    await expect(debuggerPage.outputTotal).toHaveText("総バイト数: 65 バイト");

    await expectNoHorizontalOverflow(page);
    await expectRegionsInsideViewport(debuggerPage, SIZE_800_600);

    // 折り返しの検査: 出力の本文と入力欄は、横へのスクロールが無い(長い行は折り返される)
    const wrapped = await Promise.all(
      [debuggerPage.outputText, debuggerPage.inputBox].map((locator) =>
        locator.evaluate((element) => element.scrollWidth - element.clientWidth),
      ),
    );
    expect(wrapped, "出力の本文と入力欄の横方向のはみ出し(px)").toEqual([0, 0]);
  });

  test("9.1 640×480(最小未満): .app-shell がはみ出しをスクロールで見せる設定で、スクロールすると全ボタンと全領域に到達する", async ({
    debuggerPage,
  }) => {
    const { page } = debuggerPage;
    await page.setViewportSize(SIZE_640_480);
    const shell = page.getByRole("main").locator("xpath=..");

    // 最小未満なので、文書は表示域より大きく、スクロールで見る必要がある(判定の前提)。
    // 横方向の内容は .app-shell の最小幅(800px)で決まり、そのはみ出しは文書(ビューポート)のスクロールになる
    const documentWidth = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(documentWidth, "横方向にスクロールすべき内容があること").toBeGreaterThan(0);
    // .app-shell は内容を隠さず、スクロールで見せる設定であること(hidden・visible では末尾へ到達できない)
    const overflow = await shell.evaluate((element) => ({
      x: getComputedStyle(element).overflowX,
      y: getComputedStyle(element).overflowY,
    }));
    expect(["auto", "scroll"], "表示域の横方向のスクロール設定").toContain(overflow.x);
    expect(["auto", "scroll"], "表示域の縦方向のスクロール設定").toContain(overflow.y);

    // 右下の末端へ実際にスクロールする(文書と表示域のスクロール位置を動かし、末尾の領域が表示範囲に入ることを確かめる)
    await page.evaluate(() => window.scrollTo(document.documentElement.scrollWidth, document.documentElement.scrollHeight));
    await shell.evaluate((element) => {
      element.scrollLeft = element.scrollWidth;
      element.scrollTop = element.scrollHeight;
    });
    await expect(debuggerPage.region("状態"), "末端の領域(状態)の到達").toBeInViewport();

    // 1 つずつ表示範囲に入れてから、表示範囲に入ったことを確かめる
    for (const name of TOOLBAR_BUTTONS) {
      const button = debuggerPage.button(name);
      await button.scrollIntoViewIfNeeded();
      await expect(button, `ボタン「${name}」の到達`).toBeInViewport();
    }
    for (const [name, locator] of Object.entries(regionsOf(debuggerPage)) as [RegionName, Locator][]) {
      await locator.scrollIntoViewIfNeeded();
      await expect(locator, `${name} の到達`).toBeInViewport();
    }
  });

  test("9.2 800×600: ツールバーのボタンが 2 行以上に折り返される(操作の 4 つが 1 行目、ファイルと編集の 5 つが次の行以降)", async ({
    debuggerPage,
  }) => {
    await debuggerPage.page.setViewportSize(SIZE_800_600);
    const toolbar = debuggerPage.region("操作");
    await expect(toolbar).toBeVisible();

    const boxes: Box[] = [];
    for (const name of TOOLBAR_BUTTONS) {
      await expect(debuggerPage.button(name)).toBeVisible();
      boxes.push(await boxOf(debuggerPage.button(name)));
    }

    const rows = splitRows(boxes);
    expect(rows.length, "ツールバーのボタンの行数").toBeGreaterThanOrEqual(2);

    // 1 行目には操作の 4 つだけが入り、ファイルと編集の 5 つのまとまりは 1 行目の右端を超えるため次の行に回る。
    // 外側の折り返し(操作と編集のまとまりの並び)を外すと 1 行になり、この関係が崩れる。
    // 内側の折り返し(編集のまとまりの中の折り返し)を外す変異は、最小幅 800px ではまとまりが 1 行に収まるため、
    // 対象の幅の範囲では等価な変異として検出されない(まとまりが幅を超える幅は、最小幅の制約で現れない)
    const runRow = rows[0] ?? [];
    expect(runRow.length, "1 行目のボタンの数(操作の 4 つ)").toBe(4);
    const firstRowBottom = Math.max(...runRow.map((box) => box.y + box.height));
    const newButton = boxes[TOOLBAR_BUTTONS.indexOf("新規")];
    if (newButton === undefined) {
      throw new Error("「新規」のボタンの位置が無い");
    }
    expect(newButton.y, "「新規」は 1 行目の下の行に回る").toBeGreaterThanOrEqual(firstRowBottom);

    // 折り返した後も、どのボタンも操作の並びの右端(ツールバーの右端)の内側にある
    const toolbarBox = await boxOf(toolbar);
    for (const [index, box] of boxes.entries()) {
      expect(box.x + box.width, `ボタン「${TOOLBAR_BUTTONS[index]}」がツールバーの右端を超えない`).toBeLessThanOrEqual(
        toolbarBox.x + toolbarBox.width,
      );
    }
  });

  test("9.3 1920×1080: 横方向のはみ出しが無く、各領域が重ならずに表示される", async ({
    debuggerPage,
  }) => {
    await debuggerPage.page.setViewportSize(SIZE_1920_1080);

    await expectNoHorizontalOverflow(debuggerPage.page);
    await expectRegionsInsideViewport(debuggerPage, SIZE_1920_1080);

    const named: { name: RegionName; box: Box }[] = [];
    for (const [name, locator] of Object.entries(regionsOf(debuggerPage)) as [
      RegionName,
      Locator,
    ][]) {
      named.push({ name, box: await boxOf(locator) });
    }
    for (const [index, first] of named.entries()) {
      for (const second of named.slice(index + 1)) {
        expect(
          overlaps(first.box, second.box),
          `${first.name} と ${second.name} が重なっている`,
        ).toBe(false);
      }
    }
  });
});
