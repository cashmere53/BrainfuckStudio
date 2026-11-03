# Requirements Document

## Introduction

`brainfuck-core` は、Brainfuck Studio のすべての機能(CLI、デバッグ GUI、最適化インタプリタ、コンパイラ比較)が依存する、正確でテスト可能な Brainfuck 実行エンジンである。本 spec は、8 命令・30,000 セル・`u8` ラップ・範囲外ポインタエラー・バイト I/O・EOF=0 という Phase 1 の意味論を、GUI・標準入出力・ファイルシステムなしで利用できる形で提供する。ここで確定する意味論(出力・最終テープ・エラー)は、後続のすべての実行方式にとっての正解オラクルとなる。

利用者は、本 crate を組み込む後続コンポーネントの開発者(CLI、デバッガ状態、最適化インタプリタ)と、Phase 1 の受け入れ基準を検証するテスト実装者である。

## Boundary Context

- **In scope**:
  - Cargo workspace の雛形と `brainfuck-core` crate の提供
  - ソースから命令列への解析(ソースバイトオフセットの保持、括弧対応表の解析時構築、括弧エラー)
  - 実行状態の保持と 1 命令単位の実行(`step`)、完走までの実行(`run`)
  - 実行命令数の上限(ステップ制限)と、そのエラー
  - 実行状態の観測(`pc`・`dp`・現在セル・実行数・停止状態、テープ全体、出力バイト列)
  - Parse / Runtime エラーの型による区別と、機微情報を含まない説明
  - 仕様書 §11 の単体・回帰テスト
- **Out of scope**:
  - CLI(ファイル・標準入出力の処理、終了コード、`--max-steps` 等の引数)
  - ブレークポイント実行、キャンセル、バッチ実行、テープの表示窓、行・列変換(Phase 1.5 のデバッガ状態)
  - GUI、最適化インタプリタ、コンパイラ
  - Brainfuck 方言の拡張、テープの動的拡張、可逆実行(Step Back)
- **Adjacent expectations**:
  - `brainfuck-cli` は、本 feature が返す出力バイト列・エラー種別・実行数を、そのままプロセス出力と終了コードへ対応付ける。I/O エラーの扱いは CLI が所有する
  - `debugger-state` は、本 feature の停止理由・スナップショットを後方互換のまま拡張する。`Breakpoint` と `Cancelled` の実際の挙動は本 feature が所有しない(種別の予約のみ)
  - `optimized-interpreter` および `compiler-comparison` は、本 feature の実行結果を正解オラクルとして比較する

## Requirements

### Requirement 1: ワークスペース基盤と依存・安全性の制約
**Objective:** As a 後続 feature の開発者, I want 外部環境に依存しない実行エンジンの土台が用意されること, so that CLI・GUI・最適化実装が同一の意味論を安全に再利用できる

#### Acceptance Criteria
1. The Brainfuck Core shall ワークスペースのルートから `brainfuck-core` を含む全メンバーを対象にビルドおよびテスト実行できる構成で提供される
2. The Brainfuck Core shall デスクトップ UI フレームワーク、標準入力、ファイルシステムに依存せずに利用できる
3. The Brainfuck Core shall 入力を呼び出し元が供給するバイト列として受け取り、出力を呼び出し元が取得するバイト列として返す
4. The Brainfuck Core shall `unsafe` コードを一切含まない
5. When 外部プロセスを起動せずにテストを実行する, the Brainfuck Core shall すべての単体・回帰テストを完走できる
6. The Brainfuck Core shall 公開する契約(プログラム、機械、スナップショット、停止理由、エラー)を、後続 feature が後方互換のまま拡張できる形で提供する

### Requirement 2: ソースの解析と命令列の生成
**Objective:** As a 後続 feature の開発者, I want ソース文字列から有効命令だけを順序どおり取り出せること, so that 実行・デバッグ・最適化が同じ命令列を基準にできる

#### Acceptance Criteria
1. When ソース文字列を解析する, the Brainfuck Core shall `>` `<` `+` `-` `.` `,` `[` `]` の 8 文字を、それぞれ対応する 1 個の命令としてソース上の出現順に命令列へ含める
2. When ソースに 8 文字以外の文字(改行・空白・日本語等のマルチバイト文字を含む)が含まれる, the Brainfuck Core shall それらをコメントとして無視し、命令列へ含めない
3. When 命令列を生成する, the Brainfuck Core shall 各命令に、元ソース上でその命令が現れたバイトオフセットを対応付けて参照可能にする
4. When ソースがマルチバイト文字を含む, the Brainfuck Core shall バイトオフセットを文字数ではなくソースのバイト位置として報告する
5. When ソースが空、またはコメント文字のみである, the Brainfuck Core shall 命令を 0 個とする正常な解析結果を返す
6. When 命令添字を与えてソース上のバイトオフセットを問い合わせる, the Brainfuck Core shall その命令に対応するバイトオフセットを返す
7. If 範囲外の命令添字でバイトオフセットを問い合わせる, then the Brainfuck Core shall 未定義動作や異常終了を起こさず、該当なしであることを呼び出し元が判別できる結果を返す

### Requirement 3: 括弧対応の解析時構築と括弧エラー
**Objective:** As a 後続 feature の開発者, I want 括弧の対応が解析時に一度だけ確定し不正な括弧が実行前に検出されること, so that 実行時の性能と安全性が保たれ、エラー位置を利用者に示せる

#### Acceptance Criteria
1. When 括弧が正しく対応しているソースを解析する, the Brainfuck Core shall 各 `[` について対応する `]` の命令添字を、各 `]` について対応する `[` の命令添字を参照できるようにする
2. When 括弧がネストしている, the Brainfuck Core shall 最も内側から順に正しく対応付ける
3. The Brainfuck Core shall 括弧の対応を解析時に一度だけ確定し、実行中に括弧を探索しない
4. If 対応する `[` を持たない `]` が現れる, then the Brainfuck Core shall 解析を失敗させ、最初に見つかった当該 `]` の位置を示す閉じ括弧不一致エラー(種類、ソースバイトオフセット、命令添字を含む)を返す
5. If 解析終了時に対応する `]` を持たない `[` が残る, then the Brainfuck Core shall 解析を失敗させ、未解決の `[` のうち最初に現れたものの位置を示す開き括弧不一致エラー(種類、ソースバイトオフセット、命令添字を含む)を返す
6. If 括弧が不一致である, then the Brainfuck Core shall 実行可能な状態のプログラムを一切返さず、実行が開始される前に失敗が判明する
7. If 括弧の不一致が複数ある, then the Brainfuck Core shall 一貫した規則(閉じ括弧不一致は走査中の最初の位置、開き括弧不一致は解析終了時点で残る最初の `[`)に従って 1 つのエラーを決定的に返す

### Requirement 4: 初期状態とテープ
**Objective:** As a テスト実装者, I want 機械の初期状態が一意に定まっていること, so that 同じプログラムと入力から常に同じ実行結果が得られる

#### Acceptance Criteria
1. When プログラム・入力バイト列・実行設定から機械を生成する, the Brainfuck Core shall テープの 30,000 個のセルをすべて 0 に初期化する
2. When 機械を生成する, the Brainfuck Core shall 命令ポインタ(`pc`)、データポインタ(`dp`)、実行済み命令数、入力読取位置をすべて 0 とし、出力を空にする
3. The Brainfuck Core shall テープ長を 30,000 セル、セル幅を 8 ビット符号なし整数に固定し、外部入力によって変更させない
4. The Brainfuck Core shall 有効なデータポインタの範囲を 0 以上 29,999 以下とする
5. When 同一のプログラム・入力・設定から複数の機械を生成する, the Brainfuck Core shall 互いに独立した状態を持たせ、一方の実行が他方に影響しない

### Requirement 5: ポインタ移動命令と範囲外エラー
**Objective:** As a Brainfuck 学習者, I want ポインタ移動が決まった範囲内でのみ成功し範囲外では明確なエラーになること, so that 状態遷移を予測でき誤りを発見しやすい

#### Acceptance Criteria
1. When `>` を実行し `dp` が 29,999 未満である, the Brainfuck Core shall `dp` を 1 増やし `pc` を 1 進める
2. When `<` を実行し `dp` が 0 より大きい, the Brainfuck Core shall `dp` を 1 減らし `pc` を 1 進める
3. If `dp` が 29,999 のときに `>` を実行する, then the Brainfuck Core shall ポインタ範囲外エラー(`pc`、`dp`、移動方向を含む)を返す
4. If `dp` が 0 のときに `<` を実行する, then the Brainfuck Core shall ポインタ範囲外エラー(`pc`、`dp`、移動方向を含む)を返す
5. If ポインタ範囲外エラーが発生する, then the Brainfuck Core shall `pc`、`dp`、テープ、出力、入力読取位置、実行済み命令数のいずれも変更しない
6. When `dp` が 0 から 29,999 まで `>` で移動し、続いて `<` で 0 まで戻る, the Brainfuck Core shall すべての移動を成功させる

### Requirement 6: セル演算命令とラップ
**Objective:** As a Brainfuck 学習者, I want セルの加減算が 8 ビットで循環すること, so that 標準的な Brainfuck プログラムが期待どおりに動作する

#### Acceptance Criteria
1. When `+` を実行する, the Brainfuck Core shall 現在セルを 1 加算し `pc` を 1 進める
2. When `-` を実行する, the Brainfuck Core shall 現在セルを 1 減算し `pc` を 1 進める
3. When 現在セルが 255 のときに `+` を実行する, the Brainfuck Core shall 現在セルを 0 にし、エラーを発生させない
4. When 現在セルが 0 のときに `-` を実行する, the Brainfuck Core shall 現在セルを 255 にし、エラーを発生させない
5. When セル演算を実行する, the Brainfuck Core shall `dp` 以外のセルを変更しない

### Requirement 7: 入出力命令(バイト列・EOF)
**Objective:** As a 後続 feature の開発者, I want 入出力が UTF-8 を前提としないバイト列で扱われ EOF の挙動が決定的であること, so that CLI と GUI で同一入力から同一出力が得られる

#### Acceptance Criteria
1. When `.` を実行する, the Brainfuck Core shall 現在セルの値を 1 バイトとして出力バイト列の末尾へ追加し、`pc` を 1 進める
2. When `,` を実行し未読の入力バイトが残っている, the Brainfuck Core shall 次の 1 バイトを現在セルへ格納し、入力読取位置を 1 進め、`pc` を 1 進める
3. If `,` を実行したとき入力バイトを使い果たしている, then the Brainfuck Core shall 現在セルに 0 を格納し、エラーを発生させず、`pc` を 1 進める
4. The Brainfuck Core shall 入力および出力の各バイトを 0 から 255 の任意の値として扱い、UTF-8 として有効であることを要求しない
5. When 出力バイト列が UTF-8 として不正なバイトを含む, the Brainfuck Core shall それらのバイトを変換や置換をせずにそのまま保持する
6. The Brainfuck Core shall 出力バイト列に出力命令が実行された順序でバイトを保持する

### Requirement 8: ループ命令とジャンプ
**Objective:** As a Brainfuck 学習者, I want ループの開始・終了が現在セルの値に従って正しく分岐すること, so that 反復・スキップ・ネストを含む任意の正しいプログラムが動作する

#### Acceptance Criteria
1. When `[` を実行し現在セルが 0 である, the Brainfuck Core shall `pc` を対応する `]` の次の命令へ進める
2. When `[` を実行し現在セルが 0 でない, the Brainfuck Core shall `pc` を次の命令へ進める
3. When `]` を実行し現在セルが 0 でない, the Brainfuck Core shall `pc` を対応する `[` の次の命令へ戻す
4. When `]` を実行し現在セルが 0 である, the Brainfuck Core shall `pc` を次の命令へ進める
5. When ループ命令を実行する, the Brainfuck Core shall セル・`dp`・入力読取位置・出力を変更しない
6. When ネストしたループを実行する, the Brainfuck Core shall 内側と外側のループをそれぞれ独立に正しく反復・スキップする
7. When `[` が現在セル 0 でスキップされる, the Brainfuck Core shall スキップ対象の範囲内の命令を一つも実行せず、実行済み命令数にも加えない
8. When ジャンプ先が命令列の末尾を超える位置(対応する `]` が最後の命令)である, the Brainfuck Core shall `pc` を命令列の長さと等しくしてプログラムを終了状態にする

### Requirement 9: 1 命令単位の実行と停止状態
**Objective:** As a デバッガ実装者, I want 1 回の操作でちょうど 1 命令だけ実行され停止後は状態が変わらないこと, so that ステップ実行・一時停止・再開の契約を GUI なしで構築できる

#### Acceptance Criteria
1. When 停止していない機械で 1 ステップ実行する, the Brainfuck Core shall ちょうど 1 個の命令を実行し、実行済み命令数を 1 増やし、命令を実行したことを示す結果を返す
2. The Brainfuck Core shall `pc` が命令列の長さと等しい状態を、プログラムの終了(停止)状態とする
3. When 停止状態の機械で 1 ステップ実行する, the Brainfuck Core shall 停止したことを示す結果を返し、`pc`、`dp`、テープ、出力、入力読取位置、実行済み命令数のいずれも変更しない
4. When 停止状態の機械で 1 ステップ実行を繰り返す, the Brainfuck Core shall 毎回同じ結果を返し、状態を変更しない
5. When 命令が 0 個のプログラムから機械を生成する, the Brainfuck Core shall 初期状態のまま停止状態であり、最初のステップ実行で停止したことを示す結果を返す
6. When 最後の命令を実行して `pc` が命令列の長さに達する, the Brainfuck Core shall その直後の状態を停止状態として扱う
7. If 実行エラーが発生する, then the Brainfuck Core shall 当該ステップで実行済み命令数を増やさない
8. The Brainfuck Core shall 実行済み命令数を、実際に実行した命令 1 個につき 1 ずつ加算し、コメント文字やスキップされた命令を数えない

### Requirement 10: 実行命令数の上限(ステップ制限)
**Objective:** As a 信頼できないソースを実行する利用者, I want 実行命令数に上限を設定でき上限到達時に追加の命令が実行されないこと, so that 無限ループによる資源の占有を防げる

#### Acceptance Criteria
1. Where 実行設定が命令数の上限を指定している, the Brainfuck Core shall 実行済み命令数が上限に達した機械が、次の命令を実行する前にステップ制限超過エラーを返す
2. Where 実行設定が命令数の上限を持たない(制限なし)を指定している, the Brainfuck Core shall 命令数による停止を行わない
3. If 実行済み命令数が上限に達しており、機械が停止状態でない, then the Brainfuck Core shall 追加の命令を一切実行せず、上限値と現在の `pc` を含むステップ制限超過エラーを返す
4. When ステップ制限超過エラーが発生する, the Brainfuck Core shall 機械の状態(`pc`、`dp`、テープ、出力、入力読取位置、実行済み命令数)を変更しない
5. When ちょうど上限と同数の命令でプログラムが終了する, the Brainfuck Core shall エラーを返さずに正常終了(停止)として扱う
6. When 上限より 1 命令多く必要なプログラムを実行する, the Brainfuck Core shall 上限と同数の命令を実行した時点でステップ制限超過エラーを返し、上限を超えて命令を実行しない
7. When 上限が 0 に設定されており、機械が停止状態でない, the Brainfuck Core shall 最初のステップ実行でステップ制限超過エラーを返し、命令を実行しない
8. When 上限が 0 に設定されており、命令が 0 個のプログラムである, the Brainfuck Core shall エラーを返さずに停止状態として扱う
9. While 機械がステップ制限超過で停止している, the Brainfuck Core shall 再度ステップ実行を要求されても同じエラーを返し続ける

### Requirement 11: 完走までの実行(`run`)と停止理由
**Objective:** As a 後続 feature の開発者, I want プログラムを終了またはエラーまで一括で実行し結果を判別できること, so that CLI や最適化実装の比較テストが簡潔に書ける

#### Acceptance Criteria
1. When プログラムを完走まで実行し正常に終了する, the Brainfuck Core shall 正常終了を示す停止理由を返す
2. While 機械が停止状態でない, the Brainfuck Core shall 完走までの実行中にステップ実行と同一の意味論で命令を繰り返し実行する
3. If 実行中に実行エラー(ポインタ範囲外またはステップ制限超過)が発生する, then the Brainfuck Core shall その時点で実行を中断し、当該エラーを呼び出し元へ返す
4. When 実行エラーによって中断した, the Brainfuck Core shall エラー発生時点までの実行状態(出力、テープ、`pc`、`dp`、実行済み命令数)を呼び出し元が観測できる状態で保持する
5. When すでに停止状態の機械で完走までの実行を要求する, the Brainfuck Core shall 状態を変更せず正常終了を示す停止理由を返す
6. The Brainfuck Core shall 停止理由の種別として、正常終了、ステップ制限到達、ブレークポイント、キャンセルを区別できる型を提供する
7. The Brainfuck Core shall ブレークポイントとキャンセルの停止理由を将来の拡張用に予約するのみとし、これらを返す実行挙動を本 feature では提供しない
8. When 同一のプログラム・入力・設定で完走までの実行と 1 ステップずつの繰り返し実行を行う, the Brainfuck Core shall 最終的な出力・テープ・`pc`・`dp`・実行済み命令数・エラー種別を完全に一致させる

### Requirement 12: 実行状態の観測(スナップショット・テープ・出力)
**Objective:** As a デバッガ・CLI の実装者, I want 実行中・実行後の状態を変更せずに読み取れること, so that 状態表示や結果検証を GUI なしで行える

#### Acceptance Criteria
1. When スナップショットを取得する, the Brainfuck Core shall `pc`、`dp`、現在セルの値、実行済み命令数、停止しているかどうかを返す
2. When スナップショットを取得する, the Brainfuck Core shall 機械の状態を変更しない
3. When テープを参照する, the Brainfuck Core shall 30,000 セルすべての現在値を読み取り専用で返す
4. When 出力を参照する, the Brainfuck Core shall これまでに出力されたバイト列全体を読み取り専用で返す
5. While 機械が停止状態である, the Brainfuck Core shall スナップショットの停止フラグを真とする
6. While 機械が停止状態でない, the Brainfuck Core shall スナップショットの停止フラグを偽とする
7. When ステップ実行の前後でスナップショットを取得する, the Brainfuck Core shall 実行済み命令数・`pc`・`dp`・現在セルの変化から実行された命令の効果を観測できるようにする

### Requirement 13: エラー型と診断情報
**Objective:** As a 利用者および後続 feature の開発者, I want エラーが種類ごとに型で区別され原因と位置が分かること, so that エラーを分類して表示・終了コードへ対応付けでき、原因を特定できる

#### Acceptance Criteria
1. The Brainfuck Core shall 解析エラーと実行エラーを互いに異なる型として区別して提供する
2. The Brainfuck Core shall 解析エラーとして、閉じ括弧不一致と開き括弧不一致を互いに区別できる形で提供する
3. The Brainfuck Core shall 実行エラーとして、ポインタ範囲外とステップ制限超過を互いに区別できる形で提供する
4. When ポインタ範囲外エラーを表示する, the Brainfuck Core shall 原因、`pc`、`dp`、移動方向を含む説明を生成する
5. When ステップ制限超過エラーを表示する, the Brainfuck Core shall 原因、上限値、`pc` を含む説明を生成する
6. When 括弧不一致エラーを表示する, the Brainfuck Core shall 原因、括弧の種類、命令添字、ソースバイトオフセットを含む説明を生成する
7. The Brainfuck Core shall エラーの内容(フィールドおよび表示文字列)に、ソース全文、入力バイト列、絶対パスなどの機微情報を含めない
8. The Brainfuck Core shall エラー型を標準的なエラー表現(表示可能かつ原因を辿れる形)として利用できる形で提供する

### Requirement 14: 仕様書に基づく検証可能性(テスト)
**Objective:** As a プロジェクト管理者, I want Phase 1 の受け入れ基準が自動テストで検証されること, so that 後続の最適化・コンパイラ実装が信頼できるオラクルを得られる

#### Acceptance Criteria
1. The Brainfuck Core shall パーサについて、有効文字だけの抽出、ネストした括弧、未対応の開き・閉じ括弧、エラー位置情報を検証する自動テストを含む
2. The Brainfuck Core shall 8 命令のそれぞれ、`u8` の加算・減算両方向のラップ、EOF 時の 0 格納を検証する自動テストを含む
3. The Brainfuck Core shall 左右の境界でのポインタ範囲外エラーと、エラー時の状態不変を検証する自動テストを含む
4. The Brainfuck Core shall ループの 0 スキップ、反復、ネスト、ジャンプ先が対応括弧の次であることを検証する自動テストを含む
5. The Brainfuck Core shall 1 ステップの 1 命令性、停止後の冪等性、ステップ上限の境界値(上限未満・上限ちょうど・上限超過)を検証する自動テストを含む
6. The Brainfuck Core shall 結合テストとして、`Hello World!\n` を出力するプログラム、入力をエコーするプログラム(EOF での終了を含む)を完走させ出力を検証する自動テストを含む
7. When 不具合を発見し修正する, the Brainfuck Core shall 最小の再現プログラムと対応する回帰テストを追加できる構成でテストを管理する
8. When ワークスペース全体のテストを実行する, the Brainfuck Core shall すべての自動テストが成功する
