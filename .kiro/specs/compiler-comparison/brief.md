# Brief: compiler-comparison

## Problem
同一 Brainfuck プログラムを複数の実行方式(素朴/最適化/WASM/ネイティブ)で動かしたときの差を比較・観察できない。

## Current State
最適化インタプリタと GUI が前提。コンパイラは未着手(Phase 4)。

## Desired Outcome
WASM とネイティブ AOT の少なくとも一方が Phase 1 と同一の観測結果を返し、GUI の比較画面に方式、実行時間、実行命令数(定義が異なる場合は明示)、終了状態が表示される。

## Approach
IR からターゲットコードを生成(WASM 優先を検討、設計フェーズで WASM/AOT の選択を確定)。素朴インタプリタをオラクルに差分テスト。比較画面を desktop-gui に追加。

## Scope
- **In**: 少なくとも一方のコンパイラバックエンド、同一性テスト、比較画面、実行メトリクス収集
- **Out**: 高度な最適化、JIT、パッケージ配布

## Boundary Candidates
- コード生成バックエンド
- 実行ランタイム境界(I/O・テープ・制限)
- 比較画面

## Out of Boundary
- 基準意味論(core)、IR 最適化(optimized-interpreter)、基本 GUI(desktop-gui)

## Upstream / Downstream
- **Upstream**: optimized-interpreter、desktop-gui
- **Downstream**: なし

## Existing Spec Touchpoints
- **Extends**: desktop-gui(比較画面の追加)
- **Adjacent**: optimized-interpreter

## Constraints
任意コード実行の非目標に反しないサンドボックス方針、ステップ/資源制限の維持、観測結果の完全一致。
