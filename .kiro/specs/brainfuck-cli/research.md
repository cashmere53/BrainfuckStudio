# Research & Design Decisions

## Summary
- **Feature**: `brainfuck-cli`
- **Discovery Scope**: Extension(upstream `brainfuck-core` の公開 API を利用する軽量発見。greenfield の bin crate だが外部技術の導入はなし)
- **Key Findings**:
  - core の `run()` はステップ上限を `Err(RuntimeError::StepLimitExceeded)` で返し、`StopReason::{StepLimitReached, Breakpoint, Cancelled}` は返さない。CLI は `#[non_exhaustive]` のためワイルドカードで防御し、`Halted` 以外は終了コード 3 とする
  - 仕様書の `examples/hello.bf` 検証コマンド(`cargo run -p brainfuck-cli -- examples/hello.bf`)は、`--input` 無しで常に標準入力を EOF まで読む実装だと端末で入力待ちになる。入力命令を含むプログラムのみ標準入力を読む方式で観測上の差なく解決できる
  - Brainfuck 命令は ASCII のみのため、0x80 以上のバイトを ASCII 空白へ置換すれば、命令列を変えず、かつ core が報告するソースバイトオフセットをファイル上の実バイト位置と一致させたまま、非 UTF-8 ソース(例: Shift-JIS コメント)を扱える

## Research Log

### core 公開 API の利用範囲
- **Context**: CLI がどこまで core に依存し、何を自前で持つか
- **Sources Consulted**: `.kiro/specs/brainfuck-core/design.md`、`requirements.md`
- **Findings**:
  - 使用する API は `Program::parse(&str)` / `Program::instructions()` / `Instruction` / `Machine::new(program, input, config)` / `Machine::run()` / `Machine::{output, snapshot}` / `MachineConfig::{unlimited, with_max_steps}`。いずれも core design で公開契約として定義済み
  - `MachineSnapshot` は `#[non_exhaustive]` だが公開フィールド(`pc`, `data_pointer`, `steps_executed`)は外部から読める
  - `Machine` はエラー時も状態を保持するため、実行エラー後に `output()` と `snapshot()` を取得できる(部分出力と `--dump-state` に必要)
  - `ParseError` / `RuntimeError` の `Display` は数値と列挙値のみで機微情報を含まない(core 13.7)
- **Implications**: CLI は意味論を再実装せず、core の値・文面を加工しない。core の Revalidation Triggers(`Instruction` 名、`run()` 規約、`Display` 文面)を CLI の Revalidation Triggers に引き継ぐ

### 引数解析ライブラリ(clap)の採否
- **Context**: オプションは 5 個と少ない。`--help`・終了コード 2・エラー整形が要件
- **Findings**:
  - clap は既定で `--help` を標準出力へ書き、エラー文に利用者が与えた値を含める。要件(使用方法は標準エラー、診断に指定値を含めない、`--help` を他の引数エラーより優先、argv[0] を表示しない)に合わせるには上書き設定が多く、挙動も依存バージョンに左右される
  - 手書き解析なら全挙動(重複・矛盾・`=` 形式・非 UTF-8 パス・値非エコー)を 1 ファイルで固定でき、外部依存ゼロ(core と同方針)を保てる
- **Implications**: 手書き解析を採用(`args.rs`、約 100 行想定)。短縮オプション(`-h`)と `--` 区切りは仕様書にないため非対応

### 標準入力を読む条件
- **Context**: 仕様書 §7「`--input` がない場合は標準入力を EOF までバイトとして読む」と、README の検証コマンド `cargo run -p brainfuck-cli -- examples/hello.bf` の両立
- **Findings**: `hello.bf` は `,` を含まない。端末から実行すると、無条件に標準入力を読む実装は Ctrl-D まで待機する。プログラムが入力命令を含まないなら標準入力を読まなくても出力・実行数・エラーは同一(読まれなかった入力は観測されない)
- **Implications**: 「`--input` なし かつ 入力命令あり」のときのみ標準入力を読む(要件 4.2〜4.4)。`--input` 指定は入力命令の有無によらず常にファイルを読み、不在は 4(利用者の明示指定を黙殺しない)。標準入力の読み取り失敗が報告されるのも入力命令ありの場合に限られる(許容)

### 非 UTF-8 ソースの扱い
- **Context**: `Program::parse` は `&str` を受け取る。ソースファイルは任意バイト列であり得る(日本語の旧エンコーディングのコメント等)
- **Alternatives**:
  1. 非 UTF-8 を I/O エラー(4)で拒否 — コメントだけの理由で実行できず不親切
  2. `String::from_utf8_lossy` — U+FFFD が 3 バイトになりオフセットがずれる(診断が実バイト位置と一致しない)
  3. 0x80 以上のバイトを ASCII 空白 1 個に置換 — 長さ・位置が保存される
- **Selected**: 3。命令は ASCII のみで 0x80 以上は必ずコメントなので意味は不変
- **Follow-up**: `desktop-gui` はソースを文字列(UTF-8)で扱うが、有効 UTF-8 では core が直接 `&str` を解析した結果と CLI の結果が同一になる(マルチバイト文字は元々コメント)

### 診断への機微情報の非混入
- **Context**: 制約「エラー表示に絶対パス等の機微情報を含めない」
- **Findings**: 標準ライブラリの `io::Error`(`File::open`/`read` 由来)の `Display` は OS メッセージのみでパスを含まない。ただし CLI が自前でパスを埋め込む、`argv[0]`(実行ファイルパスを含み得る)を usage に使う、未知オプション/不正値をエコーする、といった経路は設計で除外する必要がある
- **Implications**: 対象は役割名(program file / input file / standard input / standard output)で示し、usage のプログラム名は固定文字列 `brainfuck-cli`、引数エラーは種類のみを示す。結合テストで「指定した絶対パス・親ディレクトリ名・不正値が標準エラーに現れない」ことを検証する

### Windows コンソールでのバイト出力
- **Context**: 開発環境は Windows。要件はバイト列をそのまま書くこと
- **Findings**: Rust 標準ライブラリは標準出力がコンソールのとき UTF-16 変換のため UTF-8 として不正なバイトの書き込みをエラーにし得る。パイプ・ファイルへのリダイレクトでは無加工で書かれる
- **Implications**: CLI 自身は変換を一切しない(要件 5.2)。端末表示の制約は OS 側の事情として README に注記(要件 10.6)。コンソール書き込みエラーは I/O エラー(4)として診断される。結合テストはパイプで検証する

### 結合テストの実装手段(外部 crate なし)
- **Findings**:
  - `env!("CARGO_BIN_EXE_brainfuck-cli")` で実バイナリを起動できる。一時ファイルは `env!("CARGO_TARGET_TMPDIR")`(結合テスト専用の作業ディレクトリ)に一意名で作成でき `tempfile` 不要
  - `Command::output()` は標準入力を継承しない(null)。`StdinMode::OpenUnused` は `Stdio::piped()` を保持して書かず閉じない状態で「端末相当」を再現し、不要な読み取りによるハングを検出する
  - ハング検出のため `try_wait` ポーリング + 上限時間で `kill`
  - 書き込み失敗(終了コード 4)の再現: 子の標準出力パイプの読み取り側を親が即座に閉じる。論理の細部(部分失敗・実行エラー併記)は `Streams` 注入による単体テストで担保し、結合テストの再現はタイミング依存のためベストエフォートとする場合は単体テストで必須判定を満たす
- **Implications**: `tests/common/mod.rs` に共通ヘルパを集約

### `hello.bf` の検証
- **Findings**: `++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.` を参照実装(8 命令・30,000 セル・u8 ラップ)で実行し、出力 `Hello World!\n`、プログラム長 106、実行済み命令数 906、停止時 `pc=106 dp=6`、範囲外なし、`,` なしを確認
- **Implications**: `--dump-state` の期待表示 `state: pc=106 dp=6 steps=906` を結合テストの回帰オラクルにできる

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 単一 `main.rs` | すべてを 1 ファイルに実装 | 最小 | 引数/変換/入出力が混在し単体テストしづらい | brief の記述に最も近いが、責務分離のため不採用 |
| bin のみ + 小モジュール(採用) | `error`/`args`/`source`/`input`/`app` に分割し、ストリーム注入で `app` をテスト | 外部依存なし、境界が明確、プロセス起動なしで失敗系を検証可能 | ファイル数が増える | steering の `src/main.rs` を起点に同 crate 内で分割。構造は steering に適合 |
| lib + bin 分離 | `lib.rs` を設け bin は薄く | 外部からテスト可能 | 公開 API が増え「CLI 内部」を契約化してしまう | 不採用(結合テストは実バイナリで足りる) |

## Design Decisions

### Decision: 引数解析は手書き
- **Context**: 要件が出力先・終了コード・非エコーを細かく規定
- **Alternatives Considered**: clap(derive) / lexopt 等の軽量 crate / 手書き
- **Selected Approach**: `std::env::args_os` を手書きで解析
- **Rationale**: 5 オプションのみ。挙動を完全に固定でき、依存ゼロ
- **Trade-offs**: 解析コードを自前で保守。引き換えに挙動が依存バージョンに左右されない
- **Follow-up**: 表駆動の単体テストで網羅

### Decision: `--help` は標準エラーへ、終了コード 0
- **Context**: 仕様書「使用方法は標準エラーへ書く」「終了コード 0=正常終了」
- **Selected Approach**: 明示的な `--help` は標準出力を汚さず stderr に使用方法を出し 0 で終了。引数エラー時は短い案内行のみ(2)
- **Rationale**: 標準出力はプログラム出力のみという契約を `--help` でも崩さない

### Decision: 終了コードの複数失敗時の決定規則
- **Selected Approach**: 段の順序で最初の失敗が決める。唯一の例外として、実行エラー(3)の後に標準出力の書き込みが失敗した場合は 4(両診断を出力)
- **Rationale**: 出力を届けられなかった事実は呼び出し側にとって実行エラーより下位の情報ではなく、パイプラインの破損を示すため。仕様書 §7 の割り当てに矛盾しない
- **Follow-up**: 単体テストで固定

### Decision: 出力は実行完了後に一括書き込み
- **Context**: core は出力をバッファへ蓄積し、`run()` は完走またはエラーまで戻らない
- **Selected Approach**: 完了後に一括書き込み。実行エラー時も部分出力を先に書く
- **Rationale**: core に変更を加えず、決定的。対話 UI は Out of scope
- **Trade-offs**: 無制限実行 + 無限出力でメモリが増える(README で注意喚起)

### Decision: 値付きオプションの重複と排他
- **Selected Approach**: `--input` / `--max-steps` の重複と、`--max-steps` + `--no-step-limit` の併用は終了 2。値なしフラグの重複は冪等
- **Rationale**: 「最後が勝つ」はスクリプトの誤りを黙殺する。フラグの重複は無害

### Decision: ビルド/テスト基盤は core の workspace を前提とする
- **Selected Approach**: ルート `Cargo.toml`(`members = ["crates/*"]`)は core spec が作成する。CLI は変更しない
- **Follow-up**: core 未実装の状態では CLI の実装タスクは着手不可(依存関係として tasks に記載)

## Risks & Mitigations
- 標準入力を読む条件が仕様書の文言(常に読む)と異なる — 観測上の差がないこと、端末での検証コマンドが成立することを要件 4.4 / 10.3 とテストで明示
- Windows コンソールでの非 UTF-8 出力失敗 — README 注記 + リダイレクト推奨。CLI の契約はパイプ/ファイルで検証
- 書き込み失敗の結合テストはタイミング依存 — 単体テストで必須の論理を担保し、結合テストは子の起動直後に読み取り側を閉じて再現
- 10,000,000 命令の既定上限テストが遅い — 1 件のみ、デバッグビルドでも数秒を想定
- core の `Display` 文面や API 変更 — Revalidation Triggers に記載し、診断の部分文字列検証(原因語 + 数値)で脆さを抑える

## References
- `brainfuck-studio-development-specification.md` §7(CLI 仕様)、§10(エラーと安全性)、§12(受け入れ基準)
- `.kiro/specs/brainfuck-core/design.md`(公開 API、エラー型、`run()` の規約)
- Cargo book: `CARGO_BIN_EXE_<name>` / `CARGO_TARGET_TMPDIR`(結合テスト向け環境変数)
