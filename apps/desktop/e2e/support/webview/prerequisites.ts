// webview の E2E の準備物の判定(純粋関数)。値の import を持たず、存在の確認や OS への問い合わせは
// 呼び出し側が行い、結果を PrerequisiteFacts として渡す。1.3 のランチャが Node の型の除去で直接読み込むため、
// 消去可能な構文だけを使う。
export type PrerequisiteFacts = {
  platform: NodeJS.Platform;
  appBinaryExists: boolean;
  appBinaryPath: string;
  /** windowsHost.ps1 の runtime 動作が成功した(PowerShell が使える) */
  hostAvailable: boolean;
  /** 導入されていなければ null */
  webView2Version: string | null;
  /** 上流の DIST_INDEX_PATH(配信サーバ vite preview の起動に要る) */
  distExists: boolean;
};

/** 準備のコマンド(ビルドの作成。dist の作成も含む) */
const PREPARE_COMMAND = "npm run e2e:webview:prepare";

function windowsOnlyLine(platform: NodeJS.Platform): string {
  return `webview の E2E は Windows 限定です(実行中の OS: ${platform})。Windows の開発 PC で実行してください。`;
}

/**
 * ランチャが Playwright の起動より前に使う判定。Windows であることと dist の有無だけを見る。
 * 配信サーバ(vite preview)は dist が無いと先に失敗するため、その前に不足を案内する。
 */
export function missingLaunchPrerequisites(
  facts: Pick<PrerequisiteFacts, "platform" | "distExists">,
): string[] {
  if (facts.platform !== "win32") {
    return [windowsOnlyLine(facts.platform)];
  }
  if (!facts.distExists) {
    return [`画面のビルド成果物(apps/desktop/dist/index.html)が見つかりません。準備のコマンド ${PREPARE_COMMAND} を実行してください。`];
  }
  return [];
}

/**
 * 実アプリの E2E の準備物の不足の一覧(確認の順。空なら準備が整っている)。
 * Windows 以外は、他の判定が意味を持たないため Windows 限定の 1 行だけを返す。
 */
export function missingWebviewPrerequisites(facts: PrerequisiteFacts): string[] {
  if (facts.platform !== "win32") {
    return [windowsOnlyLine(facts.platform)];
  }
  const missing: string[] = [];
  if (!facts.appBinaryExists) {
    missing.push(
      `実アプリの実行ファイルが見つかりません: ${facts.appBinaryPath}。準備のコマンド ${PREPARE_COMMAND} で画面を組み込んだ debug ビルドを作成してください。`,
    );
  }
  if (!facts.hostAvailable) {
    missing.push(
      "PowerShell(powershell.exe)が使えません。Windows PowerShell 5.1 が使える環境で実行してください(OS の操作に使います)。",
    );
  }
  if (facts.webView2Version === null) {
    missing.push(
      "WebView2 ランタイムが見つかりません。Microsoft Edge WebView2 の Evergreen ランタイムを導入してから再実行してください(Evergreen Bootstrapper など)。",
    );
  }
  if (!facts.distExists) {
    missing.push(
      `画面のビルド成果物(apps/desktop/dist/index.html)が見つかりません。準備のコマンド ${PREPARE_COMMAND} を実行してください。`,
    );
  }
  return missing;
}
