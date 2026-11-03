# Brief: brainfuck-cli

## Problem
core を再現可能に実行する手段(ファイル入力、標準入出力、終了コード)が無く、Hello World などの検証や GUI/最適化の比較基準となる実行環境が得られない。

## Current State
greenfield。core 完成後に着手する。

## Desired Outcome
`brainfuck-cli [OPTIONS] <PROGRAM>` がソースを実行し、プログラム出力のみを stdout へ、診断/状態/使用方法を stderr へ出し、終了コード 0/2/3/4 を仕様どおり返す。`examples/hello.bf` が `Hello World!\n` を出力する。

## Approach
`crates/brainfuck-cli/src/main.rs` で引数解析(`--input`/`--max-steps`/`--no-step-limit`/`--dump-state`/`--help`)、`--input` 無しは stdin を EOF まで読む。core のエラーを終了コードへ対応付ける。

## Scope
- **In**: 引数処理、既定ステップ上限 10,000,000、入出力、`--dump-state`、終了コード、`examples/hello.bf`、README(検証コマンド記載)、CLI 結合テスト(stdout/stderr/終了コード)
- **Out**: 対話 UI、デバッガ機能、設定ファイル

## Boundary Candidates
- 引数/入力処理
- エラー→終了コード/診断整形
- 結合テストとサンプル

## Out of Boundary
- 実行意味論(core)
- ブレークポイントやバッチ実行(debugger-state)

## Upstream / Downstream
- **Upstream**: brainfuck-core
- **Downstream**: desktop-gui の CLI 一致検証の基準

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: brainfuck-core

## Constraints
出力はバイト列をそのまま書く(UTF-8 前提にしない)。エラー表示に絶対パス等の機微情報を含めない。
