# Brief: brainfuck-core

## Problem
Brainfuck を学習・比較するすべての機能(CLI、GUI、最適化、コンパイラ)が依存する、正確でテスト可能な実行エンジンが存在しない。

## Current State
greenfield。Cargo workspace もコードも無い。仕様書 §4〜§6 に設計が定義済み。

## Desired Outcome
`brainfuck-core` crate が、8 命令・30,000 セル・u8 ラップ・範囲外エラー・バイト I/O・EOF=0 を正しく実装し、GUI/IO なしで `Machine::step()` と `run()` を利用でき、スナップショットで `pc`/`dp`/セル値/実行数を観測できる。

## Approach
仕様書どおり `instruction` / `parser` / `machine` / `error` モジュールを持つ単一 crate として実装。パース時に jump_table を一度だけ構築し、実行中に括弧探索しない。Cargo workspace ルートも本 spec で用意する。

## Scope
- **In**: workspace 雛形、`Instruction`(ソースバイトオフセット付き)、`Program::parse`/`jump_table`、`ParseError`(UnmatchedClosing/Opening)、`Machine`/`MachineConfig`/`StepResult`/`MachineSnapshot`/`tape()`/`output()`、`RuntimeError`(PointerOutOfBounds, StepLimitExceeded)、`run()` と `StopReason::Halted`/`StepLimitReached`、仕様書 §11 の単体・回帰テスト
- **Out**: CLI、ブレークポイント実行、キャンセル、バッチ実行、GUI、最適化

## Boundary Candidates
- 構文解析(Program/jump_table)
- 実行状態機械(Machine/step/run)
- エラー型(Parse/Runtime)

## Out of Boundary
- ファイル・stdin/stdout 処理(cli)
- Breakpoint/Cancelled の実際の挙動(debugger-state。型の予約のみ可)
- 行・列変換 API(debugger-state)

## Upstream / Downstream
- **Upstream**: なし(Rust stable のみ)
- **Downstream**: brainfuck-cli, debugger-state, optimized-interpreter(オラクル)

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: debugger-state(`StopReason`・`MachineSnapshot` を拡張)

## Constraints
Tauri/stdin/fs/UI 非依存、`unsafe` 禁止、`wrapping_add/sub`、エラーに入力バイト列や絶対パスを含めない。
