# Research & Design Decisions(desktop-gui-e2e-webview)

## Summary
- **Feature**: `desktop-gui-e2e-webview`
- **Discovery Scope**: Complex Integration(実アプリのプロセス・WebView2 の CDP・OS のウィンドウ操作を、上流 `desktop-gui-e2e-browser` の Playwright 基盤に足す。製品のソースは変えない)
- **Key Findings**:
  - Tauri 2 の実行ファイルは、Tauri CLI を通さずに `cargo build` すると開発モード(`devUrl` の `http://localhost:1420` を読む)になる。配布と同じ組み込みの画面と CSP で動かすには、`tauri build --debug --no-bundle`(`beforeBuildCommand` の `npm run build` を実行し、画面を実行ファイルへ組み込む debug ビルド)を使う
  - 閉じる要求は、OS のメッセージ `WM_SYSCOMMAND`/`SC_CLOSE` を実ウィンドウへ送れば、タイトルバーの × と同じ経路(`DefWindowProc` → `WM_CLOSE` → tao の `CloseRequested` → `window.rs` の `on_window_event`)を通る。製品にテスト専用のコマンドを足さずに済む
  - Playwright の WebView2 ガイドどおり、起動プロセスの環境変数 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` と `WEBVIEW2_USER_DATA_FOLDER` で、テストが起動したプロセスだけに CDP の口を開き、利用者データを分けられる。`tauri.conf.json` は変更しない

## Research Log

### 実アプリのビルドの形(開発モードと組み込みモード)
- **Context**: brief は「debug ビルドの実アプリ」とだけ書く。CSP の実効を確かめるには、配布と同じ配信経路が要る
- **Sources Consulted**: Tauri 2 公式文書(VS Code / RustRover のデバッグ設定: CLI を使わない `cargo build` は開発サーバを読む、`--no-default-features` の注記)、`tauri build --no-bundle`(配布の分割手順)、`apps/desktop/src-tauri/tauri.conf.json`(`devUrl: http://localhost:1420`、`frontendDist: ../dist`、`beforeBuildCommand: npm run build`)、`build.rs`(`frontendDist` が無ければ空のディレクトリを作る)
- **Findings**:
  - `cargo build` だけでは `devUrl` を読み、Vite の開発サーバ(1420)が必要になる。開発サーバの応答には Tauri の CSP が付かないため、CSP の検証が成り立たない
  - `tauri build --debug --no-bundle` は、`beforeBuildCommand`(製品の型検査 `typecheck:app` と `vite build`)を実行し、`dist/` を実行ファイルへ組み込んだ debug プロファイルのバイナリを `<target>/debug/brainfuck-studio.exe` に出す。インストーラは作らない
  - Windows の Tauri 2 は組み込みの画面を `http://tauri.localhost` から配信する(`tests/commands.rs` の IPC 要求の URL も同じ)
  - debug プロファイルでは `windows_subsystem` の属性が付かないためコンソールを持つ。標準出力・標準エラーをテスト側でパイプに受けられる
- **Implications**: 準備のコマンドは `tauri build --debug --no-bundle`。実行ファイルの位置は `CARGO_TARGET_DIR` を尊重する。`tauri build` の `--debug` と `--no-bundle` の組み合わせは実装の最初のタスクで実際に確かめる(Risks)

### WebView2 への CDP 接続
- **Context**: brief と Playwright 公式の WebView2 ガイド(検証済みの事実として与えられた)
- **Sources Consulted**: Playwright `docs/src/webview2.md`(TS / Python / Java / C# の例: 空きポートの選択、`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` と `WEBVIEW2_USER_DATA_FOLDER`、`connectOverCDP`、`browser.contexts()[0].pages()[0]`、`taskkill /pid <pid> /T /F`、「WebView2 は終了時に遅延した後始末をするため、利用者データをテストごとに消せないことがある」)
- **Findings**:
  - CDP の口は `127.0.0.1:<port>` に開く。`/json/version` が応答すれば接続できる。`Browser` の値(`Edg/<版>`)で WebView2 の版が分かる
  - 1 プロセス 1 ポート。並列に複数のアプリを起動するとポートと利用者データの競合が起きるため `workers: 1`
  - `connectOverCDP` は Playwright の独自プロトコルより低忠実度で、既定のコンテキストとページはアプリが作ったものを使う(`newContext` は使わない)。`addInitScript` は以後のナビゲーションにしか効かない
  - Tauri の `additionalBrowserArgs` を使う方式は、既定の利用者データのフォルダと引数の組み合わせが変わるため別のデータディレクトリが要る。環境変数ならアプリの設定を変えずに済む
  - 利用者データの削除は、WebView2 の遅延した後始末のため失敗することがある
  - Microsoft の `CreateCoreWebView2EnvironmentWithOptions` の文書によれば、`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` はアプリが `AdditionalBrowserArguments` で渡す引数に**追加**される(アプリの指定を置き換える環境変数は `WEBVIEW2_USER_DATA_FOLDER` だけ)。wry が渡す既定の引数(`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` など)は、環境変数を付けても残る
- **Implications**: シナリオごとに空きポートと一時フォルダを割り当てる。追加の引数の環境変数にはデバッグ口(`--remote-debugging-port=<port>`)だけを入れ、アプリの既定の引数を写さない(写すとスイッチが重複し、wry の既定と食い違ったときに配布形の構成を黙って上書きする)。追加であることは 1.5 でブラウザプロセスのコマンドラインを比べて確かめる。削除の失敗は警告の注釈にし、次回の実行の準備で古いフォルダを掃除する。接続後はアプリのページ(URL が `http://tauri.localhost` で始まる)を探して使う

### 閉じる要求の起こし方
- **Context**: brief は (a) `e2e` Cargo feature のテスト専用コマンド、(b) 対象外、の二択を design に委ねた。OS の閉じるボタンは CDP から押せない
- **Sources Consulted**: `src-tauri/src/window.rs`(`WindowEvent::CloseRequested` で未保存なら `prevent_close` と `close-requested` の送出)、`lib.rs`(`on_window_event`)、`commands/{mod,names}.rs`・`build.rs`・`capabilities/default.json`・`tests/commands.rs`(登録・権限生成・ケイパビリティの三者の一致、禁止権限の検査)、Win32 の既定のウィンドウプロシージャ(× ボタンと Alt+F4 は `WM_SYSCOMMAND`/`SC_CLOSE` になり、`DefWindowProc` が `WM_CLOSE` を送る)、tao の Windows 実装(`WM_CLOSE` を `CloseRequested` に写し、自分では破棄しない)
- **Findings**:
  - (a) の場合: コマンドを `cfg(feature = "e2e")` でだけ登録し、`build.rs` は `CARGO_FEATURE_E2E` のときだけ権限名を足し、権限は `capabilities/default.json` に入れずに feature の下でだけ実行時にケイパビリティを足す(`Manager::add_capability`)必要がある。`tests/commands.rs` の三者の一致は両方の構成で保たれるが、製品の `src-tauri` の 4 か所(`commands/mod.rs`、`names.rs` か `build.rs`、`lib.rs`、新しいコマンドのファイル)と `Cargo.toml` の `[features]` を変える。フロントの `contract.test.ts` は `names.rs` の一覧とフロントの `COMMANDS` を比べるため、`names.rs` には足せない
  - (a) のコマンドは `window.close()` を呼ぶので `CloseRequested` 以降は同じ経路だが、OS のメッセージ処理(× ボタンの経路)は通らない。呼び出しにはページ内の Tauri の内部 API が要る
  - OS の `SC_CLOSE` を送る方式(採用): × ボタンと同じ OS の経路を通り、製品の変更はゼロ。PowerShell から `user32` の `PostMessage` を呼べば足りる。ウィンドウはプロセスの主ウィンドウ(表示中で所有者の無いトップレベル)として取れる
  - (b) は brief の In(11.1〜11.3・11.5)を満たさない
- **Implications**: OS の `SC_CLOSE` を採用し、(a) は「OS のメッセージが CI で届かない場合」の切り替え先として設計の制約(上記)を記録する。切り替える場合は design の Modified Files の更新を先に行う

### 閉じる要求の前提(未保存の旗の伝達)と、重ねた要求の到達
- **Context**: フロントは編集で `set_document_dirty` を非同期に呼ぶ。Rust 側の旗が立つ前に閉じる要求が届くと、確認なしで閉じる
- **Sources Consulted**: `src/ipc/backend.ts`(`invokeCommand`)、Tauri 2 の Windows の IPC(`http://ipc.localhost/<コマンド名>` へのカスタムプロトコルの要求。`tests/commands.rs` の `InvokeRequest` の URL と同系)、`window.rs`(`close-requested` の送出)
- **Findings**:
  - IPC の要求は WebView2 のネットワーク要求として CDP から観測できる見込みだが、カスタムプロトコル(`http://ipc.localhost/...`)の要求が Playwright の `request` / `requestfinished` に現れることは未検証。`set_document_dirty` の応答を観測してから閉じる要求を送れば、伝達の遅れを判定に混ぜない
  - 切り替え先: `@tauri-apps/api` の `invoke` は呼び出しのたびに `window.__TAURI_INTERNALS__.invoke` を引く。接続後に `page.evaluate` でこれを包めば、以後の呼び出しと完了を製品の変更なしにページ内で記録できる(包む前の呼び出しは記録されない)
  - 起動時の IPC は `plugin:event|listen` だけで、CDP の接続より前に終わる。`load_source` は Run の開始の流れ(`src/state/runController.ts`)からだけ、`set_document_dirty` は未保存の旗が変わったときだけ(`src/app/documentFlow.ts`)呼ばれる。観測の確認は接続後の編集と Run の後で行う
  - 重ねた閉じる要求(11.5)は、画面に届いたかを外から確かめないと「二重に出ない」の検証が空振りする。ページ内で `close-requested` の購読をもう 1 つ足し(Tauri の内部 API `__TAURI_INTERNALS__` の `plugin:event|listen`。権限は既存の `core:event:default`)、受信回数を数える
- **Implications**: IPC の観測と閉じる要求の受信の計数を補助として持つ。IPC の観測はネットワーク要求の方式を既定とし、`invoke` の包み込みを最初から切り替え先として設計に置く。どちらを使うかは 2.3 で接続後の編集での `set_document_dirty` の観測によって確定し、結果をこの文書に記録する。どちらも Tauri の内部の形に依存するため、Tauri の更新を Revalidation Trigger にする。どちらの方式でも観測できない場合はスモーク 3.1 の IPC の検証と 11.x の同期が失敗として表に出る(偽の合格にはならない)

### リロードと右クリックの検証の入力経路
- **Context**: 11.6・11.7。ブラウザのスイートは `defaultPrevented` で検証しており、実際のリロードとメニューは本 spec に残された
- **Sources Consulted**: `src/app/shortcuts.ts`(ウィンドウの `keydown` の bubble で `preventDefault`。リロードのキーも抑止。`contextmenu` は編集可能でない対象だけ抑止)、README の 2026-11-07 の記録(F5・Ctrl+R は SendKeys で代用、押しっぱなしの自動リピートは再現できず連打で代用、エディタの右クリックでは WebView2 の編集メニューが出る)
- **Findings**:
  - WebView2 のリロードのアクセラレータはホスト側で処理される。CDP で注入したキー入力がその経路を通るかは保証されない。OS のキー入力(`SendKeys`)は実機の手動確認と同じ経路
  - 「リロードされなかった」だけでは、入力が届いていないのか抑止されたのか区別できない。アプリの抑止を外した状態(ウィンドウの capture で `stopImmediatePropagation`)で同じ入力を送り、リロードが起きること(陽性対照)を確かめる必要がある
  - 右クリックの既定メニューは、レンダラが `contextmenu` を抑止しなかったときにブラウザ側が出す。CDP のマウス入力はレンダラへの入力として同じ判断を通る。メニューの表示は CDP では見えないため、OS の UI Automation でアプリのプロセスツリーに属するメニューを探す。検出できることも陽性対照で確かめる
- **Implications**: キーは OS の入力、右クリックは CDP のマウス入力、メニューの有無は UI Automation で観測する。いずれも陽性対照を同じファイルに置く

### CSP の実効の確かめ方
- **Sources Consulted**: `tauri.conf.json` の CSP(`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:`)、`tests/commands.rs`(設定値の検査のみ)、DOM の `securitypolicyviolation` イベント
- **Findings**:
  - 設定値は Rust のテストで固定済み。実効(Tauri が配信時に CSP を付けているか)は実アプリでしか分からない
  - インラインのスクリプト、外部オリジンのスクリプト、外部オリジンへの `fetch` は、それぞれ `script-src-elem`(または `script-src`)と `connect-src`(`default-src` から継承)で拒否され、`securitypolicyviolation` が発火する
  - 「外部へ要求が届かない」ことは、テスト側で `127.0.0.1` に受け口のサーバを立てて受信数で確かめられる(ネットワークの失敗と区別できる)
  - 同じオリジンの `fetch('/')` が成功すること(陽性対照)で、拒否が CSP によることを示す
- **Implications**: 違反の記録と受け口のサーバを補助にし、4 つの検証を 1 ファイルに置く

### CI(Windows)
- **Findings**:
  - `windows-latest` の WebView2 ランタイムは版が明記されていない。Evergreen Bootstrapper(`MicrosoftEdgeWebview2Setup.exe /silent /install`)で導入し、レジストリの `pv` で版を記録する
  - Rust の冷えたビルドは 10 分を超えうる。`Swatinem/rust-cache` で `apps/desktop/src-tauri` の `target` をキャッシュする(独立ワークスペース)
  - Playwright のブラウザは不要(`connectOverCDP` は既存の WebView2 に接続する)
  - job 単位の `continue-on-error: true` で、失敗をワークフローの失敗にしない。必須チェックにも登録しない
- **Implications**: 独立したワークフロー `.github/workflows/e2e-webview.yml` に job `e2e-webview` だけを置く

### 上流(ブラウザのスイート)との接続点
- **Sources Consulted**: `.kiro/specs/desktop-gui-e2e-browser/design.md`・`tasks.md`
- **Findings**:
  - `playwright.config.ts` は設定の読み込み時に準備物(dist・シム・ブリッジ・Chromium)を確認し、`E2E_SKIP_PREREQ=1` で省ける。`webServer`(`vite preview`)はどのプロジェクトでも起動する
  - `projects` は `chromium` と `perf`。トップレベルの `testDir` は `e2e/specs`
  - `vitest.config.ts` の `include` は `e2e/support/**/*.test.ts`、`tsconfig.e2e.json`(`npm run typecheck` から呼ばれる)は `e2e` 全体、`eslint` も `e2e/**` を含む
  - `DebuggerPage` はブリッジやシムに依存せず `Page` を受け取る。`probes.ts`(`installDefaultActionProbe` / `readProbe`)と `visual/focusRing.ts`(`expectFocusRingDrawn`)も `Page` だけに依存する
  - `fixtures.ts` の `test` はブリッジとシムを組み立てるため、実アプリには使えない
  - `e2e:run` は `playwright test`(プロジェクト指定なし)で全プロジェクトを実行する
- **Implications**:
  - webview のプロジェクトは環境変数 `E2E_WEBVIEW=1` のときだけ設定に加える。ブラウザのスイートの `e2e:run` の対象と結果を変えない(要件 1.3)
  - webview の実行では `E2E_SKIP_PREREQ=1` を使い、ブラウザの準備物の確認を省き、webview 独自の確認は setup プロジェクトで行う
  - 補助のコードは `e2e/support/webview/` に置き、既存の `vitest` の `include` と型検査・lint の範囲にそのまま入る。ブラウザの spec の設定ファイル(`vitest.config.ts`、`tsconfig.e2e.json`、`eslint.config.js`、`.gitignore`)は変えない
  - ブラウザの spec への**必要な**変更は無い。任意の改善として、webview の実行時に `webServer` を起動しない分岐を提案する(`vite preview` は dist があれば無害に起動するため必須ではない)

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| Playwright の CDP 接続(採用) | 実アプリを環境変数つきで起動し `connectOverCDP` | ブラウザのスイートの部品をそのまま使える。Playwright 公式の手順 | 低忠実度、Windows 限定、並列不可 | roadmap の決定どおり |
| WebdriverIO `@wdio/tauri-service` | Tauri 公式推奨の組み込みドライバ | コマンドのモック、実ウィンドウの操作 | Playwright と別の実行系。部品を再利用できない | CDP で行き詰まった場合の切り替え先として記録 |
| `e2e` feature のテスト専用コマンドで閉じる | `e2e_request_close` で `window.close()` | OS 非依存 | 製品の `src-tauri` を 4〜5 か所変更、OS の経路を通らない | 閉じる要求の代替案として記録 |

## Design Decisions

### Decision: 閉じる要求は OS の `SC_CLOSE` で送る
- **Context**: 11.1〜11.3・11.5 を実ウィンドウで自動化する
- **Alternatives Considered**:
  1. `e2e` Cargo feature のテスト専用コマンド
  2. 対象外にして手動のまま
  3. OS の `WM_SYSCOMMAND`/`SC_CLOSE` を主ウィンドウへ送る
- **Selected Approach**: 3。PowerShell の補助スクリプトがプロセスの主ウィンドウを取り、`PostMessage(hwnd, WM_SYSCOMMAND, SC_CLOSE, 0)` を送る
- **Rationale**: × ボタンと同じ OS の経路を通る(忠実度が最も高い)。製品のソース・ケイパビリティ・ビルド設定を変えない(要件 2.2、2.4)。配布ビルドとテスト用ビルドの差が「CDP の環境変数」だけになる
- **Trade-offs**: Windows の API に依存する(本 spec は Windows 限定なので許容)。ウィンドウの特定に失敗した場合は理由つきで失敗する(要件 5.5)
- **Follow-up**: CI で `SC_CLOSE` が届かない場合は、代替案 1 へ切り替える。その際は `[features] e2e = []`、`cfg(feature = "e2e")` の登録、`build.rs` での `CARGO_FEATURE_E2E` による権限名の追加、実行時のケイパビリティの追加(`capabilities/default.json` には入れない)、`tests/commands.rs` と `contract.test.ts` が両方の構成で成功することを design の Modified Files に書いてから行う

### Decision: 実アプリは `tauri build --debug --no-bundle` で作る
- **Selected Approach**: npm スクリプト `e2e:webview:prepare` で実行。出力は `<CARGO_TARGET_DIR または src-tauri/target>/debug/brainfuck-studio.exe`。`E2E_APP_BIN` で上書きできる
- **Rationale**: 配布と同じ画面の組み込みと CSP。開発サーバ(1420)が要らない(要件 2.1、2.5)
- **Trade-offs**: 毎回 `npm run build` を含む。冷えたビルドは長い(CI はキャッシュ)

### Decision: webview のプロジェクトは環境変数で opt-in にする
- **Selected Approach**: `e2e/support/webview/projects.ts` の `webviewProjects(env)` が `E2E_WEBVIEW=1` のときだけ `webview-setup` と `webview` を返し、`playwright.config.ts` の `projects` の末尾に展開する。起動は小さなランチャ(`e2e/support/webview/cli.ts`)が環境変数を付けて Playwright を呼ぶ
- **Rationale**: ブラウザのスイートの `e2e:run`(全プロジェクト)と `e2e`(chromium)の対象を変えない。npm スクリプトの環境変数の書き方がシェルに依存しない
- **Trade-offs**: 設定ファイルに 1 行の展開を足す(ブラウザの spec が許す「プロジェクトの追加」の範囲)

### Decision: 入力の経路と陽性対照
- **Selected Approach**: リロードのキーは OS の入力(PowerShell の `AppActivate` と `SendKeys`)、右クリックは CDP のマウス入力、メニューの有無は UI Automation。11.6・11.7 と CSP にそれぞれ陽性対照を置く
- **Rationale**: 「起きなかった」ことの検証を、入力が届くことの証明と組にする(要件 6.2、6.4、7.4)

### Generalization / Build vs Adopt / Simplification
- **Generalization**: OS の操作(版・タイトル・閉じる・キー・メニュー)は 1 本の PowerShell スクリプトの「動作」の切り替えにまとめ、Node 側は 1 つの薄い窓口(`WindowsHost`)にする
- **Adopt**: Playwright の CDP 接続、ブラウザのスイートの `DebuggerPage`・`probes`・`focusRing`、PowerShell 標準の `WScript.Shell`・`System.Windows.Forms.SendKeys`・`System.Windows.Automation`。新しい npm 依存は追加しない
- **Simplification**: 実アプリはテストごとに起動する(状態の初期化の仕組みを作らない)。時間計測(`@perf`)のテストは置かない(ブラウザのスイートが担う)。`webServer` の分岐はブラウザの spec へ任意の提案にとどめる

## Risks & Mitigations
- `tauri build --debug --no-bundle` の出力が想定と違う(パス・組み込みモード) — 基盤のタスクで実行して確かめ、`paths.ts` の既定を合わせる。`E2E_APP_BIN` で回避できる
- 環境変数 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` が、文書(追加される)と違って wry の既定の追加引数(`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` など)を置き換える — 環境変数にはデバッグ口だけを入れ、通常の起動とテストの起動で WebView2 のブラウザプロセスのコマンドラインの重複を除いた実効のスイッチの集合を比べて、差が(`--user-data-dir` の値を除いて)`--remote-debugging-port` だけであることを 1.5 で確かめる。既定のメニューの表示先(`msWebOOUI`)が変わると 11.7 の前提が変わるため、差が出れば design を更新してから進める
- IPC の要求(`http://ipc.localhost/...`)が CDP から見えない — 2.3 で接続後の編集での `set_document_dirty` の観測を確かめ、見えなければ `__TAURI_INTERNALS__.invoke` の包み込みの方式へ切り替える(design の AppProbes に定義済み)。どちらでも観測できなければ design を見直す
- CI のランナーで `SendKeys`・UI Automation・`SC_CLOSE` が効かない(対話セッションの有無) — job は非必須。陽性対照と「届けられない」場合の理由つきの失敗で、偽の合格を防ぐ
- 開発 PC で新しい実行ファイルが Avast に止められて固まる — 起動確認の上限時間で失敗し、除外・一時停止・新しい `CARGO_TARGET_DIR`・再起動を案内する
- 利用者データのフォルダが WebView2 の遅延した後始末で消せない — 警告の注釈にし、次の実行の setup で古いフォルダを掃除する

## References
- Playwright: WebView2(`docs/src/webview2.md`)— 環境変数、空きポート、`connectOverCDP`、`taskkill /T /F`
- Microsoft: WebView2 `CreateCoreWebView2EnvironmentWithOptions`(`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` はアプリの追加引数に追加、`WEBVIEW2_USER_DATA_FOLDER` はアプリの指定を置き換える)
- Tauri 2: Debug in VS Code / RustRover(CLI を使わない `cargo build` は開発サーバを読む)、Distribute(`tauri build --no-bundle`)
- `apps/desktop/src-tauri/{tauri.conf.json, build.rs, src/window.rs, src/commands/*, capabilities/default.json, tests/commands.rs}`
- `.kiro/specs/desktop-gui-e2e-browser/{design.md, tasks.md, research.md}`
