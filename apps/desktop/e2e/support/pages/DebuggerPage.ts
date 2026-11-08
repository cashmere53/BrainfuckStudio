// デバッガ画面のページオブジェクト。要素はロールとアクセシブルな名前で特定し、
// CodeMirror の描画とガター・装飾のクラス名は、このファイルの中だけで使う(シナリオに書かない)。
// ブリッジとシムには依存しない(下流の webview の spec が再利用できるように)。
import type { Locator, Page } from "@playwright/test";

/** ツールバーのボタン(アクセシブルな名前の前方一致。ショートカットの文字は含めてよい) */
export type ToolbarButton =
  | "Run"
  | "Pause"
  | "Step"
  | "Reset"
  | "新規"
  | "開く"
  | "保存"
  | "名前を付けて保存"
  | "全ブレークポイント解除";

/** 画面の領域(ツールバーは toolbar、通知は aria-label だけを持つ要素) */
export type Region = "操作" | "状態" | "テープ" | "出力" | "通知";

/** 状態領域の項目名(dt)。値は続く dd から読む */
export type StatusTerm = "pc" | "dp" | "現在のセル" | "実行数" | "入力の読み取り済み" | "入力の総数";

/** 入力・出力の表示形式 */
export type ViewMode = "テキスト" | "16 進";

/** 正規表現の特殊文字をエスケープする(前方一致の正規表現を組み立てるため) */
function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export class DebuggerPage {
  readonly page: Page;

  /** role=textbox、名前 "ソース"(CodeMirror の contentDOM) */
  readonly editor: Locator;
  /** フォーカス枠を描く要素(フォーカス枠の検証に使う) */
  readonly editorFrame: Locator;
  /** 状態ラベル(aria-live の領域。data-testid で特定する) */
  readonly statusLabel: Locator;
  /** 文書の表示(ファイル名・未保存の変更。data-testid で特定する) */
  readonly documentStatus: Locator;
  /** 出力のテキスト表示 */
  readonly outputText: Locator;
  /** 入力欄(textarea、名前 "入力") */
  readonly inputBox: Locator;
  /** 命令数上限の入力欄 */
  readonly stepLimit: Locator;
  /** 上限なしのチェックボックス */
  readonly unlimited: Locator;
  /** 上限なしの警告(role=status の文言で特定する。上限なしでないときは存在しない) */
  readonly stepLimitWarning: Locator;
  /** 確認ダイアログ(role=alertdialog) */
  readonly confirmDialog: Locator;
  /** 操作エラーの表示(ErrorBanner。アクセシブルな名前を持たないため、クラス名で特定する) */
  readonly errorBanner: Locator;
  /** 通知の領域(名前 "通知") */
  readonly notices: Locator;
  /** ブレークポイントのマーカー */
  readonly breakpointMarkers: Locator;
  /** 現在命令の強調 */
  readonly currentInstruction: Locator;
  /** 解析エラーの強調 */
  readonly parseErrors: Locator;
  /** 対応する括弧の強調(一致) */
  readonly bracketMatched: Locator;
  /** 対応する括弧の強調(不一致) */
  readonly bracketUnmatched: Locator;
  /** エラーの見出し(「解析エラー」「実行エラー」。ErrorBanner の見出しの段落) */
  readonly errorHeading: Locator;
  /** 実行エラーの位置の強調 */
  readonly runtimeErrors: Locator;
  /** ブレークポイントの命令文字の強調(text-decoration と枠で示す) */
  readonly breakpointChars: Locator;
  /** 出力の総バイト数の表示(「総バイト数: N バイト」。出力の領域の中) */
  readonly outputTotal: Locator;

  constructor(page: Page) {
    this.page = page;
    this.editor = page.getByRole("textbox", { name: "ソース", exact: true });
    this.editorFrame = page.locator(".cm-editor");
    this.statusLabel = page.getByTestId("status-label");
    this.documentStatus = page.getByTestId("document-status");
    this.outputText = page.getByLabel("出力の内容(テキスト)", { exact: true });
    this.inputBox = page.getByRole("textbox", { name: "入力", exact: true });
    this.stepLimit = page.getByRole("textbox", { name: "命令数上限", exact: true });
    this.unlimited = page.getByRole("checkbox", { name: "上限なし", exact: true });
    this.stepLimitWarning = page.getByRole("status").filter({ hasText: "上限なしで実行します" });
    this.confirmDialog = page.getByRole("alertdialog");
    this.errorBanner = page.locator(".error-banner");
    this.notices = page.getByLabel("通知", { exact: true });
    this.breakpointMarkers = page.locator(".cm-breakpoint-marker");
    this.currentInstruction = page.locator(".cm-execution-current");
    this.parseErrors = page.locator(".cm-parse-error");
    this.bracketMatched = page.locator(".cm-bracket-matched");
    this.bracketUnmatched = page.locator(".cm-bracket-unmatched");
    this.errorHeading = this.errorBanner.locator(".error-banner-heading");
    this.runtimeErrors = page.locator(".cm-execution-runtime-error");
    this.breakpointChars = page.locator(".cm-breakpoint-char");
    this.outputTotal = this.region("出力").getByText(/^総バイト数: \d+ バイト$/);
  }

  /** 状態領域の項目名(dt)。完全一致で探す。補助ラベルの色の検証に使う */
  statusTerm(term: StatusTerm): Locator {
    return this.region("状態").locator(`xpath=.//dt[normalize-space(.)=${JSON.stringify(term)}]`);
  }

  /** 状態ラベルが指定の文言になるまで待つ(描画フレームごとに確かめる。Playwright の既定の間隔に依らない) */
  async waitForStatusLabel(label: string): Promise<void> {
    await this.page.waitForFunction(
      (expected) => document.querySelector('[data-testid="status-label"]')?.textContent === expected,
      label,
    );
  }

  /** 画面を開き、エディタの表示を待つ(配信サーバのルートを開く) */
  async goto(): Promise<void> {
    await this.page.goto("/");
    await this.editor.waitFor();
  }

  /** ツールバーのボタン。名前の前方一致(例: "Run" は "Run (F5)" に一致する) */
  button(name: ToolbarButton): Locator {
    return this.page.getByRole("button", { name: new RegExp(`^${escapeRegExp(name)}`) });
  }

  /** 画面の領域 */
  region(name: Region): Locator {
    switch (name) {
      case "操作":
        return this.page.getByRole("toolbar", { name, exact: true });
      case "通知":
        return this.page.getByLabel(name, { exact: true });
      default:
        return this.page.getByRole("region", { name, exact: true });
    }
  }

  /** 入力の形式の切替(入力の形式のラジオボタン。出力の表示形式と区別する) */
  inputMode(mode: ViewMode): Locator {
    return this.page
      .getByRole("radiogroup", { name: "入力の形式", exact: true })
      .getByRole("radio", { name: mode, exact: true });
  }

  /** エディタの全文を置き換える(全選択の後に文字を挿入する。空文字なら削除する) */
  async setSource(text: string): Promise<void> {
    await this.editor.focus();
    await this.page.keyboard.press("ControlOrMeta+A");
    if (text === "") {
      await this.page.keyboard.press("Delete");
      return;
    }
    await this.page.keyboard.insertText(text);
  }

  /** クリップボードのデータを持つ paste イベントを送る(CRLF を含む貼り付けの確認に使う) */
  async pasteIntoEditor(text: string): Promise<void> {
    await this.editor.focus();
    await this.editor.evaluate((element, data) => {
      const transfer = new DataTransfer();
      transfer.setData("text/plain", data);
      element.dispatchEvent(new ClipboardEvent("paste", { clipboardData: transfer, bubbles: true, cancelable: true }));
    }, text);
  }

  /** エディタの現在のソース(行の区切りは LF)。CodeMirror の行の要素から組み立てる */
  async sourceText(): Promise<string> {
    return this.editor.evaluate((element) =>
      Array.from(element.querySelectorAll(".cm-line"), (line) => line.textContent ?? "").join("\n"),
    );
  }

  /**
   * 状態領域の値(10 進の整数)。「状態」領域の dt(項目名)に続く dd から読む。
   * 項目名は完全一致で探す(「実行数」と「最終実行数」を区別するため)。
   */
  async statusValue(term: StatusTerm): Promise<number> {
    const value = this.region("状態").locator(
      `xpath=.//dt[normalize-space(.)=${JSON.stringify(term)}]/following-sibling::dd[1]`,
    );
    const text = (await value.textContent())?.trim() ?? "";
    if (!/^-?\d+$/.test(text)) {
      throw new Error(`状態の項目「${term}」の値が整数ではありません: "${text}"`);
    }
    return Number(text);
  }

  /**
   * 行番号のガターの行(1 始まりの行番号)。ブレークポイントのガターは行の要素を持つ行にだけ丸印を描くため、
   * 行の位置はこの行番号の要素で特定し、丸印の有無は `hasBreakpointAt` で確かめる。
   * 行番号の幅の測定用の要素(非表示)より後ろにある実際の行を返す。
   */
  gutterLine(line: number): Locator {
    return this.page
      .locator(".cm-lineNumbers .cm-gutterElement")
      .filter({ hasText: new RegExp(`^${line}$`) })
      .last();
  }

  /** 行のブレークポイントのガターの列の中央をクリックして丸印を切り替える(ガターのマウス操作) */
  async clickBreakpointGutter(line: number): Promise<void> {
    const row = await this.gutterLine(line).boundingBox();
    const column = await this.page.locator(".cm-breakpoint-gutter").boundingBox();
    if (row === null || column === null) {
      throw new Error(`行 ${line} のガターの位置を取得できません`);
    }
    await this.page.mouse.click(column.x + column.width / 2, row.y + row.height / 2);
  }

  /** 行に丸印(ブレークポイントのマーカー)があるか。丸印の中心が行の高さに入るかで判定する */
  async hasBreakpointAt(line: number): Promise<boolean> {
    const row = await this.gutterLine(line).boundingBox();
    if (row === null) {
      return false;
    }
    for (const marker of await this.breakpointMarkers.all()) {
      const box = await marker.boundingBox();
      if (box !== null && box.y + box.height / 2 >= row.y && box.y + box.height / 2 <= row.y + row.height) {
        return true;
      }
    }
    return false;
  }
}
