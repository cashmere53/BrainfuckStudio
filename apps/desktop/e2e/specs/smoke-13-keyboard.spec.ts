// スモーク 13(キーボード): Tab・Shift+Tab で全操作領域とボタンに到達し、移動の順序が期待する順序と画面の視覚的な順序に一致すること(9.4)、
// ライトとダークの両方で、エディタを含む各要素にフォーカスが移ったときフォーカスの枠が実際に描画されること(9.5)、
// フォーカスした Run・Pause・Step・Reset が Enter と Space で動くこと(9.6)。
// 操作ボタンは状態で有効が変わる: 未開始・一時停止では Run・Step・Reset が有効、実行中は Pause だけが有効。
// そのため Pause と Reset が同時に有効になる状態は無く、Pause は実行中、Reset は一時停止の状態で確かめる。
// 期待する順序は画面の操作の並び(ツールバー、ソース、入力、命令数上限、テープ、出力)を明示する。
// 視覚的な順序は、各要素の画面上の位置(上から下、同じ行の中では左から右)と照らし合わせる。
import type { Locator, Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";
import type { DebuggerPage, ToolbarButton } from "../support/pages/DebuggerPage.ts";
import { expectFocusRingDrawn } from "../support/visual/focusRing.ts";

/** Tab で止まる要素の 1 つ。target はフォーカスを確かめる要素、ring は枠を描く要素(エディタは枠を外側の .cm-editor が描く) */
type Stop = { readonly name: string; readonly target: Locator; readonly ring: Locator };

type Box = { readonly x: number; readonly y: number; readonly width: number; readonly height: number };

/** 操作の状態。未開始(初期)・実行中・一時停止 */
type State = "initial" | "running" | "paused";

/**
 * 状態ごとに Tab で止まる要素の期待する順序(無効の要素は止まらない)。
 * 未開始: Run・Step が有効、編集の操作は全部有効。
 * 一時停止: Run・Step・Reset と編集の操作が有効。上限なしを入れた状態のため命令数上限の入力欄は無効。
 * 実行中: Pause だけが操作ボタンで有効。編集の操作は全部無効で、表示半径・テープ・出力の表示形式だけが止まる。
 * 出力の本文(pre)は出力が無いため無く、表示形式の選択だけが止まる。
 * テープのセルの一覧(ol)は横にはみ出すスクロール領域のため、ブラウザの既定でキーボードで止まる(表示半径の次)。
 */
function expectedStops(page: Page, debuggerPage: DebuggerPage, state: State): Stop[] {
  const toolbar = (name: ToolbarButton): Stop => {
    const locator = debuggerPage.button(name);
    return { name, target: locator, ring: locator };
  };
  const self = (name: string, locator: Locator): Stop => ({ name, target: locator, ring: locator });
  const controls: Stop[] =
    state === "initial"
      ? [toolbar("Run"), toolbar("Step")]
      : state === "paused"
        ? [toolbar("Run"), toolbar("Step"), toolbar("Reset")]
        : [toolbar("Pause")];
  const editing: Stop[] =
    state === "running"
      ? []
      : [
          toolbar("新規"),
          toolbar("開く"),
          toolbar("保存"),
          toolbar("名前を付けて保存"),
          toolbar("全ブレークポイント解除"),
          { name: "ソース", target: debuggerPage.editor, ring: debuggerPage.editorFrame },
          self("入力の形式(テキスト)", debuggerPage.inputMode("テキスト")),
          self("入力", debuggerPage.inputBox),
        ];
  // 上限なしを入れた状態(一時停止)では命令数上限の入力欄は無効のため、上限なしだけが止まる
  const limits: Stop[] =
    state === "running"
      ? []
      : state === "paused"
        ? [self("上限なし", debuggerPage.unlimited)]
        : [self("命令数上限", debuggerPage.stepLimit), self("上限なし", debuggerPage.unlimited)];
  return [
    ...controls,
    ...editing,
    ...limits,
    self("表示半径", debuggerPage.radiusSelect),
    self("テープのセル(横に並ぶスクロール領域)", debuggerPage.region("テープ").getByRole("list")),
    self("出力の表示形式(テキスト)", debuggerPage.outputMode("テキスト")),
  ];
}

/** 停止する要素の先頭(空の一覧は失敗にする) */
function firstStop(stops: readonly Stop[]): Stop {
  const first = stops[0];
  if (first === undefined) {
    throw new Error("Tab で止まる要素の一覧が空");
  }
  return first;
}

/** 実行を開始する(上限なしの無限ループ)。実行中は Pause だけが有効になる */
async function startRunning(page: Page, debuggerPage: DebuggerPage): Promise<void> {
  await debuggerPage.setSource("+[]");
  await debuggerPage.unlimited.check();
  await debuggerPage.button("Run").focus();
  await page.keyboard.press("Enter");
  await expect(debuggerPage.statusLabel).toHaveText("実行中");
}

/** 一時停止の状態にする。Run・Step・Reset が有効になる(Pause は無効) */
async function startPaused(page: Page, debuggerPage: DebuggerPage): Promise<void> {
  await startRunning(page, debuggerPage);
  await debuggerPage.button("Pause").focus();
  await page.keyboard.press("Enter");
  await expect(debuggerPage.statusLabel).toHaveText("一時停止");
}

/** 状態の準備。初期は何もしない */
async function prepare(page: Page, debuggerPage: DebuggerPage, state: State): Promise<void> {
  if (state === "running") {
    await startRunning(page, debuggerPage);
  } else if (state === "paused") {
    await startPaused(page, debuggerPage);
  }
}

/**
 * 文書の先頭から Tab を押して各要素に順に移り、一つずつフォーカスを確かめる(途中に余分な停止があれば次の確認で落ちる)。
 * 先頭の要素へも Tab で移る(focus() で置くと、Tab で到達できない不具合を見逃すため)。
 */
/**
 * フォーカスを外し、Tab の始点を文書の先頭にする。フォーカスしていた要素が無効になった後は、ブラウザの Tab の始点が
 * その要素の位置に残るため(一時停止の Pause の後など)、文書の左上を押して始点を先頭へ戻す。
 */
async function blurActive(page: Page): Promise<void> {
  await page.evaluate(() => {
    const active = document.activeElement;
    if (active instanceof HTMLElement) {
      active.blur();
    }
  });
  await page.locator("body").click({ position: { x: 0, y: 0 } });
}

async function tabForward(page: Page, stops: readonly Stop[]): Promise<void> {
  await blurActive(page);
  for (const stop of stops) {
    await page.keyboard.press("Tab");
    await expect(stop.target, `Tab で「${stop.name}」にフォーカスが移る`).toBeFocused();
  }
}

/** Shift+Tab で、最後の要素から最初の要素へ逆順に移る(開始時点で最後の要素にフォーカスがあること) */
async function tabBackward(page: Page, stops: readonly Stop[]): Promise<void> {
  for (const stop of [...stops].reverse().slice(1)) {
    await page.keyboard.press("Shift+Tab");
    await expect(stop.target, `Shift+Tab で「${stop.name}」にフォーカスが移る`).toBeFocused();
  }
}

/**
 * 画面上の順序の確認。隣り合う 2 要素について、同じ行(縦の重なりが小さい方の高さの半分未満)なら右側、
 * 別の行なら下側にあることを求める。
 */
function assertVisualOrder(stops: readonly Stop[], boxes: readonly Box[]): void {
  for (let index = 0; index + 1 < stops.length; index += 1) {
    const current = stops[index];
    const next = stops[index + 1];
    const before = boxes[index];
    const after = boxes[index + 1];
    if (current === undefined || next === undefined || before === undefined || after === undefined) {
      throw new Error(`順序の確認の対象が欠けています(位置 ${index})`);
    }
    const sameRow = Math.abs(before.y - after.y) < Math.min(before.height, after.height) / 2;
    const inOrder = sameRow ? after.x > before.x : after.y > before.y;
    expect(inOrder, `「${current.name}」→「${next.name}」は画面の上から下、左から右の順序に一致しない`).toBe(true);
  }
}

async function boxesOf(stops: readonly Stop[]): Promise<Box[]> {
  const boxes: Box[] = [];
  for (const stop of stops) {
    const box = await stop.target.boundingBox();
    if (box === null) {
      throw new Error(`「${stop.name}」の位置を取得できません`);
    }
    boxes.push(box);
  }
  return boxes;
}

const STATE_LABEL: Record<State, string> = { initial: "未開始", running: "実行中", paused: "一時停止" };

test.describe("スモーク 13: キーボード", () => {
  for (const state of ["initial", "running", "paused"] as const) {
    test(`9.4 ${STATE_LABEL[state]}: Tab で全操作領域とボタンに期待する順序で到達し、順序が画面の上から下・左から右に一致する`, async ({
      page,
      debuggerPage,
    }) => {
      await prepare(page, debuggerPage, state);
      const stops = expectedStops(page, debuggerPage, state);
      // 位置は Tab の移動より前に測る(移動によるスクロールで位置がずれないように)
      await expect(firstStop(stops).target).toBeVisible();
      assertVisualOrder(stops, await boxesOf(stops));

      await tabForward(page, stops);
    });

    test(`9.4 ${STATE_LABEL[state]}: Shift+Tab は Tab の逆順に、同じ要素へ戻る`, async ({ page, debuggerPage }) => {
      await prepare(page, debuggerPage, state);
      const stops = expectedStops(page, debuggerPage, state);
      await tabForward(page, stops);
      await tabBackward(page, stops);
      await expect(firstStop(stops).target, "逆順の最後は最初の操作ボタン").toBeFocused();
    });
  }
});

for (const scheme of ["light", "dark"] as const) {
  test.describe(`スモーク 13: フォーカス枠(${scheme})`, () => {
    test.use({ colorScheme: scheme });

    for (const state of ["initial", "running", "paused"] as const) {
      test(`9.5 ${scheme} の配色・${STATE_LABEL[state]}: Tab で移動した各要素の枠が 4 辺とも描画される`, async ({
        page,
        debuggerPage,
      }) => {
        // 配色が指定どおりに効いていることを先に確かめる(枠の描画の検証は配色に依存する)
        const dark = await page.evaluate(() => window.matchMedia("(prefers-color-scheme: dark)").matches);
        expect(dark).toBe(scheme === "dark");

        await prepare(page, debuggerPage, state);
        const stops = expectedStops(page, debuggerPage, state);
        await blurActive(page);
        for (const stop of stops) {
          // 枠の検査の直前に Tab で移る(:focus-visible はキーボード由来のフォーカスでだけ枠を描く)。
          // フォーカスの確認は移動の直後に行う(枠の検査は終わるとブラーするため、後では確かめられない)
          const focus = async (): Promise<void> => {
            await page.keyboard.press("Tab");
            await expect(stop.target, `「${stop.name}」にフォーカスがある`).toBeFocused();
          };
          await expectFocusRingDrawn(page, { focus, ring: stop.ring });
        }
      });
    }
  });
}

test.describe("スモーク 13: 操作ボタンの Enter と Space", () => {
  test("9.6 フォーカスした Run・Pause・Step・Reset が Enter と Space で動く", async ({ page, backend, debuggerPage }) => {
    // Pause を押せるように、上限なしの無限ループを入れて実行を続けさせる
    await debuggerPage.setSource("+[]");
    await debuggerPage.unlimited.check();

    // Run を Enter で開始する
    await debuggerPage.button("Run").focus();
    await expect(debuggerPage.button("Run")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(0);

    // Pause を Space で止める
    await debuggerPage.button("Pause").focus();
    await expect(debuggerPage.button("Pause")).toBeFocused();
    await page.keyboard.press("Space");
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");
    await expect.poll(() => backend.calls("pause")[0]?.outcome?.ok ?? null).toBe(true);
    const stopped = await debuggerPage.statusValue("実行数");
    expect(await debuggerPage.statusValue("実行数")).toBe(stopped);

    // Run を Space で再開する
    await debuggerPage.button("Run").focus();
    await expect(debuggerPage.button("Run")).toBeFocused();
    await page.keyboard.press("Space");
    await expect(debuggerPage.statusLabel).toHaveText("実行中");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBeGreaterThan(stopped);

    // もう一度 Pause を Enter で止める
    await debuggerPage.button("Pause").focus();
    await expect(debuggerPage.button("Pause")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");

    // Step を Enter と Space で、それぞれ命令を 1 つ進める
    const beforeEnter = await debuggerPage.statusValue("実行数");
    await debuggerPage.button("Step").focus();
    await expect(debuggerPage.button("Step")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(beforeEnter + 1);

    await debuggerPage.button("Step").focus();
    await expect(debuggerPage.button("Step")).toBeFocused();
    await page.keyboard.press("Space");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(beforeEnter + 2);

    // Reset を Enter で押すと、未開始に戻り実行数が 0 になる
    await debuggerPage.button("Reset").focus();
    await expect(debuggerPage.button("Reset")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(0);
    await expect(debuggerPage.button("Run")).toBeEnabled();

    // Reset を Space でも押せることを、もう一度 Step で命令を進めてから確かめる
    await debuggerPage.button("Step").focus();
    await expect(debuggerPage.button("Step")).toBeFocused();
    await page.keyboard.press("Space");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(1);
    await debuggerPage.button("Reset").focus();
    await expect(debuggerPage.button("Reset")).toBeFocused();
    await page.keyboard.press("Space");
    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(0);
  });

  test("9.6 Tab で操作ボタンに到達し、Enter で動く(実行中の Pause と一時停止の Reset)", async ({ page, debuggerPage }) => {
    // 実行中は Pause だけが有効なので、文書の先頭から Tab を押すと Pause に到達する(Run・Step は無効で止まらない)
    await startRunning(page, debuggerPage);
    await page.evaluate(() => {
      const active = document.activeElement;
      if (active instanceof HTMLElement) {
        active.blur();
      }
    });
    await page.keyboard.press("Tab");
    await expect(debuggerPage.button("Pause"), "実行中は Tab で Pause に到達する").toBeFocused();
    await page.keyboard.press("Enter");
    await expect(debuggerPage.statusLabel).toHaveText("一時停止");

    // 一時停止の状態では、Tab で Run・Step・Reset に順に到達し、Reset を Enter で押すと未開始に戻る
    await debuggerPage.button("Run").focus();
    await expect(debuggerPage.button("Run")).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(debuggerPage.button("Step"), "Tab で Step に到達する").toBeFocused();
    await page.keyboard.press("Tab");
    await expect(debuggerPage.button("Reset"), "Tab で Reset に到達する").toBeFocused();
    await page.keyboard.press("Enter");
    await expect(debuggerPage.statusLabel).toHaveText("未開始");
    await expect.poll(() => debuggerPage.statusValue("実行数")).toBe(0);
  });
});
