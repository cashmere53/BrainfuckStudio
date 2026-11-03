# Brief: optimized-interpreter

## Problem
素朴なインタプリタは長いループ・連続命令で遅く、実行方式による性能差を学習・比較できない。

## Current State
core の素朴インタプリタのみ(Phase 1)。

## Desired Outcome
IR ベースの最適化インタプリタが、素朴実装と観測的に完全一致(出力、最終テープ、エラー種別)しつつ高速に動作し、ベンチマークで差を示せる。

## Approach
パース結果を IR へ変換し、連続命令統合と代表的ループ(例: クリアループ、移動/加算ループ)の最適化を行う。素朴インタプリタをオラクルに、生成した小規模プログラムでディファレンシャルテストを行う。ベンチは正規テストと分離。

## Scope
- **In**: IR 設計、連続命令統合、代表的ループ最適化、エラー/ステップ制限の意味保存方針、ディファレンシャルテスト、ベンチマーク
- **Out**: JIT/AOT/WASM(compiler-comparison)、GUI 統合

## Boundary Candidates
- IR 生成と最適化パス
- 最適化実行器
- 差分テスト/ベンチ基盤

## Out of Boundary
- 基準意味論の変更(core)
- コンパイラ生成(compiler-comparison)

## Upstream / Downstream
- **Upstream**: brainfuck-core(オラクル)
- **Downstream**: compiler-comparison

## Existing Spec Touchpoints
- **Extends**: なし(core は変更しない)
- **Adjacent**: brainfuck-core

## Constraints
実行命令数の定義が素朴実装と異なる場合は明示。範囲外ポインタ・ステップ上限のエラー位置の扱いを設計で明確化する。`unsafe` 禁止。
