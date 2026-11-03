# Requirements Document

## Introduction

`optimized-interpreter` は、Brainfuck Studio の Phase 3 として、`brainfuck-core` が提供する素朴インタプリタと**観測的に完全に一致**しながら高速に動作する、最適化インタプリタを提供する。ソースの解析結果(`brainfuck-core` のプログラム)を中間表現(IR)へ変換し、連続命令の統合と、代表的なループ(クリアループ、移動/加算/乗算ループ、ポインタ走査ループ)の最適化を行ってから実行する。

利用者は、実行方式による性能差を学習・比較したい学習者と、最適化の結果を比較対象として使う後続 feature(`compiler-comparison`)の開発者である。素朴インタプリタ(`brainfuck-core`)は正解オラクルであり、本 feature はその意味論・公開契約を一切変更しない。最適化の正しさはベンチマークではなく、素朴実装との差分テストで担保する。

## Boundary Context

- **In scope**:
  - `brainfuck-core` の解析済みプログラムから IR への変換(連続命令統合、代表的ループの最適化)
  - IR の最適化実行(完走までの実行、実行設定としての命令数上限の適用)
  - 素朴実装との観測的一致の定義と保証(出力、最終テープ、停止/エラー種別、エラー位置、実行数)
  - 実行命令数の定義(素朴実装換算の実行数と、最適化方式固有の実行単位数の併記)
  - エラー位置・ステップ制限の意味保存方針
  - 素朴インタプリタをオラクルとした差分テスト(正規テスト)と、これと分離したベンチマーク
  - 素朴意味論の再生(縮退経路)を公開モジュール(`replay`)として提供すること、および操作の素朴換算コストを求める純粋な公開ヘルパ(Requirement 14)。下流の実装が素朴意味論の再実装を持たずに済むようにする
- **Out of scope**:
  - JIT / AOT / WASM などのコンパイル方式(`compiler-comparison`)
  - GUI への統合、CLI への統合(実行方式の切り替え UI や引数)
  - `brainfuck-core` の意味論・公開契約の変更(基準意味論は `brainfuck-core` が所有する)
  - 1 命令ずつのステップ実行、ブレークポイント、キャンセル、バッチ実行(これらは素朴インタプリタ側の `debugger-state` が扱う)
  - テープの動的拡張、Brainfuck 方言の拡張、可逆実行
  - ソース解析(括弧エラーを含む)。解析エラーは `brainfuck-core` の解析が返すものがそのまま唯一の経路となる
- **Adjacent expectations**:
  - `brainfuck-core` は、解析済みプログラム、実行設定(ステップ上限)、エラー型を本 feature が再利用できる形で提供し、素朴実行の結果を差分テストのオラクルとして供給する
  - `compiler-comparison` は、本 feature が報告する実行数が「素朴実装換算の実行数」と「方式固有の実行単位数」の 2 種類であることを前提に、比較画面でどちらの定義かを明示する
  - `compiler-comparison` は、離脱後の素朴再生を自前で実装せず、本 feature が公開する再生(`replay`)とコスト式ヘルパを再利用できる。これらの署名・意味論を変える場合は `compiler-comparison` の再確認が必要になる
  - 本 feature は GUI・CLI の表示や配線を所有しない

## Requirements

### Requirement 1: 基盤・依存・安全性の制約
**Objective:** As a 後続 feature の開発者, I want 最適化インタプリタが素朴インタプリタを変更せず安全に追加されること, so that オラクルの信頼性を保ったまま実行方式を増やせる

#### Acceptance Criteria
1. The Optimized Interpreter shall `brainfuck-core` の意味論、公開型、公開関数を一切変更せず、`brainfuck-core` の解析結果を入力として利用する
2. The Optimized Interpreter shall デスクトップ UI フレームワーク、標準入力、ファイルシステムに依存せずに利用できる
3. The Optimized Interpreter shall 入力を呼び出し元が供給するバイト列として受け取り、出力を呼び出し元が取得するバイト列として返す
4. The Optimized Interpreter shall `unsafe` コードを一切含まない
5. When 外部プロセスを起動せずにテストを実行する, the Optimized Interpreter shall すべての正規テスト(単体・差分・回帰)を完走できる
6. The Optimized Interpreter shall テープ長(30,000 セル)、セル幅(8 ビット符号なし)、EOF 時の入力値(0)、セル演算のラップ動作を素朴実装と同一に保つ
7. The Optimized Interpreter shall 外部入力(ソース長、入力バイト列)の大きさに比例しない量のメモリをテープ用に確保しない

### Requirement 2: 中間表現(IR)への変換
**Objective:** As a 最適化の実装者およびベンチマーク利用者, I want 解析済みプログラムが検査可能な IR へ決定的に変換されること, so that 最適化の適用結果を確認でき、素朴実装との対応を追跡できる

#### Acceptance Criteria
1. When 解析済みプログラムを IR へ変換する, the Optimized Interpreter shall 失敗せずに IR を返す
2. When 同一のプログラムを複数回変換する, the Optimized Interpreter shall 毎回同一の IR を返す
3. The Optimized Interpreter shall IR の各操作について、それが対応する素朴実装の命令添字の範囲を参照できるようにする
4. The Optimized Interpreter shall IR の操作列を、実行を伴わずに読み取り専用で検査できるようにする
5. When 命令が 0 個のプログラムを変換する, the Optimized Interpreter shall 操作が 0 個の IR を返し、その実行は初期状態のまま停止状態となる
6. The Optimized Interpreter shall 括弧の対応先を IR の変換時に確定し、実行中に括弧を探索しない
7. The Optimized Interpreter shall IR の操作数を、素朴実装の命令数を超えないものとする

### Requirement 3: 連続命令の統合
**Objective:** As a Brainfuck 学習者, I want `+++` や `>>>>` のような連続命令が 1 つの操作にまとめられること, so that 長い連続命令を含むプログラムが高速に実行される

#### Acceptance Criteria
1. When `+` および `-` のみから成る連続した命令列がある, the Optimized Interpreter shall それらを、正味の加減算量(8 ビットの循環で正規化)を持つ 1 つの操作へ統合する
2. When 同じ向きのポインタ移動(`>` のみ、または `<` のみ)が連続している, the Optimized Interpreter shall それらを、移動量を持つ 1 つの操作へ統合する
3. If 統合によって、素朴実装が報告するポインタ範囲外エラーの位置(`pc`・`dp`・移動方向)または実行数が変わりうる, then the Optimized Interpreter shall その統合を行わない
4. The Optimized Interpreter shall 入出力命令(`.` `,`)を統合によって削除、並べ替え、または省略しない
5. When 統合された操作の正味効果が 0 である(例: `+-`), the Optimized Interpreter shall 実行数の換算(Requirement 6)において、元の命令数どおりの実行数を数える

### Requirement 4: 代表的ループの最適化
**Objective:** As a Brainfuck 学習者, I want 頻出するループ(クリア、移動/加算/乗算、ポインタ走査)が反復なしで実行されること, so that 素朴実装との性能差を学習・比較できる

#### Acceptance Criteria
1. When 本体が現在セルを 1 減らす(または 1 増やす)だけのループ(`[-]` `[+]`)がある, the Optimized Interpreter shall それをクリアループとして認識し、ループを反復せずに現在セルを 0 にする
2. When 本体がポインタ移動と加減算のみから成り、ポインタの正味移動量が 0 で、開始セルの正味変化が -1 であるループ(例: `[->+<]` `[->>+++<<]` `[->+>+<<]`)がある, the Optimized Interpreter shall それを移動/加算/乗算ループとして認識し、開始セルの値に応じた加算を他のセルへ一括して適用したうえで開始セルを 0 にする
3. When 本体が同じ向きのポインタ移動のみから成るループ(例: `[>]` `[<]` `[>>]`)がある, the Optimized Interpreter shall それを走査ループとして認識し、現在セルが 0 になるまでの移動を反復なしで求める
4. If ループの本体が入出力命令、入れ子のループ、または上記以外の効果を含む, then the Optimized Interpreter shall そのループを最適化せず、素朴実装と同じ分岐意味論の一般ループとして実行する
5. If 最適化ループの実行時の条件(ポインタ範囲、実行数の残り)が一括適用を意味保存的に許さない, then the Optimized Interpreter shall 素朴実装と同一の状態遷移を再現できる方法で当該ループを実行する
6. When 現在セルが 0 のときに最適化ループへ到達する, the Optimized Interpreter shall 本体を実行せず、素朴実装と同じく `[` の 1 命令分のみを実行数として数える
7. The Optimized Interpreter shall 最適化の適用有無によらず、同一のプログラム・入力・設定から同一の観測結果(Requirement 5)を返す

### Requirement 5: 素朴実装との観測的一致
**Objective:** As a 後続 feature の開発者, I want 最適化インタプリタの結果が素朴インタプリタの結果と完全に一致すること, so that 実行方式を切り替えても結果が変わらず、素朴実装を唯一のオラクルとして使える

#### Acceptance Criteria
1. When 同一のプログラム・入力・設定で最適化インタプリタと素朴インタプリタを完走まで実行する, the Optimized Interpreter shall 出力バイト列、最終テープ(30,000 セル全体)、終了種別(正常停止、ポインタ範囲外、ステップ制限超過)を素朴実装と完全に一致させる
2. When 実行がエラーで中断した, the Optimized Interpreter shall エラー発生時点の出力バイト列、テープ全体、データポインタ、入力読取位置を、素朴実装がそのエラーで中断した時点の状態と完全に一致させる
3. When 実行が正常に停止した, the Optimized Interpreter shall 最終的なデータポインタ、入力読取位置、実行数(Requirement 6 の素朴実装換算の実行数)を素朴実装と完全に一致させる
4. The Optimized Interpreter shall エラーの種別だけでなく、エラーが保持する位置・数値情報(ポインタ範囲外の `pc`・`dp`・移動方向、ステップ制限超過の上限値・`pc`)も素朴実装と完全に一致させる。ここで `pc` は素朴実装の命令添字で表す
5. The Optimized Interpreter shall 実行結果の等価性を、素朴実装の実行結果との直接比較(同値性)で検証できる形で観測結果を提供する
6. The Optimized Interpreter shall 実行中のエラー以外の失敗(異常終了、未定義動作)を一切起こさない

### Requirement 6: 実行命令数の定義
**Objective:** As a 実行方式の比較を行う利用者, I want 実行方式ごとの「実行数」の意味が明確に区別されていること, so that 方式間の数値を誤解なく比較できる

#### Acceptance Criteria
1. The Optimized Interpreter shall 「素朴実装換算の実行数」(素朴実装が同じ実行で数える実行数と同じ値)を報告する
2. When 最適化された操作(統合操作、最適化ループ)を実行する, the Optimized Interpreter shall その操作が表す素朴実装の命令の実行回数(スキップされた命令やコメントを含まない)を、素朴実装換算の実行数へ加算する
3. The Optimized Interpreter shall 「方式固有の実行単位数」(IR の操作を実行した回数)を、素朴実装換算の実行数とは別の値として報告する
4. The Optimized Interpreter shall 方式固有の実行単位数が、素朴実装換算の実行数を超えないことを保証する
5. When 素朴実装換算の実行数と方式固有の実行単位数を報告する, the Optimized Interpreter shall 両者が異なる定義であることを、名称または文書によって呼び出し元が判別できるようにする
6. The Optimized Interpreter shall 素朴実装換算の実行数を、実行がエラーで中断した場合を含め、素朴実装が同一時点で数える値と一致させる
7. When 連続命令や最適化ループを含むプログラムを実行する, the Optimized Interpreter shall 方式固有の実行単位数が素朴実装換算の実行数より小さくなる場合があること(最適化の効果)を、決定的に観測できる

### Requirement 7: 命令数上限(ステップ制限)の意味保存
**Objective:** As a 信頼できないソースを実行する利用者, I want 最適化後も命令数上限が素朴実装と同じ境界で働くこと, so that 最適化によって上限を回避したり、早く打ち切られたりしない

#### Acceptance Criteria
1. Where 実行設定が命令数の上限を指定している, the Optimized Interpreter shall 上限を素朴実装換算の実行数に対して適用する
2. When 上限がちょうど素朴実装が必要とする実行数と等しい, the Optimized Interpreter shall エラーを返さずに正常停止として扱う
3. When 上限が素朴実装が必要とする実行数より 1 小さい, the Optimized Interpreter shall ステップ制限超過エラーを返し、上限と同数の命令に相当する実行を行った時点の状態で中断する
4. When 上限が、統合された操作または最適化ループの実行の途中に当たる, the Optimized Interpreter shall 素朴実装が同じ上限で中断した時点と同一の状態(出力、テープ、データポインタ、入力読取位置、実行数、エラーの `pc`)で中断する
5. When 上限が 0 に設定されており、命令が 1 個以上ある, the Optimized Interpreter shall 命令を一つも実行せずにステップ制限超過エラーを返す
6. When 上限が 0 に設定されており、命令が 0 個のプログラムである, the Optimized Interpreter shall エラーを返さずに停止状態として扱う
7. Where 実行設定が命令数の上限を持たない(制限なし)を指定している, the Optimized Interpreter shall 命令数による停止を行わない
8. While 機械がステップ制限超過で中断している, the Optimized Interpreter shall 再度の実行要求に対して同じエラーを返し続け、状態を変更しない

### Requirement 8: ポインタ範囲外エラーの意味保存
**Objective:** As a Brainfuck 学習者, I want 最適化後もポインタ範囲外が素朴実装と同じ位置で報告されること, so that エラーの原因をソースの命令位置で正しく特定できる

#### Acceptance Criteria
1. If 実行中にデータポインタが有効範囲(0 以上 29,999 以下)の外へ移動しようとする, then the Optimized Interpreter shall ポインタ範囲外エラーを返し、その `pc`・`dp`・移動方向は、素朴実装が同じ実行で報告するものと一致する
2. When ポインタ範囲外エラーが、統合された移動操作の途中で発生する, the Optimized Interpreter shall エラーを返す時点のデータポインタを範囲外へ到達する直前の位置とし、`pc` を範囲外へ移動しようとした元の命令の添字とする
3. When ポインタ範囲外エラーが、最適化ループの実行中に発生する, the Optimized Interpreter shall 素朴実装がそのループを実行して範囲外に至った時点までのテープ・出力・データポインタ・実行数の状態を再現する
4. If ポインタ範囲外エラーが発生する, then the Optimized Interpreter shall エラーの発生ステップで実行数を増やさず、エラー後の状態をそれ以上変更しない
5. When 範囲外に達しない境界上の移動(0 から 29,999 への往復を含む)を実行する, the Optimized Interpreter shall エラーを発生させずに成功させる
6. While 現在セルが 0 のため最適化ループが本体を実行しない, the Optimized Interpreter shall ループ本体が範囲外に触れうる場合でもエラーを発生させない

### Requirement 9: 入出力の意味保存
**Objective:** As a 後続 feature の開発者, I want 最適化後も入出力がバイト列として素朴実装と同一に扱われること, so that CLI・GUI・比較画面で同一入力から同一出力が得られる

#### Acceptance Criteria
1. When `.` を実行する, the Optimized Interpreter shall 現在セルの値を 1 バイトとして出力バイト列の末尾へ、実行された順序で追加する
2. When `,` を実行し未読の入力バイトが残っている, the Optimized Interpreter shall 次の 1 バイトを現在セルへ格納し、入力読取位置を 1 進める
3. If `,` を実行したとき入力バイトを使い果たしている, then the Optimized Interpreter shall 現在セルに 0 を格納し、エラーを発生させない
4. The Optimized Interpreter shall 入力および出力の各バイトを 0 から 255 の任意の値として扱い、UTF-8 として有効であることを要求しない
5. The Optimized Interpreter shall 入出力命令を含むループを最適化の対象から除外し、その反復回数と入出力の順序を素朴実装と同一に保つ

### Requirement 10: 停止状態と実行の観測
**Objective:** As a 結果を比較する利用者, I want 実行後の状態を変更せずに読み取れ、停止後の挙動が決定的であること, so that 結果の確認と素朴実装との比較が簡潔にできる

#### Acceptance Criteria
1. When 最適化インタプリタを生成する, the Optimized Interpreter shall テープの 30,000 セルをすべて 0、データポインタ・入力読取位置・実行数を 0、出力を空とした初期状態を持つ
2. When プログラムを完走まで実行し正常に終了する, the Optimized Interpreter shall 正常終了を示す結果を返す
3. When すでに停止状態の機械で完走までの実行を要求する, the Optimized Interpreter shall 状態を変更せず正常終了を示す結果を返す
4. When 実行後の状態を参照する, the Optimized Interpreter shall 状態を変更せずに、データポインタ、現在セルの値、停止しているかどうか、素朴実装換算の実行数、方式固有の実行単位数、入力読取位置、出力バイト列全体、テープ全体を読み取り専用で返す
5. When 実行後の状態を参照する, the Optimized Interpreter shall 停止状態では素朴実装の `pc` と同じ値(命令数)を、エラーで中断した場合はエラーが報告する `pc` を、素朴実装の命令添字で参照できるようにする
6. When 同一のプログラム・入力・設定から複数の機械を生成する, the Optimized Interpreter shall 互いに独立した状態を持たせ、一方の実行が他方に影響しない
7. When 同一の IR から複数の機械を生成する, the Optimized Interpreter shall IR を変更せず、各機械の実行を互いに独立させる

### Requirement 11: 差分テスト(正規テスト)
**Objective:** As a プロジェクト管理者, I want 最適化の正しさが素朴インタプリタとの差分テストで継続的に検証されること, so that 最適化による意味の変化を確実に検出できる

#### Acceptance Criteria
1. The Optimized Interpreter shall 素朴インタプリタをオラクルとし、同一のプログラム・入力・設定について Requirement 5 の観測結果を比較する差分テストを正規テストとして含む
2. The Optimized Interpreter shall 命令数が小さい範囲のプログラム(括弧が対応するものすべて)を網羅的に列挙して比較する差分テストを含む
3. The Optimized Interpreter shall 再現可能な擬似乱数(固定シード)から生成した小規模プログラムを比較する差分テストを含み、不一致時にはシードまたは生成されたソースから再現できる情報を失敗メッセージに含める
4. The Optimized Interpreter shall 各最適化パターン(連続命令統合、クリア、移動/加算/乗算、走査)と、それらのパターンに近いが条件を満たさない形(非適用の境界)を対象にした差分テストを含む
5. The Optimized Interpreter shall ステップ上限を 0 から素朴実装の必要実行数より大きい値まで 1 ずつ変化させ、統合操作・最適化ループの途中を含む全境界で素朴実装と比較する差分テストを含む
6. The Optimized Interpreter shall ポインタが左端・右端に達する(範囲外エラー、境界上の成功の両方を含む)プログラムを対象にした差分テストを含む
7. The Optimized Interpreter shall 入力が空、入力が尽きる、および 0 と 255 を含むバイト列の入力を対象にした差分テストを含む
8. When 不具合を発見し修正する, the Optimized Interpreter shall 最小の再現プログラムと対応する回帰テストを追加できる構成でテストを管理する
9. When ワークスペース全体のテストを実行する, the Optimized Interpreter shall 差分テストを含むすべての正規テストが成功し、ベンチマークは実行されない

### Requirement 12: ベンチマーク
**Objective:** As a 実行方式の性能差を学習・比較する利用者, I want 素朴実装と最適化実装の実行時間と実行数を同一プログラムで比較できること, so that 最適化の効果を数値で確認できる

#### Acceptance Criteria
1. The Optimized Interpreter shall 正規テストとは別の手順で実行できるベンチマークを提供し、通常のワークスペースのテスト実行ではそれを実行しない
2. When ベンチマークを実行する, the Optimized Interpreter shall 固定された複数のベンチマーク用プログラム(連続命令、クリア、移動/加算/乗算、走査、一般ループをそれぞれ多く含むもの)について、素朴実装と最適化実装の実行時間を同一の入力・設定で計測する
3. When ベンチマークを実行する, the Optimized Interpreter shall プログラムごとに、素朴実装換算の実行数、方式固有の実行単位数、実行時間の比を、どの値がどの定義かを明示して報告する
4. When ベンチマークが計測を行う, the Optimized Interpreter shall 計測の前に素朴実装と最適化実装の観測結果が一致することを確認し、一致しない場合は計測結果を報告せずに失敗として扱う
5. The Optimized Interpreter shall ベンチマークの結果(実行時間)の大小を、正規テストの成否判定に用いない
6. The Optimized Interpreter shall ベンチマークの実行方法を、再現可能な手順として利用者が参照できる文書に記載する
7. The Optimized Interpreter shall ベンチマーク結果が最適化の正しさの証明ではないことを、ベンチマークの出力または文書に明記する

### Requirement 13: エラーと診断情報
**Objective:** As a 利用者および後続 feature の開発者, I want 最適化インタプリタのエラーが素朴実装と同じ型・同じ情報で扱えること, so that CLI・GUI・比較画面が実行方式を意識せずにエラーを分類・表示できる

#### Acceptance Criteria
1. The Optimized Interpreter shall 実行エラーとして、素朴実装と同一のエラー型(ポインタ範囲外、ステップ制限超過)を使用し、新しい実行エラー種別を追加しない
2. The Optimized Interpreter shall 解析エラー(括弧不一致)を新たに定義せず、`brainfuck-core` の解析が返すエラーを唯一の解析エラーとする
3. The Optimized Interpreter shall エラーの内容(フィールドおよび表示文字列)に、ソース全文、入力バイト列、絶対パスなどの機微情報を含めない

### Requirement 14: 素朴意味論の再生とコスト式の公開
**Objective:** As a 下流 feature(`compiler-comparison` など)の開発者, I want 素朴意味論の再生と操作ごとの素朴換算コスト式が公開された単一の実装として提供されること, so that 素朴意味論やコスト式を別 crate に複製せずに済み、複製間のずれによる不一致を避けられる

#### Acceptance Criteria
1. The Optimized Interpreter shall 素朴意味論の再生(命令単位の実行)を、`brainfuck-core` の `Program` を入力とする公開モジュールとして提供し、その公開型を呼び出し元 crate が実行状態の保持と結果の取得に使えるようにする
2. The Optimized Interpreter shall 再生の実行状態(テープ、データポインタ、素朴換算の実行数、入力、入力読取位置、出力)を、初期状態から生成できるだけでなく、途中の状態の各項目から検証付きで生成でき、すべての項目を読み取りと取り出しで返せるようにする。If 途中状態の項目が不変条件(テープ長 30,000、データポインタが範囲内、入力読取位置が入力長以下)を満たさない, then the Optimized Interpreter shall panic せずに、不変条件違反を示すエラーを返す
3. When 再生を実行する, the Optimized Interpreter shall `brainfuck-core` の 1 命令実行と同一の判定順序(停止 → 上限 → 検証 → 更新)で命令を 1 個ずつ進め、エラーは検証段で返して実行状態を変更せず、成功した命令のみ実行数に加算する
4. When 初期状態から再生を実行する、または素朴実装が途中まで実行した状態から再生を実行する, the Optimized Interpreter shall 終了種別、エラー値のフィールド、終了時の命令添字、実行状態のすべての項目を、同じ状態・同じ上限から継続した素朴実装の完走実行の結果と完全に一致させる
5. The Optimized Interpreter shall 再生の結果として、終了時の命令添字(素朴実装の `pc`)と `Result<StopReason, RuntimeError>` を返す。上限は指定なし(制限なし)または数値で受け取り、開始位置が命令数以上の場合は panic せず停止として扱い、状態を変更しない
6. The Optimized Interpreter shall 最適化機械の縮退経路(最適化操作を一括適用できない場合の素朴再生)を、公開された再生の実装そのものとして実現し、同一 crate 内に素朴意味論の第二の実装を持たない
7. The Optimized Interpreter shall IR 操作と開始時の現在セル値(走査ループのみさらに反復数)から、その操作が表す素朴実装換算の実行数を求める純粋な公開ヘルパを提供し、各操作の閉形式(Requirement 3、4、6 のコスト式)を文書に明記する。このヘルパの結果は、同じ操作を素朴実装が実行した場合に数える実行数と一致する
8. The Optimized Interpreter shall IR 操作について、移動を伴う操作(同方向移動、移動/加算/乗算ループ)がテープ範囲内に収まるために必要な、現在位置からの相対的な到達範囲を返す純粋な公開ヘルパを提供する
9. The Optimized Interpreter shall 最適化機械の一括適用におけるコスト計算と範囲条件の評価に、要件 14.7 および 14.8 の公開ヘルパを用い、同じ式を別の場所に重複して持たない

