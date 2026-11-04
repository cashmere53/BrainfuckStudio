# 要件とテストの対応(optimized-interpreter)

最終検証(`/kiro-validate-impl`)は GO(2026-11-04)。テストは `crates/brainfuck-optimized/tests/` と各モジュールの単体テストにあり、いずれも外部プロセスなしで成功する。

## 要件の領域とテストファイル

| 領域 | 主な検証 |
|---|---|
| IR の型・変換(2〜4: 統合・一般ループ・パターン認識) | `ir.rs`(結合テスト)、`diff_patterns.rs` |
| コスト・到達範囲(14.7〜14.8) | `cost.rs`、`ir.rs` 内の単体テスト |
| 素朴再生 `replay`(14.2〜14.5) | `replay.rs`(結合テスト)、`src/replay.rs` の単体テスト |
| 機械の API 契約(6, 7, 10) | `semantics.rs`、`machine.rs` 内の単体テスト |
| 素朴実装との差分(11) | `diff_smoke.rs`、`diff_exhaustive.rs`、`diff_random.rs`、`diff_patterns.rs`、`diff_limits.rs`、`diff_edges_io.rs` |
| 回帰(11.8) | `regression.rs`(`regression/cell0_multiply_body_out_of_range.bf`) |
| ベンチ(12) | `bench_programs.rs`(正規テスト側)、`benches/interpreter_comparison.rs`(`cargo bench`) |

## 静的確認・部分的・手動となる項目

| 要件 | 状況 | 確認方法 |
|---|---|---|
| 1.1 | 静的 | `git diff --stat 5218111 -- crates/brainfuck-core` が空であること |
| 1.2 | 静的 | Cargo.toml の依存が `brainfuck-core` のみ、src が std の fs/io/env を使わないこと |
| 1.4 | 静的 | `#![forbid(unsafe_code)]` と workspace lint |
| 1.7 | 静的 | `ReplayState::new` の固定長テープ |
| 2.2 | 部分的 | 決定性は同一プロセス内の再変換の一致のみ |
| 2.6 | 静的 | 括弧の対応は変換時に確定(IR テストあり)。実行中に探索しないことはコードで確認 |
| 2.7 | 部分的 | 操作数 ≤ 命令数は例示プログラムで検証。一般には構造上の不変条件 |
| 5.6 | 静的 | 添字は範囲検査の後のみ、飽和演算。多数のテストの成功は裏付けにとどまる |
| 6.5 | 静的 | 実行数の 2 定義の区別は名称、rustdoc、README |
| 7.7 | 部分的 | 制限なし設定は停止するプログラムでしか検証できない |
| 10.7 | 静的 | 最適化済みプログラムの不変性は借用で保証 |
| 11.2 | 部分的 | 括弧整合の網羅比較は通常実行で長さ ≤ 6。長さ 8 は `#[ignore]` で、`cargo test --release -p brainfuck-optimized --test diff_exhaustive -- --ignored` により 2026-11-04 に約 297 秒で成功(手動実行)。個数は独立 DP と照合するテストで固定 |
| 11.9 | 静的 | `harness = false` と `cargo test` の出力でベンチが実行されないこと |
| 12.4 | 手動 | ベンチの不一致時の非 0 終了は自動テストがなく、最終検証で変異を入れて確認 |
| 12.5〜12.7 | 手動 | 時間を成否に使わないこと、README とベンチ出力ヘッダ |
| 13.1〜13.3 | 静的 | core のエラー型を再利用し、新たに型を作らないこと。エラー文面は数値のみ |
| 14.6, 14.9 | 静的 | 縮退経路は公開 `replay` そのもの。命令意味論の `match` は `replay.rs` のみ。コスト式は IR の `naive_cost` / `static_reach` のみ(grep と変異テストで確認) |
| 公開 API の面 | 静的 | モジュールの可視性で確認(`compile_fail` テストは無い) |

## 実装時の補足
- 上限掃引の cap は 64(設計の例は 512)。長い `[-]` は cap で打ち切られるため、`diff_patterns.rs` / `diff_limits.rs` で補っている。
- `ir_ops_executed` は一括適用した IR 操作の回数で、縮退再生の命令は数えない。core に対応値がなく、比較対象外。
