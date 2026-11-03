# Brainfuck Studio 開発仕様書

**版:** 1.0  
**対象:** Rust コア／CLI（Phase 1）、Tauri デスクトップアプリ（Phase 2以降）  
**目的:** Brainfuck を正確に実行し、実行過程を観察・比較できる学習用スタジオを段階的に開発する。

## 1. 目的と範囲

Brainfuck Studio は、次の三つを同じ基盤で提供する。

1. 正確でテスト可能な Brainfuck 実行エンジン
2. CLI による再現可能な実行環境
3. ソース、命令位置、テープ、出力を可視化するデバッグ GUI

Phase 1 の成果物は Rust workspace 内の `brainfuck-core` と `brainfuck-cli` である。GUI、最適化、WASM／ネイティブコンパイラは Phase 1 の完了条件に含めない。ただしコアの公開 API はそれらから再利用できる形にする。

非目標: Brainfuck 方言を無制限に受け入れること、任意コード実行、無制限の計算資源を提供すること、Phase 1 で JIT/AOT を実装すること。

## 2. 技術構成とリポジトリ

```text
brainfuck-studio/
├── Cargo.toml                    # workspace
├── crates/
│   ├── brainfuck-core/
│   │   ├── src/{lib,instruction,parser,machine,error}.rs
│   │   └── tests/
│   └── brainfuck-cli/
│       └── src/main.rs
├── apps/
│   └── desktop/                  # Phase 2: Tauri 2 + React + TypeScript + Vite
├── examples/
│   └── hello.bf
└── docs/
```

| 層 | 採用技術 | 責務 |
|---|---|---|
| 実行エンジン | Rust | 構文解析、状態機械、I/O 境界、実行制限 |
| CLI | Rust | ファイル入力、標準入出力、終了コード |
| デスクトップ | Tauri 2 | Rust コアと UI の境界、ローカルファイル操作 |
| UI | React + TypeScript + Vite | 画面状態と操作 |
| エディタ | CodeMirror 6 | 編集、現在命令位置、括弧対応、ブレークポイント |

`brainfuck-core` は Tauri、標準入力、ファイルシステム、UI フレームワークに依存してはならない。I/O は呼び出し元が供給する入力バイト列と取得する出力バイト列で扱う。

## 3. フェーズ計画

| フェーズ | 内容 | 完了成果 |
|---|---|---|
| 1 | 基本インタプリタと CLI | 正しい8命令、ステップ API、テスト、Hello World |
| 1.5 | デバッグ状態の整備 | スナップショット、停止理由、ブレークポイントに使える位置情報 |
| 2 | デスクトップ GUI | CodeMirror 6、実行・停止・ステップ、テープと出力の可視化 |
| 3 | 最適化インタプリタ | IR、連続命令統合、代表的なループ最適化、比較ベンチマーク |
| 4 | コンパイラ比較 | WASM とネイティブ AOT の少なくとも一方、同一プログラムの比較画面 |

Phase 3/4 の最適化・コンパイル結果は Phase 1 の意味論と完全に同じ観測可能な結果（出力、最終テープ、エラー）を返さなければならない。

## 4. Phase 1 モジュール設計

### `instruction`

```rust
pub enum Instruction { MoveRight, MoveLeft, Increment, Decrement, Output, Input, LoopStart, LoopEnd }
```

命令は有効文字そのものを一つずつ表す。ソース上のバイトオフセット（将来の行・列変換用）を各命令と対応付ける。

### `parser`

`Program::parse(&str) -> Result<Program, ParseError>` を提供する。`><+-.,[]` 以外はコメントとして無視する。解析結果には命令列と `jump_table: Vec<usize>` を含める。`[` は対応する `]` の命令添字、`]` は対応する `[` の命令添字を指す。

解析時はスタックで `[` の添字を保持する。余分な `]` はその位置で `UnmatchedClosingBracket`、解析終了時に残る `[` は最初に未解決の位置で `UnmatchedOpeningBracket` とする。実行中に括弧対応を探索してはならない。

### `machine`

`Machine` は一つの `Program` の実行状態を保持する。初期化時、テープは 30,000 個すべて 0、命令ポインタ（`pc`）とデータポインタ（`dp`）、実行済み命令数は 0、出力は空、入力読取位置は 0 とする。

### `error`

パース、実行、CLI のエラーを型として区別する。エラー表示は原因、命令添字、可能ならソース位置を含む。

## 5. 命令意味論

テープは `u8[30000]`、有効な `dp` は `0..=29999`、セル演算は `wrapping_add`／`wrapping_sub` とする。各 `step` は実行した命令 1 個につき実行回数を 1 増加する。

| 命令 | 意味 | `pc` の遷移 |
|---|---|---|
| `>` | `dp += 1`。29999 ではエラー | 成功時 `+1` |
| `<` | `dp -= 1`。0 ではエラー | 成功時 `+1` |
| `+` | 現在セルを 1 加算、255 の次は 0 | `+1` |
| `-` | 現在セルを 1 減算、0 の次は 255 | `+1` |
| `.` | 現在セルを出力バイト列末尾へ追加 | `+1` |
| `,` | 次の入力バイトを現在セルへ格納。尽きたら 0 | `+1` |
| `[` | 現在セルが 0 なら対応する `]` の次、非0なら次命令 | 条件による |
| `]` | 現在セルが非0なら対応する `[` の次、0なら次命令 | 条件による |

プログラム終了は `pc == program.instructions().len()` である。空プログラムは初期状態のまま正常終了する。範囲外移動は状態を変更せず `PointerOutOfBounds { pc, dp, direction }` を返す。

## 6. 実行状態と公開 API

具体的な内部表現は変更可能だが、少なくとも次の概念を公開する。

```rust
pub struct MachineConfig {
    pub max_steps: Option<u64>, // None は制限なし。CLI/GUI の既定値は設定する
}

pub enum StepResult { Executed, Halted }

pub enum StopReason { Halted, StepLimitReached, Breakpoint, Cancelled }

pub struct MachineSnapshot {
    pub pc: usize,
    pub data_pointer: usize,
    pub current_cell: u8,
    pub steps_executed: u64,
    pub halted: bool,
}

impl Machine {
    pub fn new(program: Program, input: Vec<u8>, config: MachineConfig) -> Self;
    pub fn step(&mut self) -> Result<StepResult, RuntimeError>;
    pub fn run(&mut self) -> Result<StopReason, RuntimeError>;
    pub fn snapshot(&self) -> MachineSnapshot;
    pub fn tape(&self) -> &[u8];
    pub fn output(&self) -> &[u8];
}
```

`step()` は停止済みの機械では `Ok(Halted)` を返し、状態を変更しない。制限値に到達済みなら次の命令を実行せず `StepLimitExceeded { limit, pc }` を返す。`run()` は `step()` を繰り返し、正常終了では `StopReason::Halted` を返す。`Breakpoint` と `Cancelled` は Phase 1.5 で導入してよいが、将来の UI 契約として予約する。

入力・出力はバイト列であり、UTF-8 テキストであることを前提にしない。GUI は表示用に UTF-8 解釈と 16 進表示を切り替えられるようにする。

## 7. CLI 仕様（Phase 1）

```text
brainfuck-cli [OPTIONS] <PROGRAM>

<PROGRAM>              Brainfuck ソースファイル
--input <PATH>          入力バイト列を読むファイル
--max-steps <N>         最大実行命令数（省略時: 10,000,000）
--no-step-limit         命令数制限を無効化
--dump-state            終了後に pc、dp、steps を標準エラーへ表示
--help                  使用方法
```

標準出力にはプログラム出力バイト列だけをそのまま書き込む。診断、状態表示、使用方法は標準エラーへ書く。`--input` がない場合は標準入力を EOF までバイトとして読む。終了コードは `0`=正常終了、`2`=引数またはパースエラー、`3`=実行エラー、`4`=I/O エラーとする。

`examples/hello.bf` は `Hello World!\n` を出力する。README の検証例は `cargo test --workspace` と `cargo run -p brainfuck-cli -- examples/hello.bf` を含める。

## 8. デバッガ仕様（Phase 1.5 / 2）

デバッガの単位は「実行命令」でありソース文字ではない。無視されたコメント文字上には停止しない。UI は命令添字からソース範囲へ変換して現在位置をハイライトする。

必須操作は Run、Pause、Step Into、Reset。Run は UI を固めないよう、Rust 側でバッチ実行（既定 1,000 命令）し、バッチ末尾、停止、ブレークポイント、エラー時に状態を通知する。ブレークポイントは命令添字に正規化し、停止前に現在 `pc` を照合する。状態画面は `pc`、`dp`、現在セル、ステップ数、テープのポインタ周辺（既定 ±16 セル）、出力、停止理由を表示する。

Phase 2 では可逆実行は必須でない。Step Back は状態履歴のメモリ量を明示した設計が決まるまで実装しない。

## 9. GUI 仕様（Phase 2）

メイン画面は、(a) CodeMirror 6 のソース編集領域、(b) Run/Pause/Step/Reset と入力設定、(c) テープビュー、(d) 出力ビュー、(e) 状態・エラー領域から成る。括弧対応強調と現在命令強調を実装する。デフォルトでは未保存の編集内容を実行でき、保存はユーザーが明示した時だけ行う。

Tauri コマンドは `load_source`、`start_session`、`run_batch`、`step`、`reset`、`get_snapshot` を最小セットとする。フロントエンドは Rust の内部型を直接複製せず、JSON シリアライズ可能な DTO を介す。巨大なテープ全体を高頻度に送らず、表示窓のみを返す API を追加する。

## 10. エラーと安全性

| 分類 | 条件 | 必須情報 |
|---|---|---|
| Parse | 未対応の括弧 | 種類、ソース位置、命令添字 |
| Runtime | ポインタ範囲外 | `pc`、`dp`、移動方向 |
| Runtime | 命令数超過 | 上限、`pc`、実行済み数 |
| I/O | 読み書き失敗 | 対象、OS エラーの文脈 |

任意のソースは信頼できない入力として扱う。命令数制限を既定で有効にし、GUI の実行はキャンセル可能にする。テープサイズは Phase 1 では固定し、外部入力から任意量を確保しない。エラーには入力バイト列や絶対パスなど不要な機微情報を含めない。Rust の `unsafe` は使用しない。

## 11. テスト方針

`brainfuck-core` は外部プロセスなしで単体テスト可能にする。

- パーサ: 有効文字だけの抽出、ネストした括弧、未対応の開閉括弧、位置情報。
- 命令: 8 命令それぞれ、`u8` の両方向ラップ、EOF が 0。
- ポインタ: 左右の境界でエラー、エラー時に状態不変。
- ループ: 0 のスキップ、反復、ネスト、ジャンプ先が `[`／`]` の次であること。
- 実行: `step()` の一命令性、停止後の冪等性、ステップ上限の境界値。
- 結合: Hello World、入力をエコーするプログラム、CLI の標準出力と終了コード。
- 回帰: 発見した不具合ごとに最小の `.bf` とテストを追加する。

Phase 3 以降は、素朴なインタプリタをオラクルとし、生成した小規模プログラムに対して最終状態・出力・エラー種別を最適化実装／コンパイラと比較する。ベンチマークは最適化の正しさの証明ではないため、正規テストと分離する。

## 12. 受け入れ基準

### Phase 1

1. `cargo test --workspace` が成功する。
2. `examples/hello.bf` が CLI で `Hello World!\n` を出力する。
3. 8 命令、30,000 セル、`u8` ラップ、範囲外ポインタエラー、バイト I/O、EOF=0 がテストで確認できる。
4. 括弧対応は解析時に一度だけ構築され、未対応の括弧は実行前に失敗する。
5. 上限到達時は追加命令を実行せず、位置を含む明確なエラーを返す。
6. `Machine::step()` が GUI なしで利用でき、スナップショットから `pc`、`dp`、セル値、実行数を観測できる。
7. CLI のプログラム出力と診断が別ストリームであり、終了コードが本仕様に一致する。

### Phase 2以降

1. GUI で編集、実行、停止、1ステップ、リセット、出力・テープ・位置表示ができる。
2. GUI の結果は同じ入力を与えた CLI と一致する。
3. 最適化／WASM／ネイティブ実行方式を追加した場合、比較画面に方式、実行時間、実行命令数（定義が異なる場合は明示）、終了状態を表示する。

## 13. 判断ログ

- セル幅とテープ長は、標準的かつ予測可能な教育用既定値として `u8` と 30,000 に固定する。
- 左右への無限拡張ではなく範囲外エラーを選び、実行状態を可視化しやすくする。
- EOF=0 は対話・バッチともに決定的な挙動を与える。
- コメント無視はソース上の可読性を許容する一方、デバッグ位置は「有効命令」を基準にすることで一貫性を保つ。

