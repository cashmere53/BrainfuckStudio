# Brief: debugger-state

## Problem
GUI がフリーズせずに実行・停止・ステップできるよう、停止理由、ブレークポイント、バッチ実行、位置情報を core レベルで提供する必要がある。

## Current State
core は `step`/`run`/`snapshot` を持つが、ブレークポイント・キャンセル・バッチ実行・命令添字→ソース範囲変換は無い(Phase 1.5)。

## Desired Outcome
UI から再利用できるデバッグ契約: `StopReason::{Halted, StepLimitReached, Breakpoint, Cancelled}`、命令添字に正規化されたブレークポイント、既定 1,000 命令のバッチ実行、ポインタ周辺(既定 ±16)のテープ窓、命令添字↔ソース範囲(行・列)変換。

## Approach
core を後方互換で拡張する。ブレークポイントは停止前に現在 `pc` と照合。キャンセルは呼び出し側が供給するフラグ/トークンで確認する(core はスレッド・UI 非依存)。コメント文字上には停止しない。

## Scope
- **In**: ブレークポイント集合、バッチ実行 API、Cancelled、テープ窓取得、位置変換、関連テスト
- **Out**: Step Back/可逆実行、Tauri コマンド/DTO(desktop-gui)、UI

## Boundary Candidates
- 停止理由とブレークポイント判定
- バッチ/キャンセル実行制御
- 表示用ビュー(テープ窓・位置変換)

## Out of Boundary
- 基本意味論の変更(core)
- JSON DTO・IPC(desktop-gui)

## Upstream / Downstream
- **Upstream**: brainfuck-core
- **Downstream**: desktop-gui

## Existing Spec Touchpoints
- **Extends**: brainfuck-core の公開 API(後方互換の追加)
- **Adjacent**: brainfuck-cli(挙動を変えない)

## Constraints
core 非依存制約を維持。デバッガの単位は実行命令。`unsafe` 禁止。
