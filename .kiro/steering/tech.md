# Technology Stack

## Architecture

Rust workspace を中核とし、UI は薄い境界の外側に置く。

- `brainfuck-core`: 構文解析、状態機械、実行制限。**Tauri・標準入力・ファイルシステム・UI に依存しない**。I/O は呼び出し元が供給する入力バイト列と、取得する出力バイト列で扱う
- `brainfuck-cli`: ファイル/標準入出力、終了コード
- `apps/desktop`: Tauri 2 が Rust コアと UI を橋渡し。フロントは JSON DTO のみ扱う

## Core Technologies

- **Language**: Rust(stable、`unsafe` 禁止)/ TypeScript
- **Desktop**: Tauri 2
- **UI**: React + TypeScript + Vite、エディタは CodeMirror 6
- **Runtime**: Node 26(`mise.toml`)

## Development Standards

### Semantics (不変条件)
- テープ `u8[30000]`、有効 `dp` は `0..=29999`、セル演算は `wrapping_add/sub`
- `,` は EOF で 0。入出力はバイト列(UTF-8 前提にしない)
- 括弧対応は**解析時に一度だけ**構築(`jump_table`)。実行中に探索しない
- `step()` は実行した命令 1 個につき実行数を +1。停止済みなら状態不変で `Halted`
- 範囲外移動は状態を変更せずエラー

### Error Handling
- Parse / Runtime / CLI(I/O)のエラーを型で区別し、原因・命令添字・可能ならソース位置を含める
- エラーに入力バイト列や絶対パスなどの機微情報を含めない

### Testing
- core は外部プロセス無しで単体テスト可能にする
- 観点: パーサ、8 命令、ラップ、境界、ループ、step の一命令性、ステップ上限、結合(Hello World/エコー/CLI 終了コード)
- 発見した不具合ごとに最小の `.bf` と回帰テストを追加
- Phase 3 以降は素朴インタプリタをオラクルとした差分テスト。ベンチマークは正規テストと分離

## Development Environment

### Common Commands
```bash
# Test: cargo test --workspace
# Run:  cargo run -p brainfuck-cli -- examples/hello.bf
```

## Dependency & Lint Policy

- `brainfuck-core` と `brainfuck-cli` は外部 crate に依存しない。`brainfuck-optimized` も `brainfuck-core` のみに依存する
- 外部依存を持つのは GUI 系と WASM 系のみ(serde、base64、ts-rs、tauri 関連、wasm-encoder、wasmparser、wasmi、フロントの Vitest/Testing Library/ESLint など)
- `unsafe` 禁止は自前コードに適用する(tauri・wasmi・ts-rs 等の内部は対象外)
- 検証コマンド: `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`
- Rust ツールチェーンは stable(`mise.toml` は Node のみ固定)

## Key Technical Decisions

- セル幅 u8・テープ 30,000 固定: 標準的で予測可能な教育用既定値
- 無限拡張ではなく範囲外エラー: 状態を可視化しやすい
- EOF=0: 対話・バッチで決定的
- デバッガの単位は「有効命令」。コメント文字には停止しない
- CLI 既定ステップ上限 10,000,000。GUI 実行はキャンセル可能、バッチ既定 1,000 命令
- 巨大テープ全体を高頻度に送らず、表示窓(既定 ±16 セル)のみ返す
- WASM(`wasmi` で実行)を採用し、ネイティブ AOT は `unsafe`/任意コード実行の非目標に反するため見送り(compiler-comparison)
- 仕様書からの意図的な差異(各 spec の design で決定):
  - CLI は `--input` が無く、プログラムに `,` が含まれる場合のみ標準入力を読む(端末で `hello.bf` が入力待ちにならないようにするため)
  - GUI のバッチ幅は既定 1,000 から往復時間に応じて最大 1,000,000 まで適応する
  - GUI の Tauri コマンドは最小 6 個に加え、ファイル操作や比較用を含む(詳細は desktop-gui の design)

---
_Document standards and patterns, not every dependency_