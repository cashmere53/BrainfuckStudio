# 要件とテストの対応(desktop-gui-e2e-webview)

状況(2026-11-09): 実装完了(5.1 の最終検証の結果は `apps/desktop/README.md` の「検証結果の記録」)。手動スモークの番号は同 README の「手動スモーク手順」の 1〜15 を指す。シナリオは `apps/desktop/e2e/webview/*.webview.spec.ts` にあり、準備の確認は `apps/desktop/e2e/webview/prerequisites.setup.ts`。表の「テスト名」は各ファイルの `test(...)` のタイトルの文字列。ブラウザ(Chromium)での確認は `desktop-gui-e2e-browser` が担い、本 spec は実アプリ(Tauri 2 + WebView2 + Rust の IPC)に対する少数の重要な流れだけを自動化する。`desktop-gui` と `desktop-gui-e2e-browser` の traceability は変更しない。

## 要件の領域とテストファイル

| 領域 | 主な検証 |
|---|---|
| 準備の確認(debug ビルドの exe、WebView2 など) | `apps/desktop/e2e/webview/prerequisites.setup.ts`(`webview の準備物が揃っている`) |
| 起動・実行制御(スモーク 1、3.1、4、9.3) | `smoke-01-startup.webview.spec.ts`、`smoke-03-hello-world.webview.spec.ts`、`smoke-04-pause-resume.webview.spec.ts` |
| 実ウィンドウの閉じる確認(スモーク 11.1〜11.3、11.5) | `smoke-11-close.webview.spec.ts` |
| WebView2 の既定動作の抑止(スモーク 11.6、11.7) | `smoke-11-defaults.webview.spec.ts` |
| CSP の実効 | `csp.webview.spec.ts` |
| フォーカス枠(スモーク 13.3) | `smoke-13-focus-ring.webview.spec.ts` |
| Windows の CI | `.github/workflows/e2e-webview.yml`(非必須。備考 8) |

## 手動スモークの分類(本 spec の In と Out)

区分の凡例: **自動化済み** = 実アプリに対して自動化。**部分的** = 自動化した部分と手動の部分が両方ある。**手動のまま** = 自動化しない(理由を書く)。**対象外** = 別の spec が担う。`test.fixme` のテストは実行されないため、その検証は自動化済みに数えない(現時点で 0 件)。

| スモーク | 内容 | 区分 | テスト名(ファイル) / 理由・担当 |
|---|---|---|---|
| 準備 | 実行に必要な準備物の確認 | 自動化済み | `webview の準備物が揃っている`(`prerequisites.setup.ts`) |
| 1 | 起動と初期状態(実ウィンドウのタイトル、`hello.bf`、無題、未開始、ボタンの有効・無効) | 自動化済み | `スモーク 1 起動の表示: OS のタイトル、エディタの内容、無題、未開始、Run・Step の有効と Pause・Reset の無効`(`smoke-01-startup.webview.spec.ts`)。観測の補助(IPC の観測が機能することの確認): `スモーク 1 観測の補助: 閉じる要求の計数が設置でき、ナビゲーションの目印が残っていると読め、遷移は 0 回`、`スモーク 1 観測の補助: 接続後にエディタを編集すると set_document_dirty の要求と応答が観測される`(同ファイル) |
| 3.1 | Hello World の実行(実 IPC で pc 106・dp 6・実行数 906) | 自動化済み | `スモーク 3.1 Run で出力 Hello World!、正常終了、pc 106・dp 6・実行数 906 になり、load_source・start_session・run_batch の要求と完了が IPC で観測される`(`smoke-03-hello-world.webview.spec.ts`) |
| 4 | 上限なしの `+[]` の Run、Pause、再開、上限なしの警告 | 自動化済み | `上限なしの警告、実行中のボタンの有効・無効、実行数の増加、Pause での停止、Run での再開`(`smoke-04-pause-resume.webview.spec.ts`)。実 IPC の往復で Pause が効くことを確認 |
| 9.3 | 実行数の表示が 1 秒以内ごとに増える | 部分的 | 自動化: 実行中に実行数が増え続けること(実 IPC での更新)。4 と同じテスト `上限なしの警告、実行中のボタンの有効・無効、実行数の増加、Pause での停止、Run での再開`(`smoke-04-pause-resume.webview.spec.ts`)。1 秒の閾値はブラウザのスイートの `@perf`(`desktop-gui-e2e-browser` の `smoke-09-responsiveness.spec.ts`。閾値 1,500 ms)が担う。実アプリでの 1 秒の計測は手動のまま |
| 11.1 | 未保存で閉じる要求を送ると確認が 1 回だけ表示 | 自動化済み | `スモーク 11.1 未保存の変更で OS の閉じる要求を送ると、プロセスは終了せず、閉じる要求の受信が 1 回で確認ダイアログが 1 つ表示される`(`smoke-11-close.webview.spec.ts`)。閉じる要求の送り方は備考 1 |
| 11.2 | 「取消」で開いたまま、編集が残る | 自動化済み | `スモーク 11.2 確認ダイアログで「取消」を選ぶと、プロセスは動き続け、編集内容が残り、ウィンドウのタイトルが読め、続けて操作できる`(`smoke-11-close.webview.spec.ts`) |
| 11.3 | 「破棄して閉じる」で終了 | 自動化済み | `スモーク 11.3 確認ダイアログで「破棄して閉じる」を選ぶと、プロセスは終了コード 0 で終わる`(`smoke-11-close.webview.spec.ts`) |
| 11.5 | 確認ダイアログが二重に出ない | 自動化済み | `スモーク 11.5 確認ダイアログの表示中に閉じる要求を重ねても、閉じる要求の受信は 2 回になり、確認ダイアログは 1 つのまま`(`smoke-11-close.webview.spec.ts`) |
| 11.6 | F5 の連打・Ctrl+R・Ctrl+F5 でリロードされない | 自動化済み | `スモーク 11.6 F5 の連打・Ctrl+R・Ctrl+F5 で既定動作が抑止され、再読み込みされず編集内容が残る`、陽性対照 `スモーク 11.6 陽性対照: アプリの抑止を無効にすると、Ctrl+R で再読み込みされて目印が消える`(`smoke-11-defaults.webview.spec.ts`) |
| 11.7 | 編集可能でない要素の右クリックで既定のメニューが出ない | 自動化済み | `スモーク 11.7 編集可能でない要素の右クリックで、WebView2 の既定のコンテキストメニューが表示されない`、陽性対照 `スモーク 11.7 陽性対照: アプリの抑止を無効にすると、同じ右クリックで既定のコンテキストメニューが検出される`(`smoke-11-defaults.webview.spec.ts`)。検出方法は備考 4 |
| CSP | CSP の実効(インラインのスクリプトと外部への取得の拒否) | 自動化済み | `CSP 7.1 インラインのスクリプトは実行されず、script-src-elem の違反が inline として記録される`、`CSP 7.2 外部のオリジンのスクリプトは実行されず、受け口に要求が届かず、script-src-elem の違反が記録される`、`CSP 7.3 外部のオリジンへの取得は拒否され、受け口に要求が届かず、connect-src の違反が記録される`、陽性対照 `CSP 7.4 陽性対照: 同じオリジンの取得は成功して 200 を返し、connect-src の違反は記録されない`(`csp.webview.spec.ts`) |
| 13.3 | エディタのフォーカス枠が実 WebView2 で描画される | 自動化済み | `スモーク 13.3 Tab の移動でエディタにフォーカスが移り、実 WebView2 の描画で枠が描かれ隠れていない`、陰性対照 `スモーク 13.3 陰性対照: 枠の上を覆うと、枠の検証は失敗する(隠れた枠を検出する)`(`smoke-13-focus-ring.webview.spec.ts`)。OS の現在の配色(ライト)だけで確認(備考 2) |
| 10.1 ほか | ネイティブダイアログ(開く・保存・名前を付けて保存)の見た目とモーダル性 | 手動のまま | 実 OS のダイアログは CDP から操作も観測もできない。保存先の選択が出ることなどの挙動は、ブラウザのスイートが中継による代替で確認する |
| 12.1 | ウィンドウのサイズ変更と最小サイズの強制 | 手動のまま | CDP からはウィンドウ自体の大きさを変えられず、最小サイズの強制を確かめられない |
| 14.1・14.2 | OS のライトテーマ・高コントラストモードの切り替え | 手動のまま | OS の設定の切り替えは自動化しない(ブラウザのスイートが `emulateMedia` で代替。OS の設定そのものの確認ではない) |
| 14.4 | スクリーンリーダー(NVDA)の読み上げ | 手動のまま | 実際の読み上げは自動化できない |
| 11.4 | 保存済み・無変更なら確認なしで終了(実ウィンドウでの確認) | 手動のまま | 確認なしで閉じると実プロセスが終了し、閉じる要求の計数など終了後の観測ができない。確認ダイアログを出さずに閉じる扱いになることはブラウザのスイートが自動化済み |
| 2、3.2、5〜8 | 編集、CLI との比較、Step・Reset・ブレークポイント・入力と上限 | 対象外 | `desktop-gui-e2e-browser`(`smoke-02-editing`〜`smoke-08-input-limit`)。3.2 は `crates/brainfuck-gui-api/tests/cli_parity_real.rs` |
| 9.1・9.2 | 実行中の応答、Pause の後の追従 | 対象外 | `desktop-gui-e2e-browser`(`smoke-09-responsiveness.spec.ts`) |
| 10.2〜10.9 | ファイルの保存・開く・新規の確認 | 対象外 | `desktop-gui-e2e-browser`(`smoke-10-files-save.spec.ts`、`smoke-10-files-open.spec.ts`) |
| 12.2・12.3 | ツールバーの折り返し、大きい表示領域のレイアウト | 対象外 | `desktop-gui-e2e-browser`(`smoke-12-layout.spec.ts`) |
| 13.1・13.2・13.4 | Tab の順序、Enter・Space | 対象外 | `desktop-gui-e2e-browser`(`smoke-13-keyboard.spec.ts`) |
| 14.3 | 波線・枠・ブレークポイントのマーカー・括弧の一致の描画 | 対象外 | `desktop-gui-e2e-browser`(`smoke-14-appearance.spec.ts`) |
| 15 | エラーの表示と再実行 | 対象外 | `desktop-gui-e2e-browser`(`smoke-15-errors.spec.ts`) |
| 並列実行、Windows 以外の OS | 並列実行(1 アプリ 1 ポート)、WebKit 系 | 対象外 | 本 spec の範囲外(`workers: 1`、Windows 限定) |

件数(全 26 行): 自動化済み 12(準備、1、3.1、4、11.1、11.2、11.3、11.5、11.6、11.7、CSP、13.3)、部分的 1(9.3)、手動のまま 5(10.1 ほか、12.1、14.1・14.2、14.4、11.4)、対象外 8(2・3.2・5〜8、9.1・9.2、10.2〜10.9、12.2・12.3、13.1・13.2・13.4、14.3、15、並列・他 OS)。`test.fixme` は 0 件。

## 静的確認・部分的・手動となる項目

| 項目 | 状況 | 確認方法 |
|---|---|---|
| 9.3 | 部分的 | webview は実行数が増え続けることだけを自動化。1 秒の閾値はブラウザのスイートの `@perf` が担い、実アプリでの 1 秒の計測は手動 |
| 11.4 | 手動 | 実ウィンドウでの確認。ブラウザのスイートが確認なしで閉じる扱いを自動化済み |
| ネイティブダイアログ(10.1 ほか) | 手動 | 見た目とモーダル性は実 OS のダイアログに依存する |
| 12.1 | 手動 | ウィンドウのサイズ変更と最小サイズの強制 |
| 14.1・14.2 | 手動 | OS のライト・高コントラストの設定の切り替え |
| 14.4 | 手動 | NVDA |
| 閉じる要求(11.1〜11.3、11.5) | 代替 | 閉じるボタンの物理的なクリックではなく OS の閉じる要求で代替(備考 1) |
| 13.3 | 部分的 | OS の現在の配色(ライト)だけで確認。ダーク・高コントラストは対象外(備考 2) |

## 見つけた製品の不具合・`test.fixme`

`test.fixme` にした検証は現時点でなし。E2E で製品の不具合が見つかった場合は製品を直さず、該当テストを `test.fixme` にして注釈に現象と追跡先を書き、ここに追記する。製品の副次的な事実(機能に影響しない CSP の `console.warn`)は備考 3。

## 備考

1. **閉じる要求の代替**: 閉じる要求は、閉じるボタンの物理的なクリックではなく、OS の閉じる要求(`WM_SYSCOMMAND/SC_CLOSE` を主ウィンドウへ PostMessage)で代替した。タイトルバーの × と同じ OS の経路を通る。
2. **13.3 の配色**: OS の現在の配色(ライト)だけで確認し、配色のエミュレーションは使っていない。WebView2 では編集可能な要素に `:focus-visible` が `focus()` でも付くため、フォーカスの経路は Tab で検証する。
3. **IPC の観測**: 製品の CSP に `connect-src 'self' ipc: http://ipc.localhost` を足したため(追加前は `connect-src` 未指定で `fetch` 方式の IPC `http://ipc.localhost/<cmd>` が拒否され、起動のたびに `console.warn` を出して `window.chrome.webview.postMessage` に切り替わっていた)、追加後は Tauri が想定する `fetch` 方式で流れる。観測は `window.fetch` の `ipc.localhost` 宛ての記録が主で、`chrome.webview.postMessage` の記録は(フォールバック経路の検出としての)保険として残している。応答は `__TAURI_INTERNALS__.runCallback` で返る。`network` 方式と `invoke` 方式の観測は実アプリで成立しなかった(`research.md` に記録)。
4. **WebView2 の既定のコンテキストメニューの検出**: 旧実装は UIA のルート直下の `Menu` を探していて常に偽で、11.7 の否定が何があっても合格する状態だった。陽性対照(`スモーク 11.7 陽性対照: ...`)がこれを見つけた。現在は Win32 の `EnumWindows` で `Chrome_WidgetWin_*` のポップアップを列挙し、その UIA の部分木から `Menu` を探す。
5. **前面化**: Windows の前面化ロックで `AppActivate` が効かず、`keys` が失敗した。復元と `AttachThreadInput` による `SetForegroundWindow` を足した。入力を送らずに前面化を試し、前面にならなければ何も送らず失敗とし、送信の直前直後に前面を再確認する。実行中はキーボードとマウスを操作しないこと。
6. **引数の比較の実測**: テストの起動のブラウザプロセスのコマンドラインは、アプリの既定の引数(`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` など)が残ったまま `--remote-debugging-port=<port>` が追加され、`--user-data-dir` はシナリオごとの一時フォルダ(`scenario-<uuid>/EBWebView`)を指す。差は `--remote-debugging-port` と `--user-data-dir` の値だけ(起動ごとに変わる mojo の pipe 名を除く)。
7. **`tauri build` の副作用**: `tauri build` は `apps/desktop/src-tauri/Cargo.toml` を書き換える(feature の同期)ことがあるため、準備の後に差分が出たら戻す。
8. **CI**: webview の job(`.github/workflows/e2e-webview.yml`)は非必須(`continue-on-error`)で、GitHub 上の実行は PR の初回で確認する。
