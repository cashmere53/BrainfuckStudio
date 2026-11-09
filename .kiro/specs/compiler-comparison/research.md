# Research & Design Decisions: compiler-comparison

---
**Purpose**: コンパイル実行方式(WASM / ネイティブ AOT)の選択、サンドボックス方針、観測的一致を保つコード生成方式の調査・判断・トレードオフの記録。`design.md` は自己完結しており、本書は背景の補足である。
---

## Summary
- **Feature**: `compiler-comparison`
- **Discovery Scope**: Complex Integration(新規の Rust crate 3 つ + Tauri 層 / フロントへの追加。外部ランタイム依存の選定とセキュリティ方針を含むため、拡張の軽量発見ではなく全体発見を行った)
- **Key Findings**:
  - **WASM を採用し、ネイティブ AOT は不採用**。ネイティブ AOT は (a) 外部のコンパイラ・リンカの起動(要件 5.4 違反、任意コード実行の非目標に近接)、または (b) 機械語を生成してプロセス内で実行する経路(`unsafe` と実行可能メモリが必須で、`unsafe` 禁止と JIT 非スコープに反する)のいずれかを要する。WASM は「外部から見える操作が宣言された import だけ」という能力ベースの隔離を得られ、`unsafe` を自コードに含めずに済む
  - **実行エンジンは `wasmi`(インタプリタ)を採用**。実行時に機械語を生成しない(JIT なし)、`unsafe` を自コードに含めない、燃料(fuel)計測とメモリ制限(`ResourceLimiter`)を標準で持つ、純 Rust でビルドが軽い。性能面では `wasmtime`(Cranelift)に劣るが、比較画面は実行時間の優劣を保証せず観察対象とする(要件 8.7, 10.11)。エンジンは `runtime.rs` に隔離し、将来 `wasmtime` へ差し替えても公開 API と生成物を変えない
  - **コード生成は最適化 IR(`OptimizedProgram`)から行い、「事前検査 + 操作境界での離脱(bailout)+ ホスト側の素朴再生」で完全状態一致を達成**する。`optimized-interpreter` が `FastPath` / `Fallback` で確立した方式を、生成コード(事前検査) + ホスト(再生)に写したもの。生成コードは「素朴実装がエラーまたは上限到達となる操作」を実行せず、操作の先頭の状態(テープ・`dp`・実行数・出力・入力位置は正確)で離脱し、ホストが素朴意味論で当該操作から終端まで再生する。再生は上流 `optimized-interpreter` が公開する `brainfuck_optimized::replay` を使い、自前の再実装は持たない(改版による決定)
  - 実行数の換算コストと範囲条件は `optimized-interpreter` の公開ヘルパ `Op::naive_cost` / `Op::static_reach`(閉形式表)に従い、生成コードは操作の実行前に「コスト ≤ 残り予算」を検査するだけで素朴実装と同じ上限境界を再現できる
  - 比較画面の実行数は、全方式で「素朴実装換算」を正確に報告できる。方式固有の単位は素朴が「実行した命令数」、最適化が「IR 操作回数」、WASM は「なし」(数えると生成コードが遅くなり、エンジン固有の燃料は決定的な学習材料にならないため)

## Research Log

### 上流 spec の契約(読了)
- **Sources Consulted**: `optimized-interpreter` の requirements / design / research、`desktop-gui` の requirements / design(「Extension Points for compiler-comparison」)、`brainfuck-core` の design、`brainfuck-cli` の design、仕様書 §3 §11 §12
- **Findings**:
  - `OptimizedProgram::ops()` / `OpKind`(`#[non_exhaustive]`: `Add` `Move` `Output` `Input` `LoopStart` `LoopEnd` `Clear` `Transfer` `Scan`)と `Op::source_range()` が IR の公開 API。`OpKind` は将来拡張されうるため、未知の操作は「生成コードで常に離脱」に倒す
  - `OptimizedMachine` は `steps_executed`(素朴換算)と `ir_ops_executed` を別名で返す。エラー・上限で素朴と完全状態一致。`step()` API は無い
  - core の `Machine` は内部状態を書き換える API を持たず、`MachineSnapshot` は `#[non_exhaustive]`。途中状態からの再開に core を使えない → core では途中状態から再開できない。改版後の `optimized-interpreter` は素朴再生を公開モジュール `replay`(`ReplayState` / `ReplayParts` / `replay`)として、コスト式を `Op::naive_cost` / `Op::static_reach` として公開するため、これらを使う(自前の再実装・式の複製をしない)
  - desktop-gui の拡張点: `ScreenDefinition`(`screens.ts`)、`useWorkspace()`、`commands/mod.rs` / `build.rs` の `AppManifest::commands` / `capabilities/default.json`、DTO は API crate へ追加し ts-rs で生成、共通部品(`ApiError` `ParseErrorDto` `RuntimeErrorDto` `OutputChunkDto` ほか)・Base64・`limits` を再利用、判別子 `kind`、camelCase、`u64` は `number`、コマンド引数は `args` 1 個、UTF-16 位置。「比較はセッションを使わない一括実行」「`DebuggerService` は共有しない」
  - desktop-gui の design は DTO を `dto.rs` 単一ファイルとして定義しつつ、拡張点では `dto/comparison.rs` または `dto_comparison.rs` を許している。当初は `dto_comparison.rs` を採用したが、改版で DTO は新 crate `brainfuck-compare-api` の `dto.rs` に置く(下記の決定)
- **Implications**: 本 spec は新規 crate 3 つ(`brainfuck-wasm`、`brainfuck-compare`、`brainfuck-compare-api`)を `crates/` に足し、Tauri 層とフロントへ「追加のみ」の変更を行う。`brainfuck-gui-api` は変更しない。既存コマンド・既存 DTO・デバッガ画面は不変

### ネイティブ AOT の評価
- **Context**: ブリーフは「WASM を優先して検討し、設計で WASM / AOT の選択を確定」としている
- **Findings**:
  - AOT の現実的な実現は (1) Rust/C のソースを生成して `rustc` / `cc` を起動する、(2) 機械語を直接生成して実行可能メモリへ置いて呼ぶ、の 2 つ
  - (1) は利用者の環境にツールチェーンを要求し、外部プロセス起動と任意の生成ソースのコンパイル・実行を伴う。要件 5.4(外部プロセス不使用)と非目標「任意コード実行」に反する
  - (2) はプロセス内で生成機械語を実行するために `unsafe`(関数ポインタへの変換、実行可能ページ)が不可避。`unsafe` 禁止(steering)、JIT のスコープ外に反する。生成機械語の誤りはメモリ安全性を直接損なう
  - WASM は線形メモリの境界検査と構造化制御フロー、import のみが外部との接点という性質を持ち、生成物の検査(import/メモリ宣言の監査)が機械的にできる
- **Implications**: ネイティブ AOT は不採用。将来の追加は本 spec の範囲外(`Method` 列挙体に方式を足す拡張点のみ設計に残す)

### WASM 実行エンジンの選定
- **Sources Consulted**: wasmi docs.rs(`Config::consume_fuel`、`Store::set_fuel` / `get_fuel`、`Store::limiter`、`TrapCode::OutOfFuel`、`Config::allow_start_fn`、`Config::wasm_*`、`compilation_mode`、`enforced_limits`)、wasmi リポジトリの説明(「efficient, lightweight, deterministic interpreter」、mutable-global・multi-value・bulk-memory 等を対応)、wasm-encoder docs.rs(0.263.0。`Module` / 各 Section / `Function` / `Instruction`)、wasmtime docs(context7: `Config::consume_fuel`、`Store::set_fuel`、`StoreLimitsBuilder::memory_size`)
- **Findings**:

  | 観点 | wasmi(採用) | wasmtime |
  |------|--------------|----------|
  | 実行方式 | インタプリタ(実行時に機械語を生成しない) | Cranelift による JIT/AOT コンパイル(実行時に機械語を生成) |
  | 自コードの `unsafe` | 不要 | 不要(埋め込み API は安全。ただし内部は JIT で実行可能メモリを扱う) |
  | 燃料計測 | `Config::consume_fuel(true)` + `Store::set_fuel` / `get_fuel`。枯渇で `TrapCode::OutOfFuel` | `Config::consume_fuel` + `Store::set_fuel`。枯渇でトラップ |
  | メモリ制限 | `ResourceLimiter` / `StoreLimits`(`Store::limiter`) | `StoreLimitsBuilder::memory_size` ほか |
  | start 関数 | `Config::allow_start_fn(false)` で禁止可能 | 設定可能 |
  | ビルド・サイズ | 軽量(純 Rust、`no_std` 対応) | 重い(Cranelift 一式。デスクトップ配布物とテストのビルド時間が増える) |
  | 性能 | 中(インタプリタ) | 高(ネイティブ近傍) |
  | 決定性 | 高(決定的実行をうたう) | 高 |

  - 比較画面の目的は「方式による差の観察」であり、実行時間の優劣は保証対象ではない。IR のクリア/乗算/走査を生成コード内でループ展開なしに実行できるため、`wasmi` 上でもパターン主体のプログラムでは素朴インタプリタとの差が観察できる
  - 「JIT 非スコープ」「`unsafe` 禁止」「任意コード実行の非目標」との整合は `wasmi` のほうが説明しやすい(機械語を作らない)
- **Implications**: `wasmi` を採用。`wasmtime` は代替案として記録し、エンジン依存を `runtime.rs` 1 ファイルに閉じる。実装時は最新安定版を固定し(調査時点で wasmi 2.0.0、wasm-encoder 0.263.0)、`Config` / `Store` の API 名は実装前に docs.rs で再確認する(`consume_fuel` / `set_fuel` / `limiter` / `allow_start_fn` / `OutOfFuel` は調査時点で存在を確認済み)

### コード生成方式の評価(観測的一致の達成方法)
- **Context**: 完全状態一致(エラー時点のテープ・出力・`dp`・入力位置・実行数・`pc`、命令数上限の操作途中の境界)を、WASM でどう保つか
- **Findings**: 下表。方式 A を採用

  | 方式 | 概要 | 長所 | 短所 |
  |------|------|------|------|
  | A. IR + 事前検査 + 離脱 + ホスト再生(採用) | 操作ごとに「コスト ≤ 残り予算」「範囲内」を検査し、不成立なら操作の先頭状態で離脱。ホストが素朴意味論で終端まで再生 | 生成コードが単純(検査 → 一括適用)。エラー値と状態が再生で構造的に一致。`optimized-interpreter` と同じ論理で、既に確立した正当性論拠を再利用 | 素朴意味論の最小再実装(ホスト側)を 1 つ持つ。差分テストで保護 |
  | B. 命令単位の完全計装 | すべての命令に上限・範囲検査と実行数加算を付ける | 離脱不要で、生成コード内で完結 | IR 最適化の意味がなくなり遅い。コードが大きい |
  | C. 異常時に core で最初から再実行 | 離脱したら core の `Machine` で最初から実行して結果を採用 | 実装が最小 | WASM の誤りを core が隠蔽し、差分テストと一致判定が空洞化する。実行時間が倍。`optimized-interpreter` が却下した方式 |
  | D. 操作ごとに素朴相当の展開コードを生成 | 速い経路と遅い経路の両方を生成 | 離脱不要 | モジュールサイズが倍増。上限(8 MiB)に早く達する |

- **Implications**: 離脱後の再生は終端まで続け、WASM 実行へは戻らない(`optimized-interpreter` と同じ)。再生が不必要に早く起動しても結果は正しい(素朴意味論そのもの)ため、事前検査の誤りは「過剰な離脱 = 性能のみ劣化」と「検査不足 = 正しさの毀損」に分かれ、後者を上限掃引・左右端テストで重点検証する。比較画面は `replay.steps` を表示して離脱の有無を透明にする

### サンドボックス / 資源制限の方針
- **Findings**(層ごとの防御):
  1. **供給元の限定**: 読み込む WASM は本 feature が Brainfuck IR から生成したバイト列のみ。外部供給のモジュールを受け取る API を作らない
  2. **能力の限定**: import は `bf.read` / `bf.write` の 2 つだけ。WASI なし、時刻・乱数・ファイル・ネットワークなし。start 関数なし
  3. **宣言の監査**: 生成直後に `wasmparser` で import 集合・メモリ(1 ページ固定、最大も 1)・テーブルなし・グローバル(インポート/エクスポート)・エクスポート集合・`memory.grow` 命令の不在を検査し、違反なら実行しない(要件 5.3)
  4. **メモリ**: 1 ページ(64 KiB)固定。テープは先頭 30,000 バイト。`ResourceLimiter` で成長を拒否。出力は上限(`MAX_OUTPUT_BYTES` = 64 MiB(比較用ステップ上限以上))で打ち切り、超過は「方式の失敗」
  5. **CPU**: 命令数上限を生成コード内で素朴換算に対して厳密に適用(第一の防御)。さらにエンジンの燃料で `limit × FUEL_PER_STEP + FUEL_BASE + 生成物のバイト数` に上限(コンパイルは Eager とし、遅延翻訳の費用を実行の燃料へ混ぜない)(第二の防御。生成コードの不具合による無限実行を防ぐ。枯渇は「方式の失敗: 監視違反」)
  6. **規模**: モジュールサイズ上限 8 MiB、ループ入れ子 256 段(WASM の制御入れ子とエンジンの上限に収めるため)。超過は「未対応」
  7. **プロセス境界**: 同一プロセス内。WASM の線形メモリはホストメモリから隔離され、境界外アクセスはトラップ
- **Implications**: 比較の上限は 50,000,000 命令(素朴実装で数秒の範囲)。キャンセルは提供せず、画面側の「結果の破棄」のみ(実行は上限で有界)。上限なしの比較は拒否

### 比較画面の実行数・時間の定義
- **Findings**: 仕様書 §12 は「定義が異なる場合は明示」。全方式で `steps_executed`(素朴換算)を正確に出せるため、これを方式間で比較する主列とし、方式固有列を併記する。時間は「準備」(IR 生成、コード生成、監査、ロード)と「実行」(中央値)を分ける。実行時間の大小で優劣や順位を表示しない(誤った一般化を避ける)
- **Implications**: `UnitDto` は `naiveInstructions` / `irOperations` / `none` の 3 種。WASM は `none`(空欄にせず「なし」を明示)

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 3 crate(採用) | `brainfuck-wasm`(生成・監査・隔離実行)、`brainfuck-compare`(観測、方式実行、比較、メトリクス)、`brainfuck-compare-api`(DTO とサービス。`brainfuck-compare` と `brainfuck-gui-api` に一方向で依存) | 中核が Tauri 非依存で `cargo test --workspace` に収まる。責務が変更理由ごとに分離。デバッガ API が重い依存を持たない | crate が 3 つ増える | 採用(改版) |
| 1 crate に全部 | 生成と比較を同一 crate に | 配線が少ない | 生成器(セキュリティ境界)と比較器(UI 向け集計)の変更理由が異なる。テストが肥大 | 却下 |
| DTO・サービスを gui-api に追加 | `brainfuck-gui-api` に比較用 DTO とサービスを加える | crate が増えない | デバッガ API が `brainfuck-compare` → `brainfuck-wasm` → `wasmi` などの重い依存を持ち、デバッガのビルド・テストに波及する | 却下(ユーザー決定: spec は分割せず crate で分離) |
| 比較ロジックを gui-api に直接 | DTO 変換と比較を一体化 | crate が増えない | DTO なしで比較を再利用/テストできない。観測比較が Tauri 向け crate に埋もれる | 却下 |
| 新 Tauri プラグイン | 比較をプラグイン化 | 疎結合 | 過剰。拡張点の表が示す追加方式(コマンドモジュール追加)で足りる | 却下 |

## Design Decisions

### Decision: 採用方式は WASM(ネイティブ AOT は不採用)
- **Context**: ブリーフが設計での確定を要求。非目標「任意コード実行」、`unsafe` 禁止、JIT スコープ外
- **Alternatives Considered**: ネイティブ AOT(外部ツールチェーン / 機械語生成)、WASM(wasmi / wasmtime)
- **Selected Approach**: WASM(生成は `wasm-encoder`、実行は `wasmi`)
- **Rationale**: 実行範囲が import に限定され、生成物を機械的に監査でき、`unsafe` と外部プロセスが不要
- **Trade-offs**: ネイティブ速度は出ない(観察対象としては許容)
- **Follow-up**: 将来の方式追加は `Method` の拡張で行う

### Decision: 実行エンジンは wasmi、差し替え境界は `runtime.rs`
- **Selected Approach**: `wasmi` を使い、エンジンの型(`Engine` `Store` `Linker`)は `runtime.rs` の外へ漏らさない。公開型は `WasmProgram` / `WasmRun` / `WasmError`
- **Rationale / Trade-offs**: 上記の比較表のとおり。性能は劣るが、機械語を生成しない点が方針に最も合致する
- **Follow-up**: 実装時に API 名を docs.rs で再確認。wasmtime への切り替えは別 spec / 改版の判断

### Decision: 離脱 + 再生の方式(A)、再生は上流の公開 `replay` を使う(改版)
- **Selected Approach**: 離脱時に線形メモリのテープ等を `ReplayParts` に詰め、`ReplayState::from_parts` → `replay(program, Some(limit), pc, &mut state)` → `into_parts` で読み戻す。`ReplayStateError` は `RunError::InvalidState` に写す。自前の再生(`replay.rs`)は持たない
- **Rationale**: `optimized-interpreter` が素朴再生を公開(改版)したため、素朴意味論の再実装の複製とずれのリスクがなくなる。再生の正当性(core との一致)は上流の `tests/replay.rs` が検証し、本 spec は WASM 側の差分テスト(`tests/vs_core.rs` と `brainfuck-compare`)で WASM 全体を検証する
- **Trade-offs**: 上流 `replay` の署名・意味論に依存する(Revalidation Trigger)。離脱時にテープ 30,000 バイトと入力のコピーが 1〜2 回発生する(離脱時のみ)

### Decision: 比較は必ず有限上限(最大 50,000,000)、キャンセル非提供
- **Context**: 仕様書 §10「GUI の実行はキャンセル可能」と非目標「無制限の計算資源」
- **Selected Approach**: 比較は上限を必須とし最大 50,000,000 命令。最適化 / WASM は途中停止 API を持たないため、協調的キャンセルは設けず、画面側で結果を破棄して操作を戻す(バックエンドは上限で有界時間に完了し、後続の比較は直列化される)
- **Rationale**: 比較は短時間の一括実行であり、中断可能性より有界性が安全上の要点。デバッガ画面の実行(`desktop-gui`)のキャンセル可能性は変更しない

### Decision: WASM の方式固有実行単位は「なし」
- **Alternatives Considered**: IR 操作回数を数える(カウンタ加算で生成コードが遅くなり、最適化の値と常に同一で情報が増えない)、エンジン燃料(エンジン・版に依存し学習材料として不安定)
- **Selected Approach**: `UnitDto::none`。素朴換算の実行数は全方式で正確に報告する

### Decision: 通信は新規コマンド 2 本(`get_comparison_info`、`run_comparison`)
- **Rationale**: 上限値などの定数を TypeScript に複製せず、バックエンドを単一の真実とする(desktop-gui の「フロントは内部型を複製しない」と整合)。比較はセッションを持たない一括実行のため、状態を持つコマンドは不要

### Synthesis
- **Generalization**: 「方式を実行して `Observation` を得る」という共通の形に 3 方式を揃え、一致判定・メトリクス・差分テストを方式非依存にする。`Method` 列挙体の追加で将来の方式を足せる(インターフェースのみ一般化。実装は 3 方式のみ)
- **Build vs Adopt**: WASM バイナリ生成は `wasm-encoder`、実行は `wasmi`、監査は `wasmparser` を採用(手書きのバイナリエンコーダ・インタプリタは作らない)。PBT / ベンチフレームワークは採用せず、固定シードの xorshift(上流と同じ)。ダイジェストは外部ハッシュ crate を使わず FNV-1a 64 を自前実装(表示用の短い指紋であり暗号用途ではない)
- **Simplification**: 方式の選択 UI は「最適化」「WASM」の 2 つのチェックのみ。素朴は常に実行。結果の保存・履歴・エクスポートなし。キャンセルなし。エンジン抽象 trait は設けない(実装は 1 つ)

## Risks & Mitigations
- 生成コードの事前検査の不足(適用してはならない操作を適用)で完全状態一致が崩れる — 上限掃引(0..N+2)・左右端・パターン近傍の差分テストを `brainfuck-compare` に置き、実行時にも毎回オラクルと一致判定する(不一致はユーザーに見える)
- 上流 `replay` の署名・意味論の変更、または状態の受け渡しの不整合 — Revalidation Trigger とし、`ReplayStateError` は `RunError::InvalidState`(方式の失敗)に写して素朴のエラーに偽装しない。意味論自体は上流が core との一致を検証し、本 spec の全差分テストが再生を経由する
- 燃料の上限(`FUEL_PER_STEP`)が過小で正当な実行が枯渇 — 操作ごとの最悪 WASM 命令数/素朴換算コストの比が定数で抑えられることを設計で示し(`design.md`)、最悪ケース(長い本体の乗算、長い走査)のテストで枯渇しないことを検証
- `wasmi` / `wasm-encoder` / `wasmparser` の API 変更・版不整合 — 版を固定し、実装タスクの最初に依存の疎通(最小モジュールの生成 → 監査 → 実行)を確認する
- 大きなモジュールの生成時間・メモリ — サイズ上限 8 MiB、ループ入れ子 256 で未対応扱い。ソース上限は desktop-gui の 1 MiB を流用
- 巨大出力(最大 50 MB × 方式数)のメモリ — 基準の出力を保持して順に比較し、比較後に破棄する。応答には末尾 64 KiB と長さ・ダイジェストのみ
- 実行時間の揺らぎでテストが不安定になる — 時間は一切アサートしない(要件 8.7, 13.11)
- core の `Machine::input_position()` に依存(`optimized-interpreter` の差分テスト設計も同じ前提) — 提供されない場合は上流の改版が必要。実装タスクの先頭で存在を確認する

## References
- `.kiro/specs/optimized-interpreter/design.md` — IR(`OpKind`)、実行数の 2 定義、完全状態一致、`FastPath` / `Fallback`
- `.kiro/specs/optimized-interpreter/research.md` — 実行数の閉形式表
- `.kiro/specs/desktop-gui/design.md` — 「Extension Points for compiler-comparison」、共通 DTO、`Workspace`、`screens.ts`
- `brainfuck-studio-development-specification.md` §3, §10, §11, §12
- wasmi (docs.rs/wasmi, github.com/wasmi-labs/wasmi) — インタプリタ、燃料計測、`ResourceLimiter`
- wasm-encoder (docs.rs/wasm-encoder) — モジュール生成 API
- Wasmtime docs (`Config::consume_fuel`、`StoreLimitsBuilder`) — 代替案の比較根拠
