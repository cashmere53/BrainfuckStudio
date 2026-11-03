# Research & Design Decisions: brainfuck-core

## Summary
- **Feature**: `brainfuck-core`
- **Discovery Scope**: New Feature(greenfield、ただし外部依存なしのため軽量な発見)
- **Key Findings**:
  - 仕様書 §4〜§6 が公開 API・意味論を既に詳細に規定しており、設計上の自由度は「エラー型の形」「`run` とステップ制限の関係」「Program のオフセット保持方法」「テスト配置」に限られる
  - 外部 crate は不要。エラーの `Display` / `Error` は手書きで 30 行程度であり、`thiserror` を入れる利点が小さい
  - 開発環境は rustc/cargo 1.99.1(stable)。標準機能(const generics の配列 `Debug` / `PartialEq`、`[lints]` の workspace 継承)のみで要件を満たせる

## Research Log

### 仕様書と要件の整合確認
- **Context**: `StopReason::StepLimitReached` を `run()` が返すのか、`StepLimitExceeded` エラーで返すのかが brief と仕様書で読み方が分かれる
- **Sources Consulted**: `brainfuck-studio-development-specification.md` §6, §7, §10, §12-5; `brief.md`
- **Findings**: §6 は `step()` が `StepLimitExceeded { limit, pc }` を返すと規定。§12-5 は上限到達時「明確なエラーを返す」。§7 は終了コード 3 = 実行エラー。§6 の `StopReason` に `StepLimitReached` があるが `run()` の説明は「正常終了では `Halted` を返す」のみ
- **Implications**: `run()` は上限到達をエラーで返す(§12-5 を優先)。`StepLimitReached` は型として存在し、debugger-state のバッチ実行(バッチ命令数に達した場合の通知)などに使える予約値とする。この決定は design.md の Machine 契約に明記し、Revalidation Triggers に含めた

### 外部依存(エラー表現)
- **Context**: エラー型の `Display` / `Error` 実装の手段
- **Sources Consulted**: Rust 標準ライブラリのドキュメント(`std::error::Error`、`core::fmt::Display`)。実装方針は既知の安定 API のみのため追加の Web 調査は不要
- **Findings**: 変種は 2 + 2 個と少なく、手書き実装で十分。外部 crate を足すと core の依存ゼロという制約(Tauri/stdin/fs/UI 非依存、安全性監査の容易さ)が崩れる
- **Implications**: 依存ゼロを採用。`Allowed Dependencies` に明記

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 値型 `Program` + 状態機械 `Machine`(採用) | 解析結果を不変値、実行状態を別型に分離 | 仕様書 §4 と一致、後続 spec が `Program` を共有可能、最適化側は `Program` を入力にできる | `Machine` が `Program` を所有するため複数 Machine 間の共有に clone が必要 | 共有が必要になれば後続で `Arc<Program>` 受け入れを後方互換で追加可能 |
| I/O trait(`Read` / `Write`)を `Machine` に注入 | 入出力を抽象化 | ストリーミング出力に対応 | core が I/O に結合、出力順序・EOF 判定のテストが複雑化、仕様書 §2「バイト列で扱う」と不一致 | 不採用 |
| 括弧を実行時に探索 | jump_table を持たない | 実装が簡単 | 仕様書 §4/§12-4 に違反、性能劣化 | 不採用 |

## Design Decisions

### Decision: `run()` はステップ上限を `Err(StepLimitExceeded)` で返す
- **Context**: `StopReason::StepLimitReached` の役割
- **Alternatives Considered**:
  1. `run()` が `Ok(StopReason::StepLimitReached)` を返す
  2. `run()` が `Err(RuntimeError::StepLimitExceeded)` を返す(採用)
- **Selected Approach**: 2。`StepLimitReached` は予約のみ
- **Rationale**: §12-5「位置を含む明確なエラーを返す」、§7 終了コード 3 と整合。`step()` と `run()` のエラー種別が一致し、差分テスト(11.8)が単純になる
- **Trade-offs**: `StepLimitReached` は core 内で未使用になる。debugger-state が利用する前提の予約
- **Follow-up**: debugger-state spec のバッチ実行設計時に `StepLimitReached` の意味(バッチ終端)を確定する

### Decision: ソースオフセットは `Program` の並列配列で保持し、`Instruction` は純列挙体のままにする
- **Context**: brief は「Instruction(ソースバイトオフセット付き)」、仕様書 §4 は `enum Instruction`(8 バリアント)と `program.instructions().len()`
- **Alternatives Considered**:
  1. `struct Op { kind: Instruction, offset: usize }` の `Vec<Op>` を `instructions()` が返す
  2. `Vec<Instruction>` + `Vec<usize>`(オフセット)(採用)
- **Selected Approach**: 2。`source_offset(index)` / `source_offsets()` で問い合わせ
- **Rationale**: 仕様書 §4/§5 の `Instruction` 列挙体と `instructions()` の型を保てる。最適化 IR 側が `Instruction` を再利用しやすい。オフセットは添字 i で引くだけ
- **Trade-offs**: 2 配列の長さ一致が不変条件(構築を `parse` に限定して保証)

### Decision: `MachineConfig` に `Default` を実装しない
- **Context**: steering は「命令数制限を既定で有効」。ただし既定値(10,000,000)は CLI の責務
- **Selected Approach**: `unlimited()` / `with_max_steps(n)` の明示コンストラクタのみ。`Default` なし
- **Rationale**: 暗黙に無制限になる事故、または core が CLI 既定を所有してしまう境界違反を避ける
- **Follow-up**: CLI spec が 10,000,000 を定数として保持する

### Decision: 判定順序は「停止 → 上限 → 実行」
- **Rationale**: 仕様書 §6「停止済みなら Halted、制限値に到達済みなら次の命令を実行せずエラー」の文順と、ちょうど上限で終了するプログラムが成功するという自然な境界の両立

### Decision: `#[non_exhaustive]` の適用範囲
- `ParseError` / `RuntimeError` / `StopReason` / `MachineSnapshot` に付与(debugger-state が後方互換で拡張するため、要件 1.6)。`StepResult`(仕様書で 2 値固定)、`MachineConfig`(リテラル構築が利便)、`Instruction`(8 命令固定)には付与しない
- トレードオフ: 下流の `match` はワイルドカード腕が必要になる

### Synthesis 結果
- **Generalization**: 「状態不変のエラー」「停止後の冪等」は共通の検証→更新の 2 段構造で一般化。個別の特例分岐を作らない
- **Build vs Adopt**: エラー実装・括弧スタック・テープ(`Box<[u8; N]>`)はすべて標準ライブラリで足りる。採用外部 crate なし
- **Simplification**: `halted` フラグを持たず `pc == len` から導出。I/O trait・イベント通知・フックは作らない。`MachineConfig` は 1 フィールドのみ

## Risks & Mitigations
- `StepLimitReached` の意味が下流で解釈ずれする — design.md に「core は返さない」と明記し、Revalidation Triggers に追加
- 無制限設定 + 無限ループで `run()` が終了しない — ドキュメントコメントで警告し、CLI/GUI が上限を設定する責務を spec 間の契約として記録
- `non_exhaustive` による下流の `match` 破綻 — 意図的。ワイルドカード腕の書き方を debugger-state 側で採用する

## References
- `brainfuck-studio-development-specification.md` §4〜§6, §10〜§12 — 意味論と受け入れ基準の正
- `.kiro/steering/{product,tech,structure}.md` — 不変条件と構成
- `.kiro/steering/roadmap.md` — 下流 spec の依存関係
