# Brief: desktop-gui

## Problem
学習者がソース、命令位置、テープ、出力を視覚的に観察しながら Brainfuck を実行する手段が無い。

## Current State
greenfield。debugger-state が提供するデバッグ契約に依存する。`apps/desktop/` は未作成。

## Desired Outcome
Tauri 2 + React + TypeScript + Vite + CodeMirror 6 のデスクトップアプリで、編集、Run/Pause/Step/Reset、入力設定、テープ・出力・状態・エラー表示ができ、同一入力で CLI と同じ結果を得る。

## Approach
Rust 側で core/debugger-state をラップし、JSON DTO と最小 6 コマンド(`load_source`/`start_session`/`run_batch`/`step`/`reset`/`get_snapshot`)+ テープ表示窓 API を公開。フロントは内部型を複製せず DTO のみ扱う。CodeMirror で現在命令・括弧対応・ブレークポイントを表示。出力は UTF-8/16 進切替。

## Scope
- **In**: `apps/desktop`、Tauri コマンド/DTO、エディタ、操作パネル、テープ/出力/状態ビュー、未保存内容の実行、明示保存、CLI 一致検証
- **Out**: 最適化・コンパイラ比較画面、Step Back、クラウド機能

## Boundary Candidates
- Tauri IPC/DTO 境界
- エディタ(CodeMirror 拡張)
- 実行制御と可視化パネル

## Out of Boundary
- 実行意味論・デバッグロジック(core/debugger-state)
- 比較画面(compiler-comparison)

## Upstream / Downstream
- **Upstream**: debugger-state(間接的に core、CLI 一致検証に brainfuck-cli)
- **Downstream**: compiler-comparison

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: brainfuck-cli(結果一致基準)

## Constraints
Node 26(mise.toml)、実行はキャンセル可能、巨大テープ全体を高頻度転送しない、保存はユーザー明示時のみ。
