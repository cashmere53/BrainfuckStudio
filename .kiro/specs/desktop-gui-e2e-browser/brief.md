# Brief: desktop-gui-e2e-browser

## Problem
desktop-gui の自動テストは vitest + jsdom(約 790 件)と Rust の統合テストだけで、実ブラウザで描画・操作するテストが無い。2026-11-07 の実機スモークでは、jsdom では見つからない不具合(エディタのフォーカス枠が見えない、ガターの背景色、幅 1040 px での右カラムの切れ)が出た。実機スモークは `orca computer` による一度きりの手作業で、繰り返せない。レイアウト・CSS・フォーカス・描画・キーボード操作・ブラウザの既定動作(リロード、右クリック)の回帰を、毎回の変更で自動検出したい。

## Current State
- `apps/desktop` は Tauri 2 + React 19 + Vite 8 + CodeMirror 6。フロントは `@tauri-apps/api/core` の `invoke` と `@tauri-apps/api/event` の `listen`(`close-requested` のみ)で Tauri に触れる(`src/ipc/backend.ts`)。環境判定や差し替えの入口は無く、`DebuggerScreen` の `backend` prop をテストだけが使う。
- `src/ipc/fakeBackend.ts` は応答を台本で与える方式で、Brainfuck を実行しない。
- 手動スモーク手順(`apps/desktop/README.md` の 1〜15)と、その実施結果(同 README「検証結果の記録」、`.kiro/specs/desktop-gui/traceability.md`)がある。未確認のまま残っている項目: ライトテーマ(14.1)、OS の高コントラスト(14.2)、NVDA(14.4)、1 秒以内の更新周期などの計測(9.2・9.3)。
- `data-testid` は `document-status` と `status-label` の 2 か所のみ。ただしボタン名(`Run (F5)` など)、`aria-label`(`操作`・`状態`・`テープ`・`出力`・`入力`・`ソース`・`通知`)、`role`(`toolbar`・`alertdialog`・`status`/`alert`)が一貫しており、ロケータはアクセシブル名で書ける。
- Playwright は未導入。CI(`.github/workflows/`)は存在しない。
- `vitest.config.ts` の `include` は `src/**/*.test.ts(x)`、`tsconfig.json` の `include` は `src` と設定ファイルのみ、`eslint.config.js` は `dist`・`node_modules`・`src-tauri` 以外を対象にする。`e2e/` を足す場合、typecheck・lint の範囲と、`test-results/`・`playwright-report/` の除外を整える必要がある。

## Desired Outcome
- `apps/desktop` で E2E 用のコマンド(例: `npm run e2e`)を実行すると、Playwright が Chromium でアプリ画面を開き、**実際の Rust バックエンド**(`brainfuck-gui-api` の `DebuggerService` とファイル入出力)を通して操作し、手動スモークのうち自動化できる項目が機械的に検証される。
- Linux の CI ランナー(GitHub Actions `ubuntu-latest`)で動き、WebView2 と Windows に依存しない。
- 手動スモーク 1〜15 の各項目について、「自動化済み(どのテスト)」「手動のまま(理由)」「対象外」が README と traceability で追跡できる。
- 製品コードの変更は最小限で、変更した箇所は spec に明記される。

## Approach
**A-1: 実 Rust を橋渡しする。**
1. `crates/brainfuck-gui-api` に、標準入出力で JSON-RPC 風のリクエストを受けて `DebuggerService` と `source_file` の関数を呼ぶ小さなバイナリ(`examples/` または `src/bin/`、テスト専用で配布しない)を追加する。リクエストごとにスレッドで処理し、`run_batch` 中の `pause` が実アプリと同じ経路で効くようにする。ネイティブダイアログに当たる `open_source_file` / `save_source_file` は、テストが事前に指定した「次に返すパス(またはキャンセル)」を使う。
2. Node 側(Playwright の `globalSetup` または `webServer`)に小さな HTTP 中継を置き、上記バイナリを起動して `invoke` 相当の要求を転送する。
3. ブラウザ側は `@tauri-apps/api/mocks` の `mockIPC`(`shouldMockEvents` 有効)で `window.__TAURI_INTERNALS__` を差し替え、`invoke` を中継へ `fetch` する。`close-requested` はモックのイベント機構で発火させる。差し込み口は design で決める: (a) `VITE_E2E` で切る製品側の入口、または (b) `mockIPC` 入りの小さなバンドルを `page.addInitScript` で注入して製品コードを変えない。
4. 画面は `vite preview`(ビルド済み)または `vite` で配信し、Playwright の `webServer` で起動する。

理由: TS での再実装(A-2)は debugger-state 相当のロジックの重複で、Rust と乖離すると「偽の合格」になる。台本のみ(A-3)は Run・Step・Pause の流れを扱えない。A-1 は実 Rust の意味論・DTO・エラー・ファイル処理をそのまま使い、Linux で動く。

## Scope
- **In**:
  - Playwright(`@playwright/test`)の導入、設定、ページオブジェクト、フィクスチャ。既存の `npm test`(vitest)とは別コマンドにし、typecheck・lint の対象範囲と出力ディレクトリの除外を整える。
  - Rust の stdio バイナリ、Node の中継、`mockIPC` の差し込み。
  - 手動スモークとの対応(番号は `apps/desktop/README.md`):
    - 1 起動と初期状態(文書タイトルの `Brainfuck Studio`、`hello.bf`、`未開始`、ボタンの有効・無効)
    - 2 編集(入力・削除・Ctrl+Z、括弧の対応強調、日本語・絵文字、CRLF の貼り付け)
    - 3.1 Hello World の実行(出力 `Hello World!`、pc 106・dp 6・実行数 906。実 Rust の値)
    - 4 上限なしの `+[]` の Pause と再開、警告の表示
    - 5 Step(pc の進行、ループ、終端で無効化、強調への追従とスクロール)
    - 6 Reset(初期状態へ、ブレークポイントと入力の保持、実行中は無効)
    - 7 ブレークポイント(ガタークリックと F9、停止、再開、入力欄での F9)
    - 8 入力と命令数上限(16 進とテキストの切替、不正な 16 進、不正な上限、上限での停止)
    - 9 応答性(実行数が増え続けること。「1 秒以内」の計測は閾値に余裕を持たせた `@perf` タグの別グループとし、CI では失敗を可視化するが必須にしない)
    - 10 保存・開く・名前を付けて保存(ダイアログは中継が返すパスで代替。キャンセル、成功時の表示、失敗時の通知、CRLF 保持、Shift_JIS の置換文字と通知、1 MiB 超の拒否、Ctrl+N / Ctrl+O の確認)
    - 11 未保存での閉じる確認(`close-requested` をモックのイベントで発火。確認ダイアログの表示・キャンセル・確定で `confirm_exit` が呼ばれること、二重表示しないこと、F5 連打・Ctrl+R・Ctrl+F5 でリロードされないこと、右クリックの既定メニューが抑止されること(`defaultPrevented`)、編集可能要素では抑止しないこと)
    - 12 レイアウト(viewport 800×600 と 1040×768 で、横方向の溢れが無いこと、ツールバーが折り返されること、主要領域に到達できること)
    - 13 キーボードのみの操作(Tab の順序、`:focus-visible` の枠がエディタを含む各要素で見えること、Enter・Space でボタンが動くこと)
    - 14.1・14.2・14.3 見た目(`page.emulateMedia` でライト・ダーク・`forced-colors` を切り替え、算出スタイルでコントラストと枠・波線・マーカーの描画を確認)
    - 15 実行エラー・解析エラーの表示と、その後の再実行
  - README の「検証結果の記録」に自動化の状況を追記し、`.kiro/specs/desktop-gui/traceability.md` は変更せず、本 spec の traceability で手動スモーク番号との対応を持つ。
- **Out**:
  - 実 WebView2・実ウィンドウの挙動(ウィンドウの最小サイズの強制、OS の閉じるボタン、ネイティブダイアログの見た目とモーダル性、Tauri の CSP の実効、WebView2 固有の右クリックメニュー)→ `desktop-gui-e2e-webview`
  - 3.2 CLI との比較(Rust の parity テストが担う)
  - 14.4 スクリーンリーダー(NVDA)の読み上げ
  - 基本 CI(cargo test / clippy / fmt / npm test のワークフロー)の新設(直接実装の候補。本 spec は E2E の job を追加する)
  - `compiler-comparison` の画面

## Boundary Candidates
- Rust 側の stdio バイナリ(`brainfuck-gui-api` の example/bin。Tauri 非依存、テスト専用)
- Node 側の中継と Playwright の基盤(設定、`webServer`、フィクスチャ、ページオブジェクト)
- ブラウザ側の IPC 差し込み(`mockIPC` の入口)
- シナリオ群(手動スモーク番号ごとのテストファイル)
- CI の job(`ubuntu-latest`、Playwright のブラウザキャッシュ、Rust のビルドキャッシュ)

## Out of Boundary
- desktop-gui の機能追加・仕様変更。製品コードは、IPC の差し込み口と `data-testid` の追加程度にとどめ、変更箇所を design に列挙する
- Rust の `DebuggerService` の API 変更(必要になれば desktop-gui への差し戻し)
- WebView2 に依存するテスト

## Upstream / Downstream
- **Upstream**: desktop-gui(画面と IPC の契約、`brainfuck-gui-api`、README の手動スモーク手順)、debugger-state、brainfuck-cli(`examples/hello.bf` の期待値)
- **Downstream**: desktop-gui-e2e-webview(Playwright の設定・ページオブジェクト・フィクスチャを再利用)、compiler-comparison(比較画面の E2E を同じ基盤に足せる)

## Existing Spec Touchpoints
- **Extends**: なし(desktop-gui は完了済みで変更しない。README の「検証結果の記録」への追記のみ)
- **Adjacent**: desktop-gui(`src/ipc/backend.ts`・`contract.test.ts` の契約)、compiler-comparison(将来の画面)

## Constraints
- Node 26(`mise exec node@26 -- npm ...`)、Rust stable、`unsafe` 禁止。Playwright は Apache-2.0。
- `npm test` に E2E を混ぜない。E2E は別コマンドで、既定ではブラウザを headless で動かす。
- `strictPort: true` の 1420 番と衝突しないよう、E2E の配信ポートと中継ポートは分ける(並列ワーカーごとに独立させる)。
- 時間を測る検証は flaky になりやすいため、`expect.poll` / 自動リトライの assertion を使い、閾値に余裕を持たせ、`@perf` タグで分離する。
- 開発 PC では新しい exe が Avast に止められて固まることがある(stdio バイナリも対象)。README に、除外設定または一時停止の必要を書く。
- 実機で未確認だった 14.1・14.2 はブラウザのエミュレーションで代替するが、OS 設定そのものの確認ではないことを traceability に明記する。
- Markdown 成果物は日本語。
