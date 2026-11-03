# Research & Design Decisions: debugger-state

## Summary
- **Feature**: `debugger-state`
- **Discovery Scope**: Extension(`brainfuck-core` の公開 API の後方互換な拡張。軽量ディスカバリ)
- **Key Findings**:
  - `brainfuck-core` の公開 API(`Machine::step` / `snapshot` / `tape` / `output` / `program`、`Program::source_offsets`、`Machine: Clone`、`Program: Clone`)だけで、ブレークポイント・バッチ・キャンセル・リセット・表示窓・位置変換を**既存コードを一切変更せず**に実装できる。変更が必要なのは `lib.rs` の `mod` 宣言と re-export のみ
  - core の `run()` は `StepLimitReached` / `Breakpoint` / `Cancelled` を返さず、上限到達は `Err(RuntimeError::StepLimitExceeded)`。デバッガ層は `step()` を自前でループし、これら 3 つの停止理由を生成する。`StepLimitReached` は core の予約意図(バッチ上限到達の通知)に従い「バッチ命令数に到達」の意味に限定し、機械の命令数上限はエラーのまま扱う
  - `Program` はソース文字列を保持しない(命令ごとのバイトオフセットのみ)。行・列・UTF-16 オフセットへの変換には、解析に用いたソース文字列から一度だけ構築する位置変換情報(`SourceMap`)が必要
  - `brainfuck-core` はこの時点で**仕様のみ**(`crates/` 未作成)。本 spec の実装は core の実装タスク完了が前提

## Research Log

### 既存 core の拡張点
- **Context**: 後方互換のまま停止理由・ブレークポイント・バッチ・位置情報を追加する方法の確認
- **Sources Consulted**: `.kiro/specs/brainfuck-core/design.md` / `requirements.md`、`brainfuck-studio-development-specification.md` §6, §8
- **Findings**:
  - 公開型 `ParseError` / `RuntimeError` / `StopReason` / `MachineSnapshot` は `#[non_exhaustive]`。ただし本 spec は**それらの定義を変更する必要が無い**(`StopReason` の 4 バリアントは既存、`MachineSnapshot` は読み取りのみ)
  - `Machine::step` は「停止判定 → 上限判定 → 実行」の順で、エラー時は状態不変。バッチループはこの契約をそのまま利用できる
  - `Machine` は `Clone` / `PartialEq` を持ち、リセットは `Machine::new(program.clone(), input.clone(), config)` で実現できる(セッションが入力と設定を保持する)
  - `MachineSnapshot` は `#[non_exhaustive]` のため外部 crate から構造体リテラルで生成できない。本 spec は core 内部モジュールとして追加するため問題ないが、`BatchReport` などは `snapshot()` の戻り値をそのまま格納する(再構築しない)
  - `pc` の取得手段は `snapshot().pc` のみ。1 命令ごとに `snapshot()`(数個のフィールドコピー)を呼ぶ。1,000 命令バッチでは無視できる
- **Implications**: 新規モジュールを追加するだけで済み、core の既存ファイルは `lib.rs` の宣言以外触れない。core の既存テストは無変更で通る(要件 1.1, 11.7)

### 配置: 同一 crate か別 crate か
- **Context**: 「core 非依存制約を維持」「core を後方互換で拡張」(brief, roadmap)
- **Alternatives Considered**:
  1. `brainfuck-core` 内の新規モジュール
  2. 新 crate `brainfuck-debugger`(core に依存)
- **Selected Approach**: 1(同一 crate 内の新規モジュール)
- **Rationale**: roadmap と steering が「debugger-state が追加する API は core を後方互換で拡張する」と規定。`StopReason` を含む型を単一 crate の公開 API として desktop-gui に渡せる。依存方向は一方向のままで、既存モジュールは新モジュールを import しない。追加依存はゼロ
- **Trade-offs**: core crate の公開面積が増える(新規 API は追加のみ)。別 crate より境界が緩いが、`pub(crate)` と import 方向の規約(下記)で既存モジュールの保護を機械的に確認できる
- **Follow-up**: desktop-gui spec で、DTO 変換を core 外(`apps/desktop` の Rust 側)に置くことを確認

### `StepLimitReached` の意味
- **Context**: 要件 2.3。仕様書 §8 の「バッチ末尾、停止、ブレークポイント、エラー時に状態を通知」
- **Alternatives Considered**:
  1. バッチ命令数到達を `StepLimitReached` とする(core の予約意図どおり)
  2. 新バリアント(例 `BatchBoundary`)を `StopReason` に追加する
- **Selected Approach**: 1。ドキュメントで「機械の命令数上限(`max_steps`)の超過とは別物」と明記し、機械の上限は `Err(RuntimeError::StepLimitExceeded)` のまま
- **Rationale**: brief の停止理由一覧(`Halted, StepLimitReached, Breakpoint, Cancelled`)と core の予約文書に一致。新バリアントは core の型定義変更を伴い、本 spec の「既存コード無変更」方針に反する
- **Trade-offs**: 名称がやや紛らわしい。GUI は「バッチ到達 = 継続可能」「`Err(StepLimitExceeded)` = 致命的エラー」として別扱いできる
- **Follow-up**: desktop-gui の DTO では表示ラベルを分ける(例: "batch" と "step limit exceeded")

### ブレークポイントの「再開時前進」規則
- **Context**: 要件 5.3〜5.7。「停止前に現在 `pc` を照合」を素朴に実装すると、`Breakpoint` で停止した位置から再開すると同じ位置で永久に停止する。一方で「再開時は無条件に最初の命令を照合しない」とすると、バッチ境界ちょうどのブレークポイントを見逃す
- **Alternatives Considered**:
  1. 各バッチ呼び出しの最初の命令だけ照合を省略する(見逃しが発生するため不採用)
  2. 位置を記憶する(`Option<usize>` の「承認済み pc」)
  3. セッションが 1 ビットの「次の 1 命令は照合を省略する」フラグを持つ
- **Selected Approach**: 3。フラグは (a) `Breakpoint` で停止した時、(b) Step Into が命令を実行した後に立て、(c) 命令が 1 つ実行された時・リセット時に下ろす
- **Rationale**: 位置を持たないため単純で、「ユーザーが既に観察した位置では再停止しない」という標準的なデバッガ(gdb の continue)の挙動に一致する。バッチ到達(`StepLimitReached`)ではフラグを立てないので境界の見逃しが無い
- **Trade-offs**: `Cancelled` では立てない(ブレークポイント位置で中断した場合、再開後に `Breakpoint` が通知される。要件 6.9)
- **Follow-up**: ループ内のブレークポイント(反復ごとに停止、5.5)を差分テストで確認

### キャンセル指示の形
- **Context**: 要件 6.1, 6.8。「呼び出し側が供給するフラグ/トークン」で、core はスレッド・UI 非依存、テストは決定的
- **Alternatives Considered**:
  1. `&AtomicBool` を直接受け取る
  2. 小さな trait `CancelSignal { fn is_cancelled(&self) -> bool }` を定義し、`AtomicBool` / `Arc<T>` / `&T` / 無効(`NeverCancel`)/ クロージャラッパ(`CancelFn`)に実装する
  3. コールバック関数のみ
- **Selected Approach**: 2。クロージャは `CancelFn<F>` ラッパで包む(`F: Fn() -> bool` の包括実装と `AtomicBool` 実装のコヒーレンス衝突の懸念を避ける)
- **Rationale**: `AtomicBool` は std で外部 crate 不要・スレッド生成不要。trait により、テストは `Cell` カウンタを使った `CancelFn` で「N 回確認後に有効」を決定的に再現できる。`NeverCancel` により呼び出しを 1 つの API に集約できる
- **Trade-offs**: 命令ごとに確認する(`AtomicBool::load(Acquire)` は x86/ARM でほぼ無償)。ジェネリクスで静的ディスパッチにし、`?Sized` を許可して `&dyn CancelSignal` も受け付ける
- **Follow-up**: 設定側(desktop-gui)は `Release` または `SeqCst` で store する(ドキュメントに明記)

### バッチ命令数 0 の扱い
- **Selected Approach**: `BatchSize(NonZeroU64)` 新型。`BatchSize::new(0) == None`。不正値を型で作れなくし、実行時エラー型を増やさない(要件 4.1, 4.2)
- **Rationale**: 命令数 0 は「何もせず StepLimitReached」という無意味な状態を生む。型による排除が最小

### ブレークポイント集合の表現
- **Alternatives Considered**: `BTreeSet<usize>` / `Vec<bool>`+件数
- **Selected Approach**: プログラム長の `Vec<bool>` と件数。判定は O(1)、件数 0 なら探索省略。一覧は添字を昇順に走査(要件 3.7)。範囲外はコンストラクタで保持した命令数と比較してエラー(3.3)
- **Rationale**: 最も頻繁な操作(命令ごとの照合)を O(1) にする

### テープ表示窓の端の扱い
- **Alternatives Considered**: (a) 端で切り詰める(幅が縮む)、(b) 幅を一定に保つため窓をずらす、(c) 範囲外をパディング
- **Selected Approach**: (a)。要件 8.2/8.4 が「`dp ± 半径` を有効範囲に切り詰める」と定義。先頭添字と窓内ポインタ位置を返すので UI は列見出しを描ける
- **Rationale**: (b) はポインタが窓中央から外れ、(c) は存在しないセルを表現する。(a) が最も定義が単純で状態の事実だけを返す

### ソース位置の単位(行・列・UTF-16)
- **Context**: 要件 9。CodeMirror 6 の文書位置は JavaScript 文字列の UTF-16 コード単位オフセットで、行区切りは既定で `\n` / `\r\n` / `\r`(`lineSeparator` 未設定時)。Rust のバイトオフセットとは一致しない(GUI spec で最終確認)
- **Alternatives Considered**: (a) バイトオフセットのみ返しフロントで変換、(b) 行・列のみ、(c) バイト・行・列(スカラ値)・UTF-16 の全部
- **Selected Approach**: (c)。位置 1 つにつき 4 値。構築時に 1 パスで全命令分を事前計算(`Vec<SourceRange>`)し、問い合わせは O(1)(逆引きは二分探索)
- **Rationale**: UTF-16 変換をフロントに任せるとマルチバイト・サロゲートペアで強調位置がずれる典型バグになる。core 側で一度だけ正しく計算し、テストで固定する。列はスカラ値個数(エラー表示向け)。CLI の診断用にも使える
- **Follow-up**: 結合文字・グラフェムクラスタは考慮しない(列はスカラ値個数と明記)

### 位置変換情報とプログラムの整合検証
- **Context**: `Program` はソースを保持しない。誤ったソースを渡されると強調位置がずれる
- **Selected Approach**: 構築時に、ソース上の全 ASCII 命令文字が `Program` の命令列・オフセットと 1 対 1 で一致するかを検証し、不一致は `SourceMapError`(数値のみ)。構築コスト O(ソース長)
- **Rationale**: 誤用を実行時のずれではなく構築時の失敗にする(要件 9.2, 9.13)

### ソース位置からのブレークポイント正規化
- **Selected Approach**: 純粋関数 `instruction_at_or_after_offset` と `first_instruction_on_line` を `SourceMap` に置き、結果の命令添字を `BreakpointSet::insert` に渡す。セッションは `SourceMap` を保持しない
- **Rationale**: セッションと位置変換情報を結合すると、ソース差し替えのたびにセッション再生成が必要になり責務が混ざる。2 手順に分けても呼び出しコードは 2 行で、テストで合成を検証できる

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| core にデバッガ状態を追加(`Machine` にフィールド追加) | `Machine` がブレークポイント・フラグを保持 | 呼び出しが 1 型 | 既存型の変更、`Machine: PartialEq` や core テストへの影響、意味論の汚染 | 不採用 |
| 外側ラッパ `DebugSession`(採用) | `Machine` を所有し `step()` をループ | 既存コード無変更、core の意味論に触れない | 命令ごとに `snapshot()` 呼び出しのオーバーヘッド | 採用 |
| 別 crate | core に依存する独立 crate | 境界が物理的に明確 | roadmap の「core を拡張」と不一致、型再エクスポートが必要 | 不採用 |

## Design Decisions

### Decision: 既存 core ファイルを変更しない追加のみの拡張
- **Context**: 要件 1.1, 1.8, 11.7
- **Alternatives Considered**:
  1. `Machine` に `step_until` / `run_with` を追加(パフォーマンス最適)
  2. 外側ラッパのみ(採用)
- **Selected Approach**: 新規 5 モジュール(`breakpoint` / `cancel` / `view` / `source_map` / `session`)と `lib.rs` の宣言のみ
- **Rationale**: core はオラクルであり最小変更が最優先。性能が問題になった場合のみ、後続で `Machine::pc()` 等の小さな追加を core spec の Revalidation として別途行う
- **Trade-offs**: `step()` の公開 API 経由のため、将来最適化の余地を残す
- **Follow-up**: 10,000,000 命令級の GUI 実行で性能が問題なら計測して判断(ベンチマークは正規テストと分離)

### Decision: エラーは `Result<StopReason, RuntimeError>` を `BatchReport` に包む
- **Context**: 要件 4.4, 4.8(エラー時にもそのバッチの実行命令数と到達状態を通知)
- **Selected Approach**: `BatchReport { stop: Result<StopReason, RuntimeError>, executed: u64, snapshot: MachineSnapshot }`。関数自体は失敗しない
- **Rationale**: `Err` を関数の戻り値にすると、エラー時に `executed` と `snapshot` を別経路で取得する必要があり、GUI の「状態を通知」が割れる

## Risks & Mitigations
- core が未実装 — 本 spec の実装タスクは core の公開 API 実装完了を前提とし、tasks.md の先頭に前提確認タスクを置く
- 命令ごとの `snapshot()` 呼び出しによる性能 — 1,000 命令バッチでは影響が小さい。問題が出たら core に小さな追加(後方互換)を提案する
- `StepLimitReached` の名称の紛らわしさ — ドキュメントとテスト名で「バッチ到達」と明記、DTO でラベルを分ける
- 位置変換の誤用(別ソースを渡す) — 構築時に整合検証して失敗させる
- キャンセル指示のメモリ順序 — `Acquire` ロードを文書化し、設定側は `Release`/`SeqCst` を使う

## References
- `brainfuck-studio-development-specification.md` §6(公開 API), §8(デバッガ仕様)
- `.kiro/specs/brainfuck-core/design.md` — 公開 API とステップ制限の規約、`StopReason` の予約方針
- CodeMirror 6 のドキュメント位置は UTF-16 コード単位(既知の仕様。desktop-gui spec で再確認)
