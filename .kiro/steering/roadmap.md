# Roadmap

## Overview
`brainfuck-studio-development-specification.md`(v1.0)を実装し、Brainfuck を正確に実行し、実行過程を観察・比較できる学習用スタジオを段階的に構築する。Rust workspace(`brainfuck-core` / `brainfuck-cli`)を土台に、デバッグ状態、Tauri 2 デスクトップ GUI、最適化インタプリタ、コンパイラ比較へ拡張する。

全フェーズを仕様書のフェーズ計画に沿って 6 spec に分解する。Phase 1 の意味論(出力・最終テープ・エラー)が全後続フェーズの正解オラクルとなる。

Phase 2 の完了後、仕様書の外側として品質基盤の 2 spec(desktop-gui の E2E 自動テスト)を追加した(2026-11-08)。実機の手動スモークを繰り返し実行できる自動テストに置き換えることが目的で、製品機能は追加しない。

## Approach Decision
- **Chosen**: 仕様書のフェーズ/モジュール境界に沿った垂直・段階的分解(core → cli / debugger-state → desktop-gui、core → optimized-interpreter → compiler-comparison)
- **Why**: 仕様書が既にフェーズ、モジュール、受け入れ基準を定義している。core は Tauri/IO/UI に非依存という制約により、後続 spec が独立して再利用できる。
- **Rejected alternatives**: (1) 単一の巨大 spec(タスク数過多、レビュー境界が曖昧)、(2) parser と machine の分割(密結合で独立実装の利点が小さい)

## Scope
- **In**: 仕様書の Phase 1 / 1.5 / 2 / 3 / 4 の全て
- **Out**: Brainfuck 方言の無制限対応、任意コード実行、無制限の計算資源、Step Back(可逆実行)、テープの動的拡張

## Constraints
- `brainfuck-core` は Tauri・標準入力・ファイルシステム・UI に依存しない。I/O はバイト列で受け渡す
- `unsafe` 禁止。テープは 30,000 × u8 固定、EOF=0、`wrapping_add/sub`、範囲外はエラー
- 命令数制限を既定で有効(CLI 既定 10,000,000)
- Phase 3/4 の結果は Phase 1 の意味論と観測的に完全一致すること
- GUI は Rust 内部型を複製せず JSON DTO を介す。Tauri 2 + React + TypeScript + Vite + CodeMirror 6
- ツール: Node 26(mise.toml)、Rust は stable
- Markdown 成果物は日本語

## Boundary Strategy
- **Why this split**: core(意味論の正)、cli(プロセス境界)、debugger-state(デバッグ契約)、desktop-gui(UI 境界)、最適化/コンパイラ(意味論保存の別実装)は責務と変更理由が異なる。
- **Shared seams to watch**:
  - core の公開 API(`Machine` / `MachineSnapshot` / `StopReason` / `Program`)は全 spec の契約。debugger-state が追加する API は core を後方互換で拡張する
  - 命令添字 ↔ ソース範囲の対応(core が保持、debugger-state が変換 API 提供、GUI が利用)
  - 実行命令数の定義(最適化方式で異なる場合の比較画面での明示)
  - CLI と GUI の結果一致(同一入力で同一出力)

## Specs (dependency order)
> チェックボックスは **実装完了** を表す(各 spec の `spec.json` の `phase` が `implementation-complete`、かつ最終検証が GO。実機での手動確認が必要でその一部が未実施の場合は、自動検証がすべて成功し、未確認の項目が spec の `traceability.md` と実装の README に明記されていれば、`[x]` としてよい。その場合の最終検証は MANUAL_VERIFY_REQUIRED(一部)となる)。`[ ]` は要件・設計・タスクが承認済みで実装待ち(`phase: tasks-generated`)の状態を指す。
- [x] brainfuck-core -- Cargo workspace、命令/パーサ(jump_table)/Machine/エラー/スナップショット/step/run/ステップ制限と単体テスト。Dependencies: none
- [x] brainfuck-cli -- ファイル/stdin 入力、stdout/stderr 分離、終了コード 0/2/3/4、`--max-steps` 等、`examples/hello.bf`、README。Dependencies: brainfuck-core
- [x] debugger-state -- Phase 1.5: ブレークポイント、StopReason(Breakpoint/Cancelled)、バッチ実行(既定1,000命令)、テープ表示窓(±16)、命令添字→ソース範囲/行・列変換。Dependencies: brainfuck-core
- [x] desktop-gui -- Phase 2: Tauri 2 + React + CodeMirror 6、Run/Pause/Step/Reset、テープ/出力/状態ビュー、括弧対応・現在命令強調、DTO と Tauri コマンド(最小 6 コマンドを含む。詳細は desktop-gui の design)、CLI 一致検証。Dependencies: debugger-state, brainfuck-cli, brainfuck-core
- [x] optimized-interpreter -- Phase 3: IR、連続命令統合、代表的ループ最適化、素朴実装とのディファレンシャルテスト、ベンチマーク(正規テストと分離)。Dependencies: brainfuck-core
- [ ] compiler-comparison -- Phase 4: WASM またはネイティブ AOT の少なくとも一方、方式・実行時間・命令数・終了状態を表示する比較画面。Dependencies: optimized-interpreter, desktop-gui, brainfuck-core
- [x] desktop-gui-e2e-browser -- 品質基盤: Playwright + Chromium で desktop-gui の画面を検証する E2E。`brainfuck-gui-api` の stdio バイナリと Node の中継で実 Rust バックエンドを通し、`@tauri-apps/api/mocks` の `mockIPC` で IPC を差し替える。手動スモーク 1〜15 のうちブラウザで検証できる項目を自動化し、Linux の CI で実行する。Dependencies: desktop-gui
- [x] desktop-gui-e2e-webview -- 品質基盤: 実 Tauri アプリ(WebView2、Windows 限定)に CDP(`connectOverCDP`)で接続し、起動・Hello World・Pause と再開・閉じる確認・WebView2 の既定動作の抑止・CSP の実効を少数のシナリオで検証する。実験的で、CI では初期は非必須。Dependencies: desktop-gui-e2e-browser, desktop-gui

## Direct Implementation Candidates
- [ ] ci-baseline -- GitHub Actions の基本ワークフロー(`cargo fmt --check`、`cargo clippy --workspace --all-targets -D warnings`、`cargo test --workspace`、`apps/desktop` の typecheck / lint / test / build)。現在ワークフローは存在しない。spec を要するほどの設計判断が無いため直接実装する。E2E の 2 spec はそれぞれ独立したワークフローファイル(`e2e-browser.yml` / `e2e-webview.yml`)に自分の job を置き、ci-baseline のワークフローは変更しない。ci-baseline と E2E の 2 spec の間に順序の依存は無い

## Notes
- E2E の 2 spec の境界: browser は「画面と IPC 契約の検証(Linux で動く)」、webview は「実配布形でしか起きないこと(Windows 限定)」。重複するシナリオは browser に置き、webview は少数に絞る。ネイティブダイアログ(開く・保存)はどちらでも自動化できず、browser では中継が返すパスで代替し、webview では対象外とする。3.2 の CLI との比較は Rust の parity テストが担う。NVDA の読み上げは手動のまま
- E2E の技術判断(2026-11-08 の discovery): 層 A の動くバックエンドは、TS での再実装(Rust と乖離すると偽の合格になる)や台本のみ(Run・Step・Pause の流れを扱えない)ではなく、実 Rust の橋渡しを選んだ。層 B は WebdriverIO(`@wdio/tauri-service`)ではなく Playwright の CDP 接続を選び、行き詰まった場合の切り替え先として WebdriverIO を記録する
- compiler-comparison のタスクグループ 1〜3(WASM バックエンドと比較ロジック)は optimized-interpreter のみに依存し、desktop-gui 完了前に着手できる。グループ 4〜7(GUI 統合)は desktop-gui に依存する
- 各 spec の design で追加された crate は structure.md を参照
- 実装状況(2026-11-08 時点。desktop-gui の最終検証と実機確認は 2026-11-07 に実施): brainfuck-core、brainfuck-cli、debugger-state、optimized-interpreter、desktop-gui が実装完了(要件→テストの対応は各 spec の `traceability.md`)。desktop-gui は自動検証がすべて成功し、実機での手動スモークも実施可能な項目は合格済みで、最終検証は MANUAL_VERIFY_REQUIRED(一部)。未確認の項目は、ライトテーマ・OS の高コントラスト・スクリーンリーダー(NVDA)の確認と、1 秒以内の更新周期などの計測で、`.kiro/specs/desktop-gui/traceability.md` と `apps/desktop/README.md` の「検証結果の記録」に記載している。compiler-comparison は desktop-gui の完了により、グループ 1〜7 のすべてに着手できる
