# Requirements Document

## Project Description (Input)
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

## Constraints
Node 26(mise.toml)、実行はキャンセル可能、巨大テープ全体を高頻度転送しない、保存はユーザー明示時のみ。

## Introduction

`desktop-gui` は、学習者が Brainfuck のソース、現在命令の位置、テープ、出力を 1 つの画面で観察しながら、プログラムを編集・実行・一時停止・1 命令ずつ実行・リセットできるデスクトップアプリケーションを提供する。ソースの編集領域、操作と入力の設定、テープビュー、出力ビュー、状態・エラー領域から成るメイン画面を持ち、現在命令の強調、括弧対応の強調、ブレークポイントの表示を行う。

実行意味論とデバッグ動作(命令の実行、停止理由、ブレークポイント照合、バッチ実行、キャンセル、テープ表示窓、命令添字とソース位置の対応)は上流の `brainfuck-core` と `debugger-state` が定義した契約をそのまま利用し、本 feature はそれらを再実装しない。同一のソース・入力・命令数上限を与えたとき、GUI の結果(出力バイト列、終了時の `pc` / `dp` / 実行済み命令数、失敗の種類)は `brainfuck-cli` と一致しなければならない。

既定では未保存の編集内容をそのまま実行でき、ファイルへの保存はユーザーが明示的に操作したときに限り行う。実行はいつでも一時停止(キャンセル)でき、巨大なテープ全体を高頻度に転送することはせず、ポインタ周辺の表示窓のみを扱う。

利用者は、主に Brainfuck の学習者(ステップ実行で状態遷移を観察する)と、本アプリを土台に比較画面を追加する後続 feature(`compiler-comparison`)の開発者である。

## Boundary Context

- **In scope**:
  - メイン画面(ソース編集、操作と入力設定、テープビュー、出力ビュー、状態・エラー領域)とその操作性
  - ソースの編集、未保存内容の実行、ファイルを開く・明示的に保存する操作
  - 入力バイト列の設定(テキスト/16 進)、命令数上限の設定
  - Run / Pause / Step / Reset と、停止理由・エラーの表示
  - 現在命令・括弧対応・ブレークポイント・エラー位置のエディタ上の表示
  - フロントエンドとバックエンドの間の JSON ベースのデータ契約(最小 6 コマンドとテープ表示窓)
  - CLI との結果一致の自動検証、アプリ自体の自動テストと開発手順の文書
- **Out of scope**:
  - 実行意味論・デバッグロジック(命令実行、停止理由の生成、ブレークポイント照合、バッチ・キャンセルの規則)の定義(`brainfuck-core` / `debugger-state`)
  - 最適化インタプリタ、WASM/ネイティブ実行、比較画面(`optimized-interpreter` / `compiler-comparison`)
  - Step Back(可逆実行)、Step Over / Step Out、条件付きブレークポイント、ウォッチポイント、テープ内容の編集
  - クラウド機能、アカウント、ネットワーク通信、利用状況の収集
  - 複数ドキュメントの同時編集(タブ)、出力のファイル保存、設定の永続化、インストーラ署名・自動更新などの配布機能
- **Adjacent expectations**:
  - `debugger-state` は、停止理由(`Halted` / `StepLimitReached` / `Breakpoint` / `Cancelled`)、ブレークポイント、バッチ実行、キャンセル、Step Into、リセット、テープ表示窓、命令添字とソース範囲(エディタ向けの文字位置を含む)の変換、解析済みプログラムを持たないソース文字列からの位置変換、エディタの文字位置(UTF-16)からの命令添字の逆引きを提供し、本 feature は位置・逆引き・テープ窓の計算を再実装せず、それらを呼び出して表示・操作へ接続する。`StepLimitReached` は「バッチ命令数に到達した」ことを意味し、利用者向けの停止理由ではなく実行継続の合図として扱う
  - `brainfuck-core` の命令数上限超過は実行エラー(`StepLimitExceeded`)であり、バッチ到達とは区別して表示する
  - `brainfuck-cli` は結果一致の基準(オラクル)であり、例えば `examples/hello.bf` を `--dump-state` 付きで実行すると `Hello World!\n` と `state: pc=106 dp=6 steps=906` を得る。CLI の既定の命令数上限は 10,000,000 である
  - `compiler-comparison` は後続 feature として、本アプリのソースと入力の設定を共有しつつ比較画面を追加する。本 feature はその画面を提供しないが、画面を追加しても既存のデバッグ画面の挙動を変えずに済む構成を保つ

## Requirements

### Requirement 1: メイン画面の構成と起動時の状態
**Objective:** As a 学習者, I want 起動直後から編集・実行・観察に必要な領域が 1 つの画面に揃っていること, so that 迷わずプログラムを実行して状態を観察できる

#### Acceptance Criteria
1. When アプリケーションが起動する, the Desktop GUI shall ソース編集領域、操作(Run / Pause / Step / Reset)と入力設定の領域、テープビュー、出力ビュー、状態・エラー領域を同時に表示するメイン画面を表示する
2. When アプリケーションが起動する, the Desktop GUI shall `examples/hello.bf` と同一内容の Hello World プログラムを、ファイルに紐づかない新規文書としてソース編集領域に表示する
3. While 実行セッションが存在しない, the Desktop GUI shall テープビュー・出力ビュー・状態領域に、初期状態(全セル 0、`dp` 0、`pc` 0、実行済み命令数 0、出力なし)と「未開始」であることを表示する
4. The Desktop GUI shall 1 つのウィンドウで 1 つの文書と 1 つの実行セッションのみを同時に扱う
5. The Desktop GUI shall 画面上のラベル・メッセージ・状態名を日本語で表示する

### Requirement 2: ソースの編集・未保存内容の実行・明示保存
**Objective:** As a 学習者, I want 編集中の内容をファイルに保存しなくても実行でき、保存は自分が操作したときだけ行われること, so that 試行錯誤を気軽に行いつつ、ファイルを意図せず書き換えられない

#### Acceptance Criteria
1. The Desktop GUI shall ソース編集領域で、日本語などの非 ASCII 文字を含む任意のテキストを編集できる
2. While 編集領域の内容が最後に開いた/保存した内容と異なる, the Desktop GUI shall 文書に未保存の変更があることを画面上で示す
3. When ユーザーが Run または Step を要求する, the Desktop GUI shall ファイルに保存されていない現在の編集内容を実行対象とする
4. The Desktop GUI shall ユーザーが「保存」または「名前を付けて保存」を明示的に操作したときに限り、ファイルへ書き込む(自動保存・一時ファイルの作成・実行に伴う書き込みを行わない)
5. When ユーザーが「開く」を操作してファイルを選択する, the Desktop GUI shall そのファイルの内容を編集領域に読み込み、未保存の変更が無い状態にし、ファイル名(ディレクトリを含まない名前)を表示する
6. When ユーザーが「新規」を操作する, the Desktop GUI shall 空の新規文書を編集領域に表示する
7. If 未保存の変更がある状態で「開く」「新規」またはウィンドウを閉じる操作が行われる, then the Desktop GUI shall 変更を破棄してよいかをユーザーに確認し、ユーザーが取り消した場合は現在の文書と編集内容を変更しない
8. When ユーザーが保存を操作し、文書がファイルに紐づいている, the Desktop GUI shall そのファイルへ現在の内容を UTF-8 で書き込み、未保存の変更が無い状態にする
9. When ユーザーが保存を操作し、文書がファイルに紐づいていない, the Desktop GUI shall 保存先を選択させ、選択された場合のみ書き込んで、その文書をそのファイルに紐づける
10. If 保存先の選択が取り消される, then the Desktop GUI shall ファイルを書き込まず、編集内容と未保存の変更の表示を維持する
11. If ファイルの書き込みまたは読み込みに失敗する, then the Desktop GUI shall 失敗した操作の種類と OS が報告する原因をユーザーに示し、編集内容と未保存の変更の表示を維持する
12. If 開こうとしたファイルが UTF-8 として不正なバイト列を含む, then the Desktop GUI shall 不正な部分を置換文字に置き換えて読み込み、置き換えを行ったことをユーザーに通知する
13. If 開こうとしたファイルのサイズが 1 MiB(1,048,576 バイト)を超える, then the Desktop GUI shall 読み込まずに理由を通知し、現在の文書を変更しない
14. The Desktop GUI shall 編集領域の内容が 1 MiB(1,048,576 バイト)を超える場合、実行を開始せず理由を通知する

### Requirement 3: 入力の設定
**Objective:** As a 学習者, I want プログラムに与える入力バイト列をテキストまたは 16 進で指定できること, so that `,` を使うプログラムを任意のバイト列で試せる

#### Acceptance Criteria
1. The Desktop GUI shall 入力設定領域に、入力をテキストとして指定するモードと 16 進バイト列として指定するモードを提供する
2. When テキストモードで入力が指定される, the Desktop GUI shall 入力文字列を UTF-8 に符号化したバイト列を、改行の追加・変換や末尾の補完なしにそのまま入力バイト列とする
3. When 16 進モードで入力が指定される, the Desktop GUI shall 2 桁の 16 進数(大文字・小文字を区別しない)の並びを、空白(スペース・タブ・改行)を無視して、入力バイト列に変換する
4. If 16 進モードの入力に 16 進数字・空白以外の文字が含まれる、または 16 進数字の総数が奇数である, then the Desktop GUI shall 入力設定領域に問題の種類と位置を示すエラーを表示し、入力が訂正されるまで実行セッションの開始を行わない
5. If 入力バイト列が 1 MiB(1,048,576 バイト)を超える, then the Desktop GUI shall 入力を受け付けず、上限を超えたことを通知する
6. When テキストモードから 16 進モードへ切り替える, the Desktop GUI shall 現在の入力バイト列を同じバイト列を表す 16 進表記へ変換する
7. When 16 進モードからテキストモードへ切り替える, the Desktop GUI shall 入力バイト列が UTF-8 として正しい場合にのみ同じバイト列を表すテキストへ変換し、正しくない場合はモードを変更せず理由を通知する
8. The Desktop GUI shall 現在の入力バイト列の総バイト数を表示する
9. When 実行セッションが入力バイト列を読み取る, the Desktop GUI shall 状態領域に、読み取り済みの入力バイト数と入力の総バイト数を表示する
10. When 入力が尽きた状態で入力命令が実行される, the Desktop GUI shall `brainfuck-core` の意味論(現在セルに 0 を格納)をそのまま反映した結果を表示する

### Requirement 4: 実行制御(Run / Pause / Step / Reset)
**Objective:** As a 学習者, I want Run・Pause・Step・Reset でプログラムの実行を制御できること, so that 実行を最後まで走らせることも、1 命令ずつ観察することもできる

#### Acceptance Criteria
1. When 実行セッションが存在しない状態でユーザーが Run または Step を要求する, the Desktop GUI shall 現在の編集内容を解析し、現在の入力・命令数上限・ブレークポイントで新しい実行セッションを開始してから、要求された操作を実行する
2. If 実行セッションを開始するための解析が括弧の不整合で失敗する, then the Desktop GUI shall セッションを開始せず、状態・エラー領域にエラーを表示し、実行ビューを未開始の状態に保つ
3. When ユーザーが Run を要求する, the Desktop GUI shall 停止(正常終了)・ブレークポイント・実行エラー・ユーザーの Pause のいずれかが起きるまで、ユーザーの追加操作なしに実行を継続する
4. When ユーザーが Pause を要求する, the Desktop GUI shall 実行中のプログラムを命令の境界で停止し、その時点の状態(`pc`、`dp`、テープ、出力、実行済み命令数)を整合した形で表示する
5. When Pause で停止したセッションで再び Run を要求する, the Desktop GUI shall 停止した位置から実行を再開し、中断なしで実行した場合と同じ最終結果(出力・テープ・`pc`・`dp`・実行済み命令数・エラー)を得る
6. When ユーザーが Step を要求する, the Desktop GUI shall ブレークポイントとは無関係に、次に実行される命令をちょうど 1 個だけ実行し、実行済み命令数を 1 増加させ、各ビューを更新する
7. When Step の実行によってプログラムが最後の命令を終えて停止状態に達する, the Desktop GUI shall 正常終了を表示する
8. While セッションが停止状態(正常終了)または実行エラーで停止している, the Desktop GUI shall Run と Step を無効にし、Reset のみで実行を再開できる状態にする
9. When ユーザーが Reset を要求する, the Desktop GUI shall 同じソース・同じ入力・同じ命令数上限の初期状態へ実行を戻し、出力を空にし、各ビューと現在命令の強調を初期状態にし、設定済みのブレークポイントを保持する
10. While 実行セッションが存在しない, the Desktop GUI shall Pause と Reset を無効にし、Run と Step のみを有効にする
11. While プログラムが Run により実行中である, the Desktop GUI shall Run・Step・Reset を無効にし、Pause のみを有効にする
12. When ソースの編集、入力の変更、命令数上限の変更、別ファイルの読み込み、新規文書の作成のいずれかが実行セッションの存在中(実行中を除く)に行われる, the Desktop GUI shall 既存のセッションを破棄し、各ビューを未開始の状態へ戻す

### Requirement 5: 命令数上限と長時間実行時の応答性
**Objective:** As a 学習者, I want 無限ループなどの長時間実行でも画面が固まらず、いつでも止められること, so that 安全にプログラムを試せる

#### Acceptance Criteria
1. The Desktop GUI shall 命令数上限の設定を提供し、既定値を CLI の既定と同じ 10,000,000 とする
2. When ユーザーが命令数上限として 1 以上の整数を指定する, the Desktop GUI shall 次に開始する実行セッションからその上限を適用する
3. If 命令数上限として整数でない値、0 以下の値、または 9,007,199,254,740,991(2^53 − 1)を超える値が指定される, then the Desktop GUI shall 値を受け付けず、理由を入力欄の近くに表示し、直前の有効な上限を維持する
4. Where ユーザーが「上限なし」を明示的に選択する, the Desktop GUI shall 命令数上限を無効にして実行セッションを開始し、出力やメモリ使用量が増え続けうることを警告として表示する
5. If 実行が命令数上限に達した状態でさらに命令を実行しようとする, then the Desktop GUI shall 実行を停止し、上限値と停止位置(`pc`)を含む「命令数上限超過」のエラーを表示し、バッチ実行の命令数への到達とは区別して扱う
6. When 内部のバッチ実行がバッチ命令数に到達したがプログラムが停止していない, the Desktop GUI shall それを利用者向けの停止として表示せず、ユーザーの操作なしで実行を継続する
7. While Run による実行が続いている, the Desktop GUI shall ウィンドウの操作(Pause を含む)に応答し続ける
8. When 命令数上限が無効、または上限への到達に 1 秒以上かかる状態で、終了しないプログラムを Run している間にユーザーが Pause を要求する, the Desktop GUI shall 要求から 1 秒以内に実行を停止して、停止した状態を表示する
9. While Run による実行が続いている, the Desktop GUI shall テープ・出力・状態領域の表示を少なくとも 1 秒に 1 回は最新の状態に更新する
10. While Run による実行が続いている, the Desktop GUI shall ソース編集領域・入力設定・命令数上限・ブレークポイントの編集・ファイル操作を受け付けない

### Requirement 6: ブレークポイント
**Objective:** As a 学習者, I want 命令の位置にブレークポイントを設定して、その手前で実行を止められること, so that 注目する箇所の状態を観察できる

#### Acceptance Criteria
1. When ユーザーがエディタの行番号欄(ガター)で行を操作する, the Desktop GUI shall その行で最初に現れる命令にブレークポイントを切り替える(設定済みなら解除する)
2. When ユーザーがカーソル位置でブレークポイントの切り替えを操作する, the Desktop GUI shall カーソル位置以降で最初に現れる命令にブレークポイントを切り替える
3. If 指定した行またはカーソル位置以降に命令が存在しない, then the Desktop GUI shall ブレークポイントを設定せず、その理由をユーザーに通知する
4. The Desktop GUI shall ブレークポイントを、コメント文字ではなく実行される命令にのみ設定し、実際に停止する命令の位置にマーカーとして表示する
5. When Run 中に次に実行する命令がブレークポイントに設定されている, the Desktop GUI shall その命令を実行する前に停止し、停止理由として「ブレークポイント」を表示し、その命令を現在命令として強調する
6. When ブレークポイントで停止したセッションで再び Run を要求する, the Desktop GUI shall 同じ位置で再び停止し続けず、その命令を実行して先へ進む
7. While ブレークポイントがループ内の命令に設定されている, the Desktop GUI shall Run のたびに、そのループの反復ごとに同じ位置で停止する
8. When ソースが編集される, the Desktop GUI shall ブレークポイントを、対象の命令の移動に追従させ、対象の命令が削除された場合はそのブレークポイントを解除する
9. When ユーザーがすべてのブレークポイントの解除を操作する, the Desktop GUI shall 設定済みのブレークポイントをすべて解除する
10. When 別ファイルの読み込みまたは新規文書の作成が行われる, the Desktop GUI shall すべてのブレークポイントを解除する
11. While 実行セッションが存在し Run による実行中でない, the Desktop GUI shall ブレークポイントの切り替えと解除を受け付け、次の実行から有効にする
12. The Desktop GUI shall Step の実行時にはブレークポイントによる停止を行わない

### Requirement 7: エディタ上の現在命令・括弧対応・エラー位置の強調
**Objective:** As a 学習者, I want ソース上で現在の命令と括弧の対応が分かること, so that プログラムの制御の流れを目で追える

#### Acceptance Criteria
1. While 実行セッションが存在し、プログラムが停止していない, the Desktop GUI shall 次に実行される命令(現在命令)の文字をソース編集領域で強調する
2. When 実行が停止する(Pause・ブレークポイント・Step・エラー), the Desktop GUI shall 現在命令がソース編集領域の可視範囲に入るようにスクロールする
3. While プログラムが正常終了している, the Desktop GUI shall 現在命令の強調を表示しない
4. While 実行セッションが存在しない, the Desktop GUI shall 現在命令の強調を表示しない
5. While テキストカーソルが括弧命令(`[` または `]`)の直前または直後にある, the Desktop GUI shall 対応する括弧を強調する
6. If テキストカーソルに隣接する括弧に対応する括弧が存在しない, then the Desktop GUI shall 対応なしであることが区別できる形でその括弧を強調する
7. The Desktop GUI shall 現在命令・括弧対応・ブレークポイント・エラー位置の強調を、色の違いだけに頼らず、形状・下線・マーカーなどの色以外の手がかりでも区別できるようにする
8. The Desktop GUI shall 強調の位置を、マルチバイト文字(サロゲートペアを含む)や CRLF・CR・LF の改行を含むソースでも、実行される命令の文字位置と正確に一致させる
9. If 実行エラーが発生する, then the Desktop GUI shall エラー時点の現在命令(エラーを起こした命令)をエラー位置として強調する
10. If 解析エラー(括弧の不整合)が発生する, then the Desktop GUI shall 該当する括弧の位置をエラー位置として強調する
11. While 実行中(Run)である, the Desktop GUI shall 現在命令の強調を、表示の更新に合わせて最新の位置へ更新する

### Requirement 8: テープビュー
**Objective:** As a 学習者, I want データポインタの周辺のテープの内容が見えること, so that セルの値とポインタの移動を観察できる

#### Acceptance Criteria
1. The Desktop GUI shall テープビューに、データポインタを中心とする既定半径 16 セル(最大 33 セル)の窓を表示し、テープの端では範囲外のセルを表示せずに切り詰める
2. The Desktop GUI shall 窓の各セルについて、テープ上の添字と値を 10 進と 16 進で表示し、値が表示可能な ASCII 文字であればその文字も表示する
3. The Desktop GUI shall データポインタが指すセルを、色以外の手がかり(枠・矢印・ラベルなど)でも区別できる形で強調する
4. When 実行の状態が更新される, the Desktop GUI shall 窓を更新後のデータポインタを中心に再計算して表示する
5. Where ユーザーが表示半径を変更する, the Desktop GUI shall 0 以上 64 以下の半径での窓表示へ切り替え、以降の更新に適用する
6. The Desktop GUI shall 各更新でバックエンドから取得するテープのセル数を、表示半径に基づく窓の大きさ以下に限り、テープ全体(30,000 セル)を取得しない
7. While 実行セッションが存在しない, the Desktop GUI shall 全セル 0・ポインタ位置 0 を初期状態の窓として表示する
8. The Desktop GUI shall テープビューに表示する値を、`brainfuck-core` のテープの現在値と常に一致させる

### Requirement 9: 出力ビュー
**Objective:** As a 学習者, I want プログラムの出力をテキストまたは 16 進で見られること, so that 文字出力もバイナリ出力も確認できる

#### Acceptance Criteria
1. The Desktop GUI shall 出力ビューに、現在のセッションが開始(またはリセット)されてから出力されたすべてのバイトを、出力された順に表示する
2. The Desktop GUI shall 出力ビューにテキスト表示と 16 進表示を切り替える操作を提供し、切り替えが実行の状態に影響しないようにする
3. While テキスト表示が選択されている, the Desktop GUI shall 出力全体のバイト列を UTF-8 として解釈して表示し、UTF-8 として不正なバイト列は置換文字として表示する
4. When 1 つの UTF-8 文字のバイト列が複数回の表示更新にまたがって出力される, the Desktop GUI shall 出力全体を連結してから解釈し、文字を欠損・化けさせずに表示する
5. While テキスト表示が選択されている, the Desktop GUI shall 改行とタブ以外の制御文字を、視認できる記号で表示する
6. While 16 進表示が選択されている, the Desktop GUI shall 出力のバイトを 2 桁の 16 進数として、出力の先頭からのバイト位置が分かる形で表示する
7. The Desktop GUI shall 現在の出力の総バイト数を表示する
8. If 出力が 65,536 バイトを超える, then the Desktop GUI shall 表示を末尾の 65,536 バイトに限り、先頭が省略されていることと省略後も正確な総バイト数を表示する
9. The Desktop GUI shall 出力がまだ空の場合、空であることが分かる表示にする

### Requirement 10: 状態・エラー領域
**Objective:** As a 学習者, I want 実行の現在状態と停止理由・エラーの内容が分かること, so that プログラムがなぜ止まったかを理解できる

#### Acceptance Criteria
1. The Desktop GUI shall 状態領域に、`pc`(命令添字)、`dp`(データポインタ)、現在セルの値、実行済み命令数を 10 進で常に表示する
2. The Desktop GUI shall 実行の状態を「未開始」「実行中」「一時停止」「ブレークポイントで停止」「ステップ実行後」「正常終了」「実行エラー」のいずれかのラベルで状態領域に表示する
3. When プログラムが正常終了する, the Desktop GUI shall 「正常終了」と最終的な実行済み命令数を表示する
4. If 解析エラー(未対応の括弧)が発生する, then the Desktop GUI shall 状態・エラー領域に、エラーの種類(対応する開き括弧がない閉じ括弧/対応する閉じ括弧がない開き括弧)、命令添字、ソース上の行と列を表示する
5. If ポインタ範囲外の実行エラーが発生する, then the Desktop GUI shall 状態・エラー領域に、エラー時の `pc`・`dp`・移動方向を表示し、実行状態はエラー発生直前のまま保持する
6. If 命令数上限超過の実行エラーが発生する, then the Desktop GUI shall 状態・エラー領域に、上限値と `pc` を表示し、実行状態は上限に達した時点のまま保持する
7. If バックエンドとの通信の失敗や内部エラー、ファイル操作の失敗など、プログラムの実行結果ではないエラーが発生する, then the Desktop GUI shall それをプログラムの実行エラーとは区別できる形で通知し、アプリケーションの操作を継続できる状態に保つ
8. While エラーが表示されている, the Desktop GUI shall 次のユーザー操作(Reset・編集・Run・Step・別ファイルの読み込み)によって状態が変わるまで、そのエラー表示を維持する
9. The Desktop GUI shall エラー・診断の表示に、ソースの内容、入力バイト列、絶対パスを含めない
10. When 実行の状態ラベルが変化する, the Desktop GUI shall その変化を支援技術が読み上げ可能な形で状態領域に反映する

### Requirement 11: CLI との結果一致
**Objective:** As a 学習者および後続 feature の開発者, I want 同じソースと入力に対して GUI が CLI と同じ結果を返すこと, so that GUI での観察結果をそのまま信頼できる

#### Acceptance Criteria
1. When 同一のソース・入力バイト列・命令数上限で GUI と CLI がプログラムを実行する, the Desktop GUI shall CLI と同一の出力バイト列を得る
2. When 同一のソース・入力バイト列・命令数上限で GUI と CLI が正常終了まで実行する, the Desktop GUI shall CLI の状態表示(`pc` / `dp` / 実行済み命令数)と同一の終了時の `pc`・`dp`・実行済み命令数を得る
3. When `examples/hello.bf` を空の入力・既定の命令数上限で GUI が実行する, the Desktop GUI shall `Hello World!\n` を出力し、終了時に `pc` 106・`dp` 6・実行済み命令数 906 を表示する
4. If 同一のソース・入力・上限で CLI が解析エラー、ポインタ範囲外、命令数上限超過のいずれかで失敗する, then the Desktop GUI shall 同じ種類の失敗を、CLI と同じ命令添字・`pc`・`dp`・方向・上限値で報告する
5. The Desktop GUI shall 実行結果を、Run を 1 回で最後まで実行した場合、Pause と Run を繰り返した場合、ブレークポイントで停止と再開を繰り返した場合、Step を繰り返した場合、内部のバッチ命令数がどのような値の場合のいずれでも同一にする
6. The Desktop GUI shall 出力バイト列を、UTF-8 として正しくない場合も含め、バイト単位で変更せずに保持し、テキスト/16 進の切り替えは表示だけに作用させる
7. The Desktop GUI shall 非 ASCII のコメント文字や改行コードの種類(LF・CRLF・CR)が命令列と実行結果に影響しないようにする
8. The Desktop GUI shall 上記の一致を、少なくとも Hello World、入力をそのまま出力するプログラム(0x00 と 0xFF を含む入力)、ポインタ範囲外、命令数上限超過、括弧の不整合、入力が尽きた状態の入力命令を含むプログラムについて、自動テストで検証できるようにする。ただし上記の一致は GUI が受け付ける入力に対するものであり、次の 2 点は意図的な対象外として文書化する: (a) 命令数上限 0(要件 5.3 により GUI は受け付けないが CLI の `--max-steps 0` は受け付ける)、(b) 非 UTF-8 のファイル(要件 2.12 により GUI は置換文字へ置き換えて読み込むため、解析エラーのバイトオフセットは CLI が実ファイルのバイト列に対して報告する値と異なりうる)

### Requirement 12: フロントエンドとバックエンドのデータ契約
**Objective:** As a 後続 feature の開発者, I want 画面と実行エンジンの間が JSON で表現できる安定した契約で結ばれていること, so that 画面を追加しても実行エンジンの内部型や挙動に依存せずに済む

#### Acceptance Criteria
1. The Desktop GUI shall フロントエンドとバックエンドの間に、少なくとも `load_source`、`start_session`、`run_batch`、`step`、`reset`、`get_snapshot` の 6 つのコマンドと、テープの表示窓を取得する手段を提供する
2. When `load_source` が現在のソースを受け取る, the Desktop GUI shall ソースを解析し、成功した場合は命令数を、失敗した場合は種類・命令添字・ソース上の位置を含む解析エラーを返す
3. When `start_session` が入力バイト列・命令数上限・ブレークポイントを受け取る, the Desktop GUI shall 解析済みのソースから新しいセッションを開始し、初期状態を返す
4. When `run_batch` が呼び出される, the Desktop GUI shall 指定されたバッチ命令数(既定 1,000)を超えない範囲で実行し、停止理由またはエラー、実行した命令数、更新後の状態を返す
5. When `step`・`reset`・`get_snapshot` が呼び出される, the Desktop GUI shall それぞれ 1 命令の実行・初期状態への復帰・現在状態の取得を行い、更新後の状態を返す
6. The Desktop GUI shall 停止理由と実行エラーを、正常終了・ブレークポイント・キャンセル(Pause)・バッチ命令数への到達・ポインタ範囲外・命令数上限超過が互いに区別できる形で表現する
7. The Desktop GUI shall すべての要求と応答を JSON で表現できる型(DTO)に限り、フロントエンドがバックエンドの内部型(機械の内部状態、プログラム、セッション自体)を複製・保持することを要しない形にする
8. The Desktop GUI shall 状態の応答に、現在のスナップショット(`pc` / `dp` / 現在セル / 実行済み命令数 / 停止状態)、テープの表示窓、現在命令のソース上の範囲、入力の読み取り済みバイト数、直近の停止理由を含める
9. The Desktop GUI shall 出力を、フロントエンドが既に保持しているバイト位置以降の差分と総バイト数で返し、毎回すべての出力を再送しない
10. The Desktop GUI shall ソース上の位置を、エディタ上の文字位置にそのまま対応付けられる形(文字位置、行、列)で返す
11. If 要求の値が不正(バッチ命令数 0、範囲外のブレークポイントやテープ半径、存在しない/破棄済みのセッションへの操作、解析済みのソースがない状態での開始など)である, then the Desktop GUI shall 状態を変更せず、原因の種類を示す構造化されたエラーを返し、異常終了しない
12. While セッションが破棄された後に古い応答が到着する, the Desktop GUI shall その応答を画面に反映せず、現在のセッションの表示を保つ
13. The Desktop GUI shall フロントエンドが使うデータ型を、バックエンドの DTO 定義から機械的に得られる形にし、両者の乖離を自動検査で検出できるようにする
14. The Desktop GUI shall 後続の画面がソースと入力の設定を共有でき、独自のコマンドと DTO を既存のコマンドと衝突させずに追加できる構成にする

### Requirement 13: 安全性とセキュリティ
**Objective:** As a 利用者, I want 信頼できないソースを実行しても PC やファイルが安全であること, so that 他人から受け取ったプログラムも安心して試せる

#### Acceptance Criteria
1. The Desktop GUI shall 入力されたソースを信頼しない入力として扱い、命令数上限を既定で有効にした状態で実行を開始する
2. The Desktop GUI shall プログラムの実行を `brainfuck-core` の意味論の範囲に限り、ソースの内容によって外部のコマンド実行・ファイル操作・ネットワーク通信が行われないようにする
3. The Desktop GUI shall 画面(Web コンテンツ)に対し、アプリケーションが定義したコマンド以外の特権(任意のファイルアクセス、プロセス起動、ネットワークアクセス)を与えない
4. The Desktop GUI shall ファイルの読み書きを、ユーザーがファイル選択ダイアログで明示的に選んだファイルに限る
5. The Desktop GUI shall ネットワーク通信を行わず、利用状況やソース・入力・出力を外部へ送信しない
6. The Desktop GUI shall ソース・入力・バッチ命令数・テープ半径など外部から与えられる値に上限を設け、それらの値の大きさに応じて任意量のメモリを確保しない
7. The Desktop GUI shall 診断メッセージとログに、ソースの内容、入力バイト列、絶対パスを含めない
8. The Desktop GUI shall 新規に追加する Rust コードで `unsafe` を使用しない

### Requirement 14: 操作性とアクセシビリティ
**Objective:** As a 学習者, I want キーボードだけでも主要な操作ができ、状態が色以外でも分かること, so that さまざまな環境で学習に集中できる

#### Acceptance Criteria
1. The Desktop GUI shall Run・Pause・Step・Reset・ブレークポイントの切り替え・ファイル操作・表示モード(出力のテキスト/16 進、入力のテキスト/16 進)の切り替えを、キーボードだけで実行できるようにする
2. The Desktop GUI shall Run・Pause・Step・Reset のボタンに文字のラベルを付け、各操作に割り当てたキーボードショートカットを画面上で確認できるようにする
3. While 操作が現在の状態で実行できない, the Desktop GUI shall 該当する操作を無効な状態で表示する
4. The Desktop GUI shall 実行の状態・エラー・ブレークポイント・現在位置などの意味を、色の違いだけで伝えない
5. The Desktop GUI shall 幅 800 × 高さ 600 ピクセルまでのウィンドウサイズ縮小を許し、そのサイズでもすべての領域に到達でき、内容が欠落しない

### Requirement 15: 検証と開発手順
**Objective:** As a 開発者, I want GUI の振る舞いと CLI との一致を自動テストで確認でき、開発手順が文書化されていること, so that 変更による不具合を早期に検出して継続的に開発できる

#### Acceptance Criteria
1. The Desktop GUI shall アプリケーションのウィンドウを起動せずに実行できる自動テストで、データ契約(すべての停止理由・エラーの表現、位置の変換、テープ窓の端、出力差分、不正な要求の拒否)を検証する
2. The Desktop GUI shall 自動テストで、Run / Pause / Step / Reset / ブレークポイントの一連の操作が要件 4・5・6 に従うこと(ブレークポイント停止後の再開、Pause 後の再開、バッチ命令数や分割の仕方によらない同一結果)を検証する
3. The Desktop GUI shall 自動テストで、入力の変換(テキスト・16 進・モード切り替え・不正値)、出力の表示変換(UTF-8・16 進・不正バイト・文字のまたがり・省略)、操作の有効/無効の判定、エディタ上の強調位置(マルチバイト・改行コード)を検証する
4. The Desktop GUI shall 自動テストで、要件 11 の CLI との結果一致を検証する
5. The Desktop GUI shall 既存の `cargo test --workspace` を、フロントエンドのビルド成果物やウィンドウ表示の環境なしで成功し続けられるようにする
6. The Desktop GUI shall Node 26(`mise.toml`)で、依存の導入・開発起動・ビルド・テストの手順と、自動テストで検証できないウィンドウ表示の確認手順(編集、Hello World の実行と CLI との比較、無限ループの Pause、Step、Reset、ブレークポイント、保存)を、アプリケーションの README に文書化する
7. When 不具合が発見される, the Desktop GUI shall 再現する最小のソース/操作列と対応する回帰テストを追加する運用に従う
