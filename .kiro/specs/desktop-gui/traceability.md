# 要件とテストの対応(desktop-gui)

最終検証の状況(2026-11-07): 自動検証は GO 相当。実ウィンドウでの手動スモークは、1〜15 の実施可能な項目を 2026-11-07 に実機で確認して合格(結果は README)。ライトテーマ・高コントラスト・NVDA(14.1・14.2・14.4)と、1 秒以内の更新周期などの計測は未確認のため **MANUAL_VERIFY_REQUIRED**(一部)。自動テストは `crates/brainfuck-gui-api/tests/`、`apps/desktop/src-tauri/tests/`、`apps/desktop/src/**/*.test.ts(x)` にあり、いずれもウィンドウを起動せずに成功する(結果は `apps/desktop/README.md` の「検証結果の記録」)。手動スモーク番号は同 README の「手動スモーク手順」の 1〜15 を指す。

## 要件の領域とテストファイル

| 領域 | 主な検証 |
|---|---|
| 画面の構成・起動時の状態(1) | `src/app/App.test.tsx`、`src/app/DebuggerScreen.test.tsx`、`src/samples/hello.test.ts`、`src/state/debuggerStore.test.ts`(初期状態と未開始の表示) |
| 文書の編集・開く/保存・閉じる確認(2) | `src/app/documentFlow.test.ts`、`src/app/documentOperations.test.tsx`、`src/components/ConfirmDialog.test.tsx`、`src/components/SourceEditor.test.tsx`、`src/state/workspace.test.ts`、`crates/brainfuck-gui-api/tests/source_file.rs`、`apps/desktop/src-tauri/tests/files.rs`、`apps/desktop/src-tauri/tests/window.rs` |
| 入力の設定(3) | `src/domain/inputBytes.test.ts`、`src/domain/base64.test.ts`、`src/components/InputPanel.test.tsx`、`src/components/StatusPanel.test.tsx`(3.9)、`src/app/debuggerIntegration.test.tsx`(0x00・0xFF と入力切れ。3.10) |
| 実行制御(4) | `src/domain/controls.test.ts`、`src/components/Toolbar.test.tsx`、`src/state/runController.test.ts`、`src/state/debuggerStore.test.ts`、`src/app/debuggerIntegration.test.tsx`、`crates/brainfuck-gui-api/tests/service_batch.rs`、`crates/brainfuck-gui-api/tests/service_breakpoints_pause.rs`(Pause)、`crates/brainfuck-gui-api/tests/service_flow.rs` |
| 命令数上限・応答性(5) | `src/domain/stepLimit.test.ts`、`src/components/StepLimitField.test.tsx`(5.1〜5.4、上限なしの警告)、`src/domain/batchPolicy.test.ts`(5.6、5.9 のバッチ幅)、`src/state/runController.test.ts`(Run ループ・Pause)、`src/app/debuggerIntegration.test.tsx`(実行中の編集の拒否。5.10)、`crates/brainfuck-gui-api/tests/service_batch.rs`(`step_limit_error_is_distinct_from_batch_reached_and_keeps_the_machine`)、`crates/brainfuck-gui-api/tests/service_breakpoints_pause.rs`(`pause_stops_a_running_batch_with_cancelled_while_the_batch_holds_the_lock`)、`apps/desktop/src-tauri/tests/commands.rs`(`pause_returns_while_a_batch_holds_the_session_lock`) |
| ブレークポイント(6) | `src/editor/breakpoints.test.ts`、`src/editor/breakpointSync.test.ts`、`src/domain/instructionScan.test.ts`、`src/components/SourceEditor.test.tsx`(全解除ハンドル)、`src/app/debuggerIntegration.test.tsx`、`crates/brainfuck-gui-api/tests/service_breakpoints_pause.rs`、`crates/brainfuck-gui-api/tests/service_batch.rs`(停止と再開、ループ内、Step での無視) |
| エディタ上の強調(7) | `src/editor/executionDecorations.test.ts`、`src/editor/bracketMatching.test.ts`、`src/editor/editorDisplay.test.ts`、`src/components/SourceEditor.test.tsx`、`src/app/accessibility.test.tsx`(色以外の手がかり)、`crates/brainfuck-gui-api/tests/positions.rs`(位置の写像) |
| テープビュー(8) | `src/components/TapeView.test.tsx`、`src/state/runController.test.ts`(テープ窓の再取得)、`crates/brainfuck-gui-api/tests/service_batch.rs`(`tape_window_follows_the_pointer_clips_at_the_edge_and_rejects_bad_radius`)、`crates/brainfuck-gui-api/tests/convert_mapping.rs`(窓の端)、`crates/brainfuck-gui-api/tests/service_flow.rs`(半径の上限) |
| 出力ビュー(9) | `src/domain/outputFormat.test.ts`、`src/domain/outputBuffer.test.ts`、`src/components/OutputView.test.tsx`、`crates/brainfuck-gui-api/tests/convert_mapping.rs`(出力差分)、`crates/brainfuck-gui-api/tests/service_batch.rs`(`output_chunk_keeps_only_the_last_64_kib_and_reset_empties_it`、`non_utf8_output_bytes_are_carried_unchanged_in_the_base64_chunk`) |
| 状態・エラー領域(10) | `src/components/StatusPanel.test.tsx`、`src/components/ErrorBanner.test.tsx`、`src/components/NoticeList.test.tsx`、`src/domain/runLabel.test.ts`、`src/ipc/errors.test.ts`、`src/state/debuggerStore.test.ts`、`src/app/accessibility.test.tsx`(支援技術への通知)、`crates/brainfuck-gui-api/tests/source_file.rs`、`crates/brainfuck-gui-api/tests/session_dto_contract.rs`(エラー文面に内容・パスを含めない) |
| CLI との結果一致(11) | `crates/brainfuck-gui-api/tests/cli_parity_core.rs`(core との全分割一致)、`crates/brainfuck-gui-api/tests/cli_parity_real.rs`(実 CLI バイナリとの一致。11.1〜11.4)、`src/test/machineScript.test.ts`(フロント側の台本が実 CLI と同じ値)、`crates/brainfuck-gui-api/tests/source_file.rs`(改行コード。11.7) |
| データ契約(12) | `crates/brainfuck-gui-api/tests/dto_contract.rs`、`crates/brainfuck-gui-api/tests/session_dto_contract.rs`、`crates/brainfuck-gui-api/tests/convert_mapping.rs`、`crates/brainfuck-gui-api/tests/positions.rs`、`crates/brainfuck-gui-api/tests/service_flow.rs`、`crates/brainfuck-gui-api/tests/service_batch.rs`、`crates/brainfuck-gui-api/tests/bindings_up_to_date.rs`(12.13)、`src/ipc/contract.test.ts`、`src/ipc/backend.test.ts`、`src/ipc/errors.test.ts`、`src/ipc/fakeBackend.test.ts`、`src/state/runController.test.ts`(古い応答の排除。12.12)、`src/app/debuggerIntegration.test.tsx`(破棄済みセッションの遅延応答)、`src/app/App.test.tsx`(画面レジストリ。12.14)、`apps/desktop/src-tauri/tests/commands.rs`(全コマンドの IPC 到達) |
| 安全性・セキュリティ(13) | `apps/desktop/src-tauri/tests/commands.rs`(ケイパビリティの許可集合、CSP の内容、最小サイズ)、`crates/brainfuck-gui-api/tests/service_flow.rs`、`crates/brainfuck-gui-api/tests/service_batch.rs`(外部入力の上限と拒否)、`crates/brainfuck-gui-api/tests/source_file.rs`、`apps/desktop/src-tauri/tests/files.rs`(エラーにパスを含めない)、`src/domain/limits.test.ts` |
| 操作性・アクセシビリティ(14) | `src/app/accessibility.test.tsx`、`src/app/shortcuts.test.ts`、`src/app/documentOperations.test.tsx`、`src/components/Toolbar.test.tsx`、`src/domain/controls.test.ts`、`apps/desktop/src-tauri/tests/commands.rs`(`main_window_keeps_the_minimum_size_and_no_global_tauri`) |
| 回帰(15.7) | `crates/brainfuck-gui-api/tests/regression.rs`(`regression/batch_boundary_halts.bf`)、`src/app/debuggerIntegration.test.tsx`(出力の分割の回帰。5.6)、`src/state/runController.test.ts`(開始の直列化の回帰) |

## 静的確認・部分的・手動となる項目

「手動」の列の番号は `apps/desktop/README.md` の手動スモーク手順の番号。いずれも 2026-11-07 時点で未実施。

| 要件 | 状況 | 確認方法 |
|---|---|---|
| 1.1〜1.3 | 部分的 | 構成と初期表示はコンポーネントテストで確認。実ウィンドウでの表示とタイトルは手動(スモーク 1) |
| 1.4 | 静的 | 1 文書・1 セッションは `Workspace` と `RunController` の構造による。専用のテストは無い(画面レジストリは `App.test.tsx`) |
| 1.5 | 部分的 | 通知文と状態ラベルの日本語は `errors.test.ts`、`runLabel.test.ts` 等で確認。画面全体の網羅は手動(スモーク 1〜15 の観察) |
| 2.1 | 部分的 | 非 ASCII の編集は `SourceEditor.test.tsx` の範囲。実 IME 入力は手動(スモーク 2.3) |
| 2.4 | 静的 | 保存はユーザー操作時のみ。保存コマンドの呼び出し元が文書操作だけであること(`documentFlow.test.ts`、`files.rs`)で裏付けるが、全呼び出し箇所の網羅はコードで確認 |
| 2.5、2.8〜2.13 | 部分的 | 変換・状態遷移・エラー文面は `source_file.rs`、`files.rs`、`documentFlow.test.ts` で確認。ネイティブのファイルダイアログ(モーダル・キャンセル・実ファイルの読み書き・Shift_JIS・1 MiB 超)は手動(スモーク 10) |
| 2.7 | 部分的 | 判定は `window.rs`、`documentFlow.test.ts`、`documentOperations.test.tsx` で確認。実ウィンドウの × と確認ダイアログの 1 回表示は手動(スモーク 10.9、11.1〜11.5) |
| 3.1〜3.10 | 自動 | 入力の変換・切り替え・不正値は自動テストで確認(視覚的な確認は手動スモーク 8.1〜8.2) |
| 4.4、4.5 | 部分的 | Pause の配線と再開は `runController.test.ts`、`service_breakpoints_pause.rs`、`debuggerIntegration.test.tsx` で確認。実 WebView での操作は手動(スモーク 4、9) |
| 5.7、5.8、5.9 | 手動 | 実 WebView でのウィンドウの応答、Pause を 1 秒以内に押せること、表示更新が 1 秒以内に追いつくことの体感は自動化できない(スモーク 4、9)。バッチ幅の適応は `batchPolicy.test.ts` が裏付けにとどまる |
| 5.8(警告) | 部分的 | 上限なしの警告の表示は `StepLimitField.test.tsx` で確認。実表示は手動(スモーク 4.4) |
| 6.1、6.2 | 部分的 | ガタークリックと F9 の切り替えは `breakpoints.test.ts`、`documentOperations.test.tsx` で確認。実ガターのクリックと入力欄での F9 は手動(スモーク 7) |
| 7.1〜7.4、7.9、7.10 | 部分的 | 装飾の範囲と有無は `executionDecorations.test.ts`、`editorDisplay.test.ts` で確認。描画とスクロールの追従は手動(スモーク 5.4、15.2) |
| 7.5〜7.8 | 部分的 | 位置規則は `bracketMatching.test.ts`、`executionDecorations.test.ts`(マルチバイト・サロゲートペア・CRLF・CR・LF)で確認。実エディタ上の見え方は手動(スモーク 2.2〜2.4、14.3) |
| 8.3 | 部分的 | 枠・ラベルなど色以外の手がかりの存在は `TapeView.test.tsx`、`accessibility.test.tsx` で確認。見え方は手動(スモーク 14.3) |
| 8.6 | 部分的 | 要求する窓の大きさの上限は `service_batch.rs` と `service_flow.rs`(半径の境界)で確認。フロントが半径に応じた窓だけを要求することは `runController.test.ts` の範囲 |
| 8.8 | 部分的 | テープ値が core と一致することは `cli_parity_core.rs` で確認(テープ窓を比較)。画面での表示は `TapeView.test.tsx` |
| 10.9、13.7 | 部分的 | エラー文面にソース・入力・絶対パスを含めないことは `source_file.rs`、`files.rs`、`session_dto_contract.rs` で確認。ログ出力の網羅は静的(コードでログ出力の呼び出しを確認) |
| 10.10 | 部分的 | ライブ領域の存在と通知は `accessibility.test.tsx`、`StatusPanel.test.tsx` で確認。スクリーンリーダー(NVDA など)の実読み上げは手動(スモーク 14.4) |
| 11.3 | 部分的 | Core 経由と実 CLI の `Hello World!\n`・`pc=106 dp=6 steps=906` は `cli_parity_real.rs` で確認。GUI 画面での値の確認は手動(スモーク 3) |
| 11.1〜11.8(境界) | 部分的 | 命令数上限 `0` と非 UTF-8 ファイルの `byteOffset` は意図的に対象外(README「CLI との一致検証の対象外」) |
| 13.1 | 部分的 | 既定の上限 10,000,000 は `limits.test.ts`、`StepLimitField.test.tsx` で確認。上限なしは利用者が明示した場合のみで、警告を出す(5.8) |
| 13.2 | 静的 | 実行は `brainfuck-core` の意味論に限る。外部プロセスや OS の呼び出しが無いことは `brainfuck-gui-api` の依存とコードで確認 |
| 13.3 | 部分的 | ケイパビリティの許可集合が登録済みコマンドと一致し、`fs`・`dialog`・`shell`・`http`・`core:window:*` を含まないこと、CSP の内容は `commands.rs` で確認。CSP が実 WebView で効くこと(スクリプトの外部読み込みの遮断)は手動(専用のスモーク項目は無く、スモーク 1 の起動時のコンソールで確認する) |
| 13.4 | 部分的 | 読み書きがダイアログで選んだパスに限られることは `files.rs`、`documentFlow.test.ts` で確認。ダイアログ自体は手動(スモーク 10.1) |
| 13.5 | 静的 | ネットワーク通信を行わない。CSP とケイパビリティに `http` が無いこと(`commands.rs`)と依存の確認 |
| 13.6 | 自動 | 入力・ソース・バッチ幅・半径・ブレークポイント数の上限は `service_flow.rs`、`service_batch.rs`、`service_breakpoints_pause.rs`、`limits.test.ts` で確認 |
| 13.8 | 静的 | `apps/desktop/src-tauri/Cargo.toml` の `[lints.rust] unsafe_code = "forbid"`、`cargo check` と `cargo clippy --all-targets -- -D warnings` の成功 |
| 14.1、14.2 | 部分的 | ショートカットの割り当てと押しっぱなしは `shortcuts.test.ts`、`documentOperations.test.tsx` で確認。F5・Ctrl+R のリロード抑止と右クリックの既定メニューは WebView2 の挙動で手動(スモーク 11.6〜11.7、13) |
| 14.4 | 部分的 | 色以外の手がかりの存在は `accessibility.test.tsx` で確認。コントラストと高コントラストモードは手動(スモーク 14.1〜14.3) |
| 14.5 | 手動 | 800 × 600 の最小サイズは設定値のみ `commands.rs` で確認(`main_window_keeps_the_minimum_size_and_no_global_tauri`)。レイアウトの到達性は手動(スモーク 12) |
| 15.1〜15.4 | 自動 | データ契約・一連の操作・入力と出力の変換・CLI 一致を自動テストで確認(上の表) |
| 15.5 | 静的 | `cargo test --workspace` がフロントのビルド成果物やウィンドウ表示を必要としないこと(`brainfuck-gui-api` は Tauri に依存せず、`apps/desktop/src-tauri` は独立した `[workspace]`)。ルートの全件が成功した記録は README |
| 15.6 | 静的 | `apps/desktop/README.md` に Node 26(`mise exec node@26 --`)、導入・開発起動・ビルド・テスト・手動スモークの手順が記載されていること |
| 実行の体感・実ウィンドウ全般 | 手動 | WebView2 の起動、`npm run tauri dev`、`npm run tauri build` はいずれも未実施。特にスモーク 1(起動と初期状態)と 3(Hello World の GUI 実行と CLI との比較)が未実施のため、MANUAL_VERIFY_REQUIRED |
