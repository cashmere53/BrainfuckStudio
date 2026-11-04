# 要件とテストの対応(debugger-state)

最終検証(`/kiro-validate-impl`)は GO(2026-11-04)。テストは `crates/brainfuck-core/tests/` にあり、いずれも外部プロセスなしで成功する。

## 要件の領域とテストファイル

| 要件の領域 | 主な検証 |
|---|---|
| 2〜3: セッション生成・観測・リセット・Step Into・停止理由 | `debug_session.rs`, `debug_view.rs` |
| 3.x / 5.x: ブレークポイント集合・停止・再開規則 | `debug_breakpoints.rs`, `debug_regression.rs`(`regression/debug_batch_boundary_breakpoint.bf`) |
| 6.x: キャンセル契約・バッチ実行 | `debug_cancel.rs`, `debug_session.rs` |
| 8.x: テープ表示窓・状態ビュー | `debug_view.rs` |
| 9.x: ソース位置変換(UTF-16・行・CRLF/CR・逆引き) | `debug_source_map.rs` |
| 10.x: 位置変換からのブレークポイント正規化 | `debug_breakpoints.rs`(4.4 の合成テスト) |
| 11.x: core の `run()` との差分・回帰・公開面 | `debug_differential.rs`, `debug_regression.rs`, ドキュメントテスト(`lib.rs`) |

## 自動テストでは部分的、または手動・静的確認となる項目

| 要件 | 状況 | 確認方法 |
|---|---|---|
| 1.1 | 静的 + 間接 | `git diff --stat 2ba3260 -- crates/brainfuck-core` で既存ファイルの差分が `lib.rs`(追加のみ)であること、既存テストの成功 |
| 1.2 | 静的 | 新規 src に UI・標準入力・ファイル・スレッドの使用がないこと、core の依存が空であること |
| 1.3 | コンパイル時 | `#![forbid(unsafe_code)]` と workspace の lint |
| 1.4 | 静的 | 新規テストに外部プロセス・スレッド生成がないこと |
| 1.5 | 静的 | シリアライズ形式への依存がなく、derive が標準的なものに限られること |
| 1.6 | 静的 | 公開型の `#[non_exhaustive]` と private フィールドのレビュー |
| 1.7 | 部分的 | `Send` のコンパイル時検証は `DebugSession` と `SourceMap` のみ |
| 1.8 | 間接 | 既存 core テストの無変更での成功と `debug_differential.rs` |
| 6.8 | 部分的 | `AtomicBool`/`Arc` の委譲は単一スレッドで検証。別スレッドからの可視性は Acquire/Release の契約と説明コメントに依拠(スレッド生成禁止のため) |
| 11.7 | 手動 | main(2ba3260)との差分で既存テストファイルが無変更であること |
| 11.9 | 手動 | `cargo test --workspace` の成功 |

## 実装時の補足
- `run_batch` はバッチ幅に達した時点でもキャンセルを 1 回確認する(判定順序: 停止 → キャンセル → ブレークポイント → バッチ幅 → 実行)。デスクトップ GUI から呼ぶ際の注意点。
- `tests/debug_common/mod.rs` は 1.1 で作成して以降編集せず、未使用ヘルパの警告は同ファイルの属性で抑えている。
