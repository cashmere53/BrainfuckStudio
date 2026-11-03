# Requirements Document

## Project Description (Input)

### Problem
core を再現可能に実行する手段(ファイル入力、標準入出力、終了コード)が無く、Hello World などの検証や GUI/最適化の比較基準となる実行環境が得られない。

### Current State
greenfield。core 完成後に着手する。

### Desired Outcome
`brainfuck-cli [OPTIONS] <PROGRAM>` がソースを実行し、プログラム出力のみを stdout へ、診断/状態/使用方法を stderr へ出し、終了コード 0/2/3/4 を仕様どおり返す。`examples/hello.bf` が `Hello World!\n` を出力する。

### Approach
`crates/brainfuck-cli/src/main.rs` で引数解析(`--input`/`--max-steps`/`--no-step-limit`/`--dump-state`/`--help`)、`--input` 無しは stdin を EOF まで読む。core のエラーを終了コードへ対応付ける。

### Scope
- **In**: 引数処理、既定ステップ上限 10,000,000、入出力、`--dump-state`、終了コード、`examples/hello.bf`、README(検証コマンド記載)、CLI 結合テスト(stdout/stderr/終了コード)
- **Out**: 対話 UI、デバッガ機能、設定ファイル

### Constraints
出力はバイト列をそのまま書く(UTF-8 前提にしない)。エラー表示に絶対パス等の機微情報を含めない。

## Introduction

`brainfuck-cli` は、`brainfuck-core` の実行エンジンをプロセス境界(ファイル、標準入出力、終了コード)から再現可能に利用するためのコマンドライン実行環境である。`brainfuck-cli [OPTIONS] <PROGRAM>` は Brainfuck ソースファイルを実行し、プログラムの出力バイト列だけを標準出力へ、診断・状態表示・使用方法を標準エラーへ書き分け、結果の種別を終了コード(0 / 2 / 3 / 4)で返す。

利用者は、Brainfuck プログラムを手元で確かめたい学習者と、本 CLI の結果を「同一入力に対する正解」として GUI・最適化実装・コンパイラの出力と比較する後続 feature の開発者・テスト実装者である。

## Boundary Context

- **In scope**:
  - コマンドライン引数の受理と検証(プログラムパス、`--input`、`--max-steps`、`--no-step-limit`、`--dump-state`、`--help`)
  - ソースファイルと入力の取得(入力ファイルまたは標準入力)、出力バイト列の標準出力への書き出し
  - 既定の命令数上限(10,000,000)と、その変更・無効化
  - 診断・状態表示・使用方法の標準エラーへの出力、機微情報(パス、入力バイト列)の除外
  - 終了コード 0 / 2 / 3 / 4 の割り当てと、複数の失敗が起こり得る場合の決定規則
  - `examples/hello.bf`、README の検証手順、CLI 結合テスト
- **Out of scope**:
  - 対話 UI、デバッガ機能(ブレークポイント、ステップ実行、バッチ実行)、設定ファイル
  - Brainfuck の実行意味論そのもの(命令遷移、`u8` ラップ、テープ長、EOF、範囲外判定、実行数の数え方)
  - 括弧対応の解析規則とその診断の中身の決定
  - 実行方式の切り替え(最適化インタプリタ、コンパイラ)
- **Adjacent expectations**:
  - `brainfuck-core` が実行意味論・解析・命令数上限の強制・実行エラー種別を所有する。本 CLI は core が返した出力バイト列・エラー種別・実行数を、結果として加工せずプロセスの出力と終了コードへ対応付ける
  - `desktop-gui` は、同一プログラム・同一入力に対する本 CLI の標準出力と終了コードを、結果一致検証の基準として用いる
  - `debugger-state` が扱うブレークポイントやバッチ実行は本 feature の対象外である

## Requirements

### Requirement 1: コマンドライン引数の受理と検証
**Objective:** As a Brainfuck 学習者, I want 決まった形式のコマンドラインでプログラムと実行条件を指定できること, so that 同じコマンドで同じ結果を再現できる

#### Acceptance Criteria
1. When プログラムのパスを 1 個だけ指定して起動する, the Brainfuck CLI shall そのファイルのソースを実行する
2. The Brainfuck CLI shall `--input <PATH>`、`--max-steps <N>`、`--no-step-limit`、`--dump-state`、`--help` の 5 個のオプションを受け付ける
3. When 値を取るオプションを `--option value` または `--option=value` のいずれかの形式で指定する, the Brainfuck CLI shall 両形式を同じ意味として扱う
4. When オプションをプログラムのパスの前後どちらに指定する, the Brainfuck CLI shall 同じ意味として扱う
5. If プログラムのパスが指定されていない, then the Brainfuck CLI shall 標準出力へ何も書かず、原因と使用方法の案内を標準エラーへ書き、終了コード 2 で終了する
6. If プログラムのパスが 2 個以上指定されている, then the Brainfuck CLI shall 標準出力へ何も書かず、原因を標準エラーへ書き、終了コード 2 で終了する
7. If 未知のオプションが指定されている, then the Brainfuck CLI shall 標準出力へ何も書かず、原因を標準エラーへ書き、終了コード 2 で終了する
8. If 値を取るオプションの値が欠けている, then the Brainfuck CLI shall 標準出力へ何も書かず、原因を標準エラーへ書き、終了コード 2 で終了する
9. If 値を取るオプション(`--input`、`--max-steps`)が 2 回以上指定されている, then the Brainfuck CLI shall 標準出力へ何も書かず、原因を標準エラーへ書き、終了コード 2 で終了する
10. If 引数の検証に失敗する, then the Brainfuck CLI shall ファイルの読み込みとプログラムの実行を開始しない
11. When 同じ値なしオプション(`--no-step-limit`、`--dump-state`)を 2 回以上指定する, the Brainfuck CLI shall 1 回指定した場合と同じ意味として扱う

### Requirement 2: 使用方法の表示(`--help`)
**Objective:** As a 利用者, I want `--help` で使用方法を確認できること, so that オプションと終了コードの意味を調べられる

#### Acceptance Criteria
1. When `--help` が指定される, the Brainfuck CLI shall 使用方法を標準エラーへ書き、標準出力へ何も書かず、終了コード 0 で終了する
2. When `--help` が指定される, the Brainfuck CLI shall ファイルの読み込みと標準入力の読み取りとプログラムの実行を行わない
3. When `--help` が他の引数エラー(プログラムパスの欠落、未知のオプション、値の不正など)と同時に指定される, the Brainfuck CLI shall 引数エラーよりも `--help` を優先し、終了コード 0 で使用方法を表示する
4. The Brainfuck CLI shall 使用方法に、起動形式、全オプションの意味、`--max-steps` の既定値(10,000,000)、終了コードの意味(0 / 2 / 3 / 4)を含める

### Requirement 3: ソースの読み込みと解析
**Objective:** As a Brainfuck 学習者, I want 指定したファイルのソースが正しく解析され、不正な括弧が実行前に分かること, so that 誤ったプログラムを実行してしまわずに済む

#### Acceptance Criteria
1. When 読み込めるソースファイルが指定される, the Brainfuck CLI shall その内容を Brainfuck ソースとして解析して実行する
2. When ソースファイルが UTF-8 として不正なバイトを含む, the Brainfuck CLI shall 当該バイトをコメントとして扱い、読み込みと解析を失敗させない
3. When ソースファイルが UTF-8 として不正なバイトを含む, the Brainfuck CLI shall エラー診断が示すソースバイトオフセットを、ファイル先頭からの実際のバイト位置と一致させる
4. When ソースファイルが空、またはコメント文字のみである, the Brainfuck CLI shall 標準出力へ何も書かず、終了コード 0 で終了する
5. If ソースの括弧が対応していない, then the Brainfuck CLI shall 命令を 1 個も実行せず、標準出力へ何も書かず、括弧エラーの種類・命令添字・ソースバイトオフセットを含む診断を標準エラーへ書き、終了コード 2 で終了する
6. If ソースファイルを読み込めない(存在しない、読み取り権限がない、ファイルではない等), then the Brainfuck CLI shall 標準出力へ何も書かず、読み込みに失敗した対象とその原因を標準エラーへ書き、終了コード 4 で終了する
7. The Brainfuck CLI shall 処理を、引数の検証、ソースの読み込み、ソースの解析、入力の取得、プログラムの実行、出力の書き出しの順に進め、最初に失敗した段階の分類に対応する終了コードで終了する

### Requirement 4: 入力の供給
**Objective:** As a Brainfuck 学習者, I want プログラムの入力をファイルまたは標準入力から与えられること, so that `,` を使うプログラムを再現可能に実行できる

#### Acceptance Criteria
1. When `--input <PATH>` が指定される, the Brainfuck CLI shall 指定ファイルの全バイトを、変換や解釈をせずにプログラムの入力として供給する
2. When `--input` が指定されない, かつ プログラムが入力命令(`,`)を 1 個以上含む, the Brainfuck CLI shall 標準入力を EOF までバイト列として読み取り、変換や解釈をせずにプログラムの入力として供給する
3. When `--input` が指定される, the Brainfuck CLI shall 標準入力を読み取らない
4. When `--input` が指定されない, かつ プログラムが入力命令(`,`)を 1 個も含まない, the Brainfuck CLI shall 標準入力を読み取らず、標準入力が端末や未クローズのパイプであっても待機せずに実行する
5. When 入力のバイトを使い果たした後に入力命令が実行される, the Brainfuck CLI shall 現在セルに 0 が格納される結果(EOF=0)をそのまま出力に反映する
6. When 入力が空(0 バイト)である, the Brainfuck CLI shall 入力命令を EOF として実行する
7. If `--input` で指定したファイルを読み込めない, then the Brainfuck CLI shall プログラムが入力命令を含むか否かにかかわらず、標準出力へ何も書かず、対象が入力ファイルであることと原因を標準エラーへ書き、終了コード 4 で終了する
8. If 標準入力の読み取りに失敗する, then the Brainfuck CLI shall 標準出力へ何も書かず、対象が標準入力であることと原因を標準エラーへ書き、終了コード 4 で終了する

### Requirement 5: 出力と診断のストリーム分離
**Objective:** As a 後続 feature のテスト実装者, I want 標準出力にプログラムの出力バイト列だけが出ること, so that CLI の標準出力をそのまま正解として他実装と比較できる

#### Acceptance Criteria
1. The Brainfuck CLI shall 標準出力へ、プログラムが出力したバイト列だけを、出力された順序でそのまま書き込む
2. The Brainfuck CLI shall 出力バイトに対して、UTF-8 としての検証、文字コード変換、改行コードの変換、付加(末尾改行を含む)を一切行わない
3. When 出力バイト列が 0x00 や 0xFF など UTF-8 として不正なバイトを含む, the Brainfuck CLI shall それらを欠落や置換なしに標準出力へ書き込む
4. The Brainfuck CLI shall 診断、状態表示、使用方法を標準出力へ書かず、標準エラーへのみ書く
5. While `--help` も `--dump-state` も指定されておらず実行が正常に完了する, the Brainfuck CLI shall 標準エラーへ何も書かない
6. If 実行エラーで実行が中断される, then the Brainfuck CLI shall 中断までにプログラムが出力したバイト列を標準出力へ書いた上で、診断を標準エラーへ書く
7. When プログラムの出力が空である, the Brainfuck CLI shall 標準出力へ 1 バイトも書かない

### Requirement 6: 命令数の上限
**Objective:** As a 信頼できないソースを実行する利用者, I want 既定で命令数に上限があり、明示的に変更・無効化できること, so that 無限ループでプロセスが止まらなくなることを防げる

#### Acceptance Criteria
1. When `--max-steps` も `--no-step-limit` も指定されない, the Brainfuck CLI shall 命令数の上限を 10,000,000 として実行する
2. When `--max-steps <N>` が指定される, the Brainfuck CLI shall 命令数の上限を N として実行する
3. When `--max-steps 0` が指定され, かつ プログラムが 1 個以上の命令を持つ, the Brainfuck CLI shall 命令を 1 個も実行せず、命令数上限超過として終了コード 3 で終了する
4. When プログラムがちょうど N 個の命令を実行して停止し, かつ `--max-steps N` が指定される, the Brainfuck CLI shall 実行エラーとせず、終了コード 0 で終了する
5. When `--no-step-limit` が指定される, the Brainfuck CLI shall 命令数による中断を行わずに実行する
6. If 実行済み命令数が上限に達してもプログラムが停止しない, then the Brainfuck CLI shall 追加の命令を実行せず、上限値と命令位置を含む診断を標準エラーへ書き、終了コード 3 で終了する
7. If `--max-steps` と `--no-step-limit` が同時に指定される, then the Brainfuck CLI shall 標準出力へ何も書かず、矛盾する指定であることを標準エラーへ書き、終了コード 2 で終了する
8. If `--max-steps` の値が 0 以上の 10 進整数として解釈できない(空、符号付き、非数字を含む、表現できる最大値を超える), then the Brainfuck CLI shall 標準出力へ何も書かず、`--max-steps` の値が不正であることを標準エラーへ書き、終了コード 2 で終了する

### Requirement 7: 実行状態の表示(`--dump-state`)
**Objective:** As a Brainfuck 学習者, I want 実行終了時の主要な状態を確認できること, so that プログラムがどこまで進んだかを CLI だけで把握できる

#### Acceptance Criteria
1. When `--dump-state` が指定され, かつ プログラムが停止する, the Brainfuck CLI shall 命令位置(`pc`)、データポインタ(`dp`)、実行済み命令数(`steps`)の 3 値を、名前と値の対応が分かる形で標準エラーへ書く
2. When `--dump-state` が指定され, かつ 実行エラー(ポインタ範囲外または命令数上限超過)で実行が中断される, the Brainfuck CLI shall 中断時点の `pc`、`dp`、`steps` を標準エラーへ書く
3. If `--dump-state` が指定されていても引数エラー、ソースの読み込み失敗、解析エラー、入力の取得失敗のいずれかで実行に至らない, then the Brainfuck CLI shall 状態表示を行わない
4. While `--dump-state` が指定されていない, the Brainfuck CLI shall 状態表示を標準エラーへ書かない
5. The Brainfuck CLI shall 状態表示の `pc`・`dp`・`steps` を、実行エンジンが報告する値と一致させる
6. When 標準出力への書き込みに失敗する, かつ `--dump-state` が指定され, かつ 実行自体は停止または中断している, the Brainfuck CLI shall 状態表示を標準エラーへ書く
7. The Brainfuck CLI shall 状態表示の書式を固定し、同じプログラムと入力に対して常に同じ表示を行う

### Requirement 8: 終了コード
**Objective:** As a スクリプトから CLI を呼ぶ利用者, I want 結果の種別が終了コードで一意に分かること, so that 呼び出し側が成功と各種失敗を機械的に判別できる

#### Acceptance Criteria
1. When プログラムが停止し, かつ 出力をすべて書き込めた, the Brainfuck CLI shall 終了コード 0 で終了する
2. If 引数の検証に失敗する, または ソースの括弧が対応していない, then the Brainfuck CLI shall 終了コード 2 で終了する
3. If 実行エンジンがポインタ範囲外または命令数上限超過の実行エラーを返す, then the Brainfuck CLI shall 終了コード 3 で終了する
4. If ソースファイル・入力ファイル・標準入力の読み取り、または標準出力への書き込みに失敗する, then the Brainfuck CLI shall 終了コード 4 で終了する
5. If 実行エンジンが実行エラーを返し, かつ 続く標準出力への書き込みにも失敗する, then the Brainfuck CLI shall 両方の診断を標準エラーへ書き、終了コード 4 で終了する
6. If 実行エンジンが正常停止以外の予約済みの停止理由を返す, then the Brainfuck CLI shall 標準エラーへ未完了で停止した旨の診断を書き、終了コード 3 で終了する
7. The Brainfuck CLI shall 終了コードとして 0、2、3、4 のいずれか 1 つだけを返し、本要件に定めのない値を返さない
8. When 同一のプログラム・入力・オプションで複数回実行する, the Brainfuck CLI shall 毎回同じ標準出力・標準エラー・終了コードを返す

### Requirement 9: 診断の内容と機微情報の除外
**Objective:** As a 利用者, I want 失敗の原因と位置が分かる診断が出て、パスや入力内容が漏れないこと, so that 問題を直せる一方で、ログやスクリーンショットを共有しても安全である

#### Acceptance Criteria
1. When 実行エラーが発生する, the Brainfuck CLI shall 診断に、原因の種別と、ポインタ範囲外では `pc`・`dp`・移動方向、命令数上限超過では上限値と `pc` を含める
2. When 括弧エラーが発生する, the Brainfuck CLI shall 診断に、エラーの種類、命令添字、ソースバイトオフセットを含める
3. When 入出力エラーが発生する, the Brainfuck CLI shall 診断に、失敗した対象の種別(プログラムファイル、入力ファイル、標準入力、標準出力のいずれか)と OS が報告した原因の説明を含める
4. The Brainfuck CLI shall すべての診断から、コマンドラインで指定されたパス文字列、その絶対パス、親ディレクトリ名、およびカレントディレクトリのパスを除外する
5. The Brainfuck CLI shall すべての診断から、ソースの全文・抜粋、プログラムへの入力バイト列、プログラムの出力バイト列を除外する
6. If 引数が不正である, then the Brainfuck CLI shall 診断に、利用者が指定した引数値の文字列を含めず、どの種類の不正か(欠落、未知、重複、矛盾、値の形式)を示す
7. The Brainfuck CLI shall 診断の文面を固定し、同じ原因に対して同じ文面を返す(OS が報告する原因の説明を除く)

### Requirement 10: サンプルプログラムと README
**Objective:** As a 初めて使う利用者, I want 付属のサンプルと README の手順で動作を確認できること, so that 環境構築後すぐに CLI が正しく動くことを確かめられる

#### Acceptance Criteria
1. The Brainfuck CLI shall リポジトリの `examples/hello.bf` として、実行すると正確に `Hello World!` と改行 1 個(`Hello World!\n`)だけを出力するサンプルプログラムを提供する
2. When `examples/hello.bf` を既定のオプションで実行する, the Brainfuck CLI shall 標準出力へ `Hello World!\n` だけを書き、標準エラーへ何も書かず、終了コード 0 で終了する
3. When 標準入力が端末である状態で `examples/hello.bf` を実行する, the Brainfuck CLI shall 入力待ちをせずに終了する
4. The Brainfuck CLI shall README に、検証手順として `cargo test --workspace` と `cargo run -p brainfuck-cli -- examples/hello.bf` の両方を記載する
5. The Brainfuck CLI shall README に、使用方法、全オプション、命令数上限の既定値、終了コード 0 / 2 / 3 / 4 の意味、標準出力と標準エラーの役割分担を記載する
6. The Brainfuck CLI shall README に、プログラム出力を端末へ直接表示すると端末側の文字コード処理により UTF-8 として不正なバイトが表示できない場合があること、およびファイルやパイプへのリダイレクトでバイト列を正確に取得できることを記載する

### Requirement 11: 実行エンジンの結果との一致と依存の制約
**Objective:** As a 後続 feature の開発者, I want CLI が実行意味論を独自に持たず実行エンジンの結果をそのまま反映すること, so that CLI の結果を単一の正解として GUI や他の実行方式と比較できる

#### Acceptance Criteria
1. The Brainfuck CLI shall Brainfuck の実行意味論(命令遷移、セル演算、ポインタ範囲、EOF、実行数の数え方)を独自に実装せず、`brainfuck-core` の結果を用いる
2. When 同一のソース・入力・命令数上限で `brainfuck-core` を直接実行した結果と CLI の実行結果を比較する, the Brainfuck CLI shall 標準出力のバイト列、実行済み命令数、実行エラーの種別を一致させる
3. The Brainfuck CLI shall `unsafe` コードを一切含まない
4. The Brainfuck CLI shall ワークスペースのルートから `cargo test --workspace` で、`brainfuck-core` とともにビルドおよびテストできる

### Requirement 12: テスト
**Objective:** As a 保守者, I want CLI の観測可能な契約が自動テストで検証されること, so that 終了コードやストリーム分離が将来の変更で壊れても検知できる

#### Acceptance Criteria
1. The Brainfuck CLI shall 実際に CLI を起動するプロセス単位の結合テストで、標準出力・標準エラー・終了コードを検証する
2. The Brainfuck CLI shall 結合テストで、`examples/hello.bf` の標準出力が `Hello World!\n` であり終了コードが 0 であることを検証する
3. The Brainfuck CLI shall 結合テストで、0x00 と 0xFF を含む任意のバイト列を標準入力および `--input` ファイルから与えるエコープログラムが、同一のバイト列を標準出力へ書くことを検証する
4. The Brainfuck CLI shall 結合テストで、終了コード 0、2、3、4 のそれぞれに到達する代表ケースを検証する
5. The Brainfuck CLI shall 結合テストで、プログラム出力が標準出力のみに、診断・状態表示・使用方法が標準エラーのみに出ることを、成功ケースと失敗ケースの両方で検証する
6. The Brainfuck CLI shall 結合テストで、標準入力を読まない条件(入力命令を含まないプログラム、`--input` 指定)で、標準入力が閉じられていなくても CLI が終了することを検証する
7. The Brainfuck CLI shall 結合テストで、診断にコマンドラインで指定したパス文字列が含まれないことを検証する
8. If 結合テスト中に CLI が終了しない, then the Brainfuck CLI shall テストを有限時間内に失敗として終了させる
9. When `cargo test --workspace` を実行する, the Brainfuck CLI shall 外部ツール、ネットワーク、既存の環境変数設定に依存せず、すべてのテストを完走させる
10. When 不具合が発見される, the Brainfuck CLI shall 最小の `.bf` と対応する回帰テストを追加する運用に従う。初期ケースとして、空のソースファイルが終了コード 0・標準出力空で終了する回帰テストを 1 件含む
