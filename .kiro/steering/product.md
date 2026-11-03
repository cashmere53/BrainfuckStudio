# Product Overview

Brainfuck Studio は、Brainfuck を正確に実行し、実行過程(ソース、命令位置、テープ、出力)を観察・比較できる学習用スタジオ。学習者・教育用途を主な対象とする。

## Core Capabilities

- 正確でテスト可能な Brainfuck 実行エンジン(`brainfuck-core`)
- CLI による再現可能な実行(`brainfuck-cli`)
- デバッグ GUI: 現在命令、テープ、出力、停止理由の可視化(Phase 2)
- 実行方式の比較: 素朴/最適化インタプリタ、WASM/ネイティブ AOT(Phase 3/4)

## Target Use Cases

- 学習者が 1 ステップずつ実行して状態遷移を観察する
- 同一プログラム・同一入力を CLI と GUI で実行し結果が一致することを確認する
- 実行方式ごとの時間・命令数・終了状態を比較する

## Value Proposition

- **単一の意味論**: Phase 1 の素朴インタプリタが正解オラクル。最適化・コンパイル結果は出力・最終テープ・エラーで完全一致しなければならない
- **決定性**: u8 セル、30,000 セル固定、範囲外はエラー、EOF=0。教育用に予測可能
- **安全性**: 任意ソースは信頼しない入力。命令数制限を既定で有効化

## Non-goals

Brainfuck 方言の無制限対応、任意コード実行、無制限の計算資源、Phase 1 での JIT/AOT、Step Back(可逆実行)。

---
_Source of truth: `brainfuck-studio-development-specification.md`。分解と順序は `roadmap.md`_
