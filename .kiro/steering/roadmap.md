# Roadmap

## Overview
`brainfuck-studio-development-specification.md`(v1.0)を実装し、Brainfuck を正確に実行し、実行過程を観察・比較できる学習用スタジオを段階的に構築する。Rust workspace(`brainfuck-core` / `brainfuck-cli`)を土台に、デバッグ状態、Tauri 2 デスクトップ GUI、最適化インタプリタ、コンパイラ比較へ拡張する。

全フェーズを仕様書のフェーズ計画に沿って 6 spec に分解する。Phase 1 の意味論(出力・最終テープ・エラー)が全後続フェーズの正解オラクルとなる。

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
> チェックボックスは **実装完了** を表す(各 spec の `spec.json` の `phase` が `implementation-complete`、かつ最終検証が GO)。`[ ]` は要件・設計・タスクが承認済みで実装待ち(`phase: tasks-generated`)の状態を指す。
- [x] brainfuck-core -- Cargo workspace、命令/パーサ(jump_table)/Machine/エラー/スナップショット/step/run/ステップ制限と単体テスト。Dependencies: none
- [x] brainfuck-cli -- ファイル/stdin 入力、stdout/stderr 分離、終了コード 0/2/3/4、`--max-steps` 等、`examples/hello.bf`、README。Dependencies: brainfuck-core
- [x] debugger-state -- Phase 1.5: ブレークポイント、StopReason(Breakpoint/Cancelled)、バッチ実行(既定1,000命令)、テープ表示窓(±16)、命令添字→ソース範囲/行・列変換。Dependencies: brainfuck-core
- [x] desktop-gui -- Phase 2: Tauri 2 + React + CodeMirror 6、Run/Pause/Step/Reset、テープ/出力/状態ビュー、括弧対応・現在命令強調、DTO と Tauri コマンド(最小 6 コマンドを含む。詳細は desktop-gui の design)、CLI 一致検証。Dependencies: debugger-state, brainfuck-cli, brainfuck-core
- [x] optimized-interpreter -- Phase 3: IR、連続命令統合、代表的ループ最適化、素朴実装とのディファレンシャルテスト、ベンチマーク(正規テストと分離)。Dependencies: brainfuck-core
- [ ] compiler-comparison -- Phase 4: WASM またはネイティブ AOT の少なくとも一方、方式・実行時間・命令数・終了状態を表示する比較画面。Dependencies: optimized-interpreter, desktop-gui, brainfuck-core

## Notes
- compiler-comparison のタスクグループ 1〜3(WASM バックエンドと比較ロジック)は optimized-interpreter のみに依存し、desktop-gui 完了前に着手できる。グループ 4〜7(GUI 統合)は desktop-gui に依存する
- 各 spec の design で追加された crate は structure.md を参照
- 実装状況(2026-11-08 時点): brainfuck-core、brainfuck-cli、debugger-state、optimized-interpreter、desktop-gui が実装完了(要件→テストの対応は各 spec の `traceability.md`)。desktop-gui は自動検証と実機での手動スモークの大半が完了済みで、ライトテーマ・OS の高コントラスト・スクリーンリーダー(NVDA)の確認と、1 秒以内の更新周期などの計測が未確認(README の「検証結果の記録」)。compiler-comparison は desktop-gui の完了により、グループ 1〜7 のすべてに着手できる
