# Brief: desktop-gui-e2e-webview

## Problem
`desktop-gui-e2e-browser` は Chromium と IPC の差し替えで画面を検証するが、実際の配布形(Tauri 2 + WebView2 + Rust の IPC)で起きること(実ウィンドウの閉じる要求、WebView2 の既定の右クリックメニューとリロード、CSP の実効、実 IPC の往復時間、実ウィンドウの最小サイズ)は確認できない。これらは 2026-11-07 に `orca computer` で一度だけ手動確認したが、繰り返せない。少数の重要な流れを、実アプリに対して自動で通したい。

## Current State
- 実アプリは `npm run tauri dev`(または debug ビルドの exe)で起動し、WebView2 で画面を出す。
- Playwright の公式ガイド(WebView2)どおり、起動プロセスの環境変数 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` と、`chromium.connectOverCDP` で接続できる。Tauri 2 での実例が複数ある。`WEBVIEW2_USER_DATA_FOLDER` で利用者データのフォルダを分けられる。
- 制約: Windows 限定(WebKit 系に CDP は無い)、同時に 1 アプリ 1 ポート(並列不可)、`connectOverCDP` は低忠実度、ランナーの WebView2 の版は制御できない、ネイティブダイアログ(開く・保存)は操作できない、OS の閉じるボタンは CDP から押せない。
- 開発 PC では新しい exe が Avast に止められて固まることがある(除外設定か一時停止が要る)。
- 閉じる確認は Rust の `CloseRequested` → `close-requested` イベント → アプリ内ダイアログ → `confirm_exit`(`app.exit(0)`)の流れ。

## Desired Outcome
- Windows で `npm run e2e:webview`(名称は design で決める)を実行すると、debug ビルドの実アプリが起動し、Playwright が CDP で接続して、起動・Hello World・Pause と再開・閉じる確認・WebView2 の既定動作の抑止・CSP の実効を自動で検証する。
- `desktop-gui-e2e-browser` の Playwright 設定・ページオブジェクト・フィクスチャを再利用し、重複を持たない。
- CI では Windows の job として実行するが、初期は「失敗しても必須にしない(`continue-on-error`、または手動トリガー)」とし、安定してから必須に昇格する。ローカルでの実行手順(Avast の扱い、WebView2 の版、ポート)を README に書く。
- 自動化できない項目(ネイティブダイアログ、OS の閉じるボタン、NVDA)は、手動のまま README と traceability に明記する。

## Approach
- Playwright のプロジェクト(`webview`)を追加し、`globalSetup` で debug ビルドの exe を環境変数付きで起動、`http://localhost:<port>/json/version` を待ってから `connectOverCDP` する。`workers: 1`、テストごとに利用者データのフォルダを一時ディレクトリに切る。teardown で確実にプロセスを終了し、フォルダを削除する。
- OS の閉じるボタンは押せないため、閉じる要求の自動化は design で選ぶ: (a) `e2e` Cargo feature で有効化するテスト専用コマンド(`request_close` など。配布ビルドには含めない)、(b) 対象外にして手動のまま。
- ネイティブダイアログ(開く・保存)は対象外にし、`desktop-gui-e2e-browser` の中継による代替に委ねる。
- 代替案として WebdriverIO の `@wdio/tauri-service`(公式推奨、組み込みドライバ、コマンドのモック)があるが、Playwright と別の実行系になるため、本 spec では採らない。CDP で行き詰まった場合の切り替え先として design に記録する。

## Scope
- **In**(手動スモーク番号は `apps/desktop/README.md`):
  - 1 起動と初期状態(実ウィンドウのタイトル、`hello.bf`、`未開始`、ボタンの有効・無効)
  - 3.1 Hello World の実行(実 IPC 経由で pc 106・dp 6・実行数 906)
  - 4 上限なしの `+[]` の Pause と再開(実 IPC の往復で Pause が効くこと)
  - 9.3 実行数が増え続けること(実 IPC での更新)
  - 11.1〜11.3・11.5 閉じる確認(閉じる要求の起こし方は design で決める)
  - 11.6 F5 連打・Ctrl+R・Ctrl+F5 でリロードされないこと(WebView2 の実挙動)
  - 11.7 右クリックで既定メニューが出ないこと(非編集領域)
  - CSP の実効(インラインスクリプトや外部取得が拒否されること)
  - 13.3 エディタのフォーカス枠が実 WebView2 で描画されること
  - CI の Windows job(初期は非必須)、ローカル実行手順
- **Out**:
  - ネイティブダイアログ(開く・保存・名前を付けて保存)の操作と見た目
  - ウィンドウのサイズ変更・最小サイズの強制(CDP からは行えない)
  - OS のライトテーマ・高コントラストモードの切り替え、NVDA
  - 並列実行、Windows 以外の OS
  - `desktop-gui-e2e-browser` が担う項目の重複実装

## Boundary Candidates
- アプリの起動と CDP 接続の基盤(環境変数、ポート、利用者データ、待機、終了)
- テスト専用の Rust 側の口(`e2e` feature。採る場合)
- シナリオ群(上記の In)
- CI の Windows job

## Out of Boundary
- desktop-gui の機能変更
- `desktop-gui-e2e-browser` の基盤の変更(必要なら同 spec に差し戻す)
- WebdriverIO への移行

## Upstream / Downstream
- **Upstream**: desktop-gui-e2e-browser(Playwright の基盤)、desktop-gui(アプリ、`window.rs` の閉じる要求、`tauri.conf.json` の CSP・ウィンドウ設定)
- **Downstream**: compiler-comparison(実アプリでの比較画面の確認を同じ基盤に足せる)

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: desktop-gui-e2e-browser(同じ Playwright の設定を共有。プロジェクト名で分ける)

## Constraints
- Windows 11 / WebView2 Evergreen。GitHub Actions の `windows-latest` には WebView2 ランタイムが明記されておらず版が古いことがあるため、job で Evergreen Bootstrapper か `choco install webview2-runtime` を実行する。
- Rust の debug ビルドを使い、`Swatinem/rust-cache` などでキャッシュする(冷えた状態で 10 分超になりうる)。
- `workers: 1`、ポートと利用者データのフォルダはテストごとに独立させる。
- 配布ビルドにテスト専用コマンドを含めない(`e2e` feature を採る場合は capability にも含めない)。
- Avast の影響(新しい exe が固まる)を README に明記し、固まった場合の対処(除外設定、再起動)を書く。
- Markdown 成果物は日本語。
