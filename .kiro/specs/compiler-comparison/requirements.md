# Requirements Document

## Project Description (Input)
# Brief: compiler-comparison

## Problem
同一 Brainfuck プログラムを複数の実行方式(素朴/最適化/WASM/ネイティブ)で動かしたときの差を比較・観察できない。

## Current State
最適化インタプリタと GUI が前提。コンパイラは未着手(Phase 4)。

## Desired Outcome
WASM とネイティブ AOT の少なくとも一方が Phase 1 と同一の観測結果を返し、GUI の比較画面に方式、実行時間、実行命令数(定義が異なる場合は明示)、終了状態が表示される。

## Approach
IR からターゲットコードを生成(WASM 優先を検討、設計フェーズで WASM/AOT の選択を確定)。素朴インタプリタをオラクルに差分テスト。比較画面を desktop-gui に追加。

## Scope
- **In**: 少なくとも一方のコンパイラバックエンド、同一性テスト、比較画面、実行メトリクス収集
- **Out**: 高度な最適化、JIT、パッケージ配布

## Boundary Candidates
- コード生成バックエンド
- 実行ランタイム境界(I/O・テープ・制限)
- 比較画面

## Out of Boundary
- 基準意味論(core)、IR 最適化(optimized-interpreter)、基本 GUI(desktop-gui)

## Upstream / Downstream
- **Upstream**: optimized-interpreter、desktop-gui
- **Downstream**: なし

## Existing Spec Touchpoints
- **Extends**: desktop-gui(比較画面の追加)
- **Adjacent**: optimized-interpreter

## Constraints
任意コード実行の非目標に反しないサンドボックス方針、ステップ/資源制限の維持、観測結果の完全一致。

## Introduction

`compiler-comparison` は、Brainfuck Studio の Phase 4 として、Brainfuck ソースをコンパイルして実行する「コンパイル実行方式」を追加し、同一のプログラム・入力・命令数上限を複数の実行方式(素朴インタプリタ、最適化インタプリタ、コンパイル実行方式)で実行した結果を、デスクトップアプリの比較画面で並べて観察できるようにする。

コンパイル実行方式は、素朴インタプリタ(`brainfuck-core`)と**観測的に完全に一致**しなければならない。すなわち、同一のソース・入力・上限に対して、出力バイト列、最終テープ、終了状態(正常停止、ポインタ範囲外、命令数上限超過とそのエラー値)、データポインタ、入力読取位置、素朴実装換算の実行数が一致する。コンパイル実行方式は信頼できないソースから生成されるため、「任意コード実行」「無制限の計算資源」を提供しないという非目標を守り、生成物が実行できることを Brainfuck の意味論の範囲(テープ、入出力バイト列、命令数上限)に限定する。

利用者は、実行方式による時間・実行数・終了状態の差を学習・比較したい学習者と、比較画面の保守者である。素朴インタプリタは正解オラクルであり、本 feature はその意味論・公開契約も、最適化インタプリタ(`optimized-interpreter`)と基本 GUI(`desktop-gui`)の既存の挙動も変更しない。

## Boundary Context

- **In scope**:
  - Brainfuck プログラムを、素朴インタプリタと同一の観測結果を返すコンパイル実行方式で実行すること(少なくとも 1 方式)
  - コンパイル実行方式の安全性方針(生成物の実行範囲の限定、資源制限、命令数上限の維持)
  - 実行方式間の観測結果の一致判定と、実行方式ごとの実行メトリクス(準備時間、実行時間、実行数、成果物の大きさ)の収集
  - 実行数の定義の明示(素朴実装換算の実行数と方式固有の実行単位数)
  - 素朴インタプリタをオラクルとした差分テスト
  - `desktop-gui` の既存アプリへ追加する比較画面(方式、実行時間、実行命令数とその定義、終了状態の表示)
- **Out of scope**:
  - 素朴インタプリタの意味論・公開契約の変更(`brainfuck-core`)、最適化インタプリタの IR 最適化の拡張・変更(`optimized-interpreter`)、デバッガ画面・既存コマンドの変更(`desktop-gui`)
  - 高度な最適化(`optimized-interpreter` の IR を超える最適化)、実行時に機械語を生成する JIT、成果物の配布(パッケージ化、インストーラ)
  - コンパイル実行方式での 1 命令ずつのステップ実行、ブレークポイント、実行中の一時停止
  - 外部のコンパイラ・リンカなど外部ツールチェーンの実行、利用者が供給するコードの実行
  - 比較結果の永続化・エクスポート、ネットワーク通信
  - 実行時間の大小を保証すること(実行時間は観察対象であり、合否の基準ではない)
- **Adjacent expectations**:
  - `brainfuck-core` は、解析済みプログラム、実行設定(命令数上限)、エラー型、実行後の観測 API を提供し、素朴実装の結果を基準(オラクル)として供給する
  - `optimized-interpreter` は、素朴実装換算の実行数と方式固有の実行単位数という 2 種類の実行数を報告し、解析済みプログラムの中間表現(IR)を読み取り専用で提供する。本 feature はその定義を実行数の表示に用い、定義を変更しない
  - `desktop-gui` は、ソース・入力・命令数上限を共有する仕組みと、画面を追加できる拡張点、共通のエラー表現を提供する。本 feature はそれらを利用して画面とコマンドを追加するが、既存のデバッガ画面の挙動を変えない

## Requirements

### Requirement 1: 基盤・依存・安全性の制約
**Objective:** As a 後続の保守者, I want コンパイル実行方式と比較機能が既存の実行方式・画面を変更せずに安全に追加されること, so that オラクルとデバッガ画面の信頼性を保ったまま実行方式を増やせる

#### Acceptance Criteria
1. The Compiler Comparison shall `brainfuck-core` の意味論・公開型・公開関数、`optimized-interpreter` の意味論・公開型・公開関数、および `desktop-gui` の既存コマンド・既存の共通表現を変更しない
2. The Compiler Comparison shall コンパイル実行方式と比較の中核機能を、デスクトップ UI フレームワーク、標準入力、ファイルシステム、ネットワークに依存せずに利用できる形で提供する
3. The Compiler Comparison shall 入力を呼び出し元が供給するバイト列として受け取り、出力を呼び出し元が取得するバイト列として返す
4. The Compiler Comparison shall 自身が追加するコードに `unsafe` を一切含まない
5. When 外部プロセスを起動せずにテストを実行する, the Compiler Comparison shall すべての正規テスト(単体・差分・回帰)を、比較画面の起動やブラウザ表示なしで完走できる
6. The Compiler Comparison shall テープ長(30,000 セル)、セル幅(8 ビット符号なし)、EOF 時の入力値(0)、セル演算のラップ動作を素朴実装と同一に保つ
7. The Compiler Comparison shall 外部入力(ソース長、入力バイト列)の大きさに比例しない量のメモリをテープ用に確保しない
8. The Compiler Comparison shall デバッガ用の API を提供するクレート(`desktop-gui` の `brainfuck-gui-api`)に、比較機能・WASM の生成と実行・実行エンジンなどの比較機能固有の依存を持ち込まず、比較の契約(DTO)とサービスをそれとは別のクレートに分離して提供する。デバッガ用 API のビルドとテストは、比較機能の有無に影響されない
9. The Compiler Comparison shall 素朴意味論の再生、操作の素朴換算コストと到達範囲の式、テープ窓、出力チャンク、解析エラー・実行エラーの DTO 変換、Base64 コーデックと制限値を、それぞれの所有 spec が公開する単一の実装から再利用し、自身で複製しない。ただし WASM 命令へ転記が不可避なコスト式については、所有 spec の公開ヘルパを仕様および検証のオラクルとして用いる

### Requirement 2: コンパイル実行方式の提供
**Objective:** As a Brainfuck 学習者, I want ソースをコンパイルして実行する方式で同じプログラムを動かせること, so that インタプリタとコンパイル実行の違いを観察できる

#### Acceptance Criteria
1. The Compiler Comparison shall 解析済みの Brainfuck プログラムをコンパイルして実行する方式を、少なくとも 1 つ提供する
2. When 解析済みのプログラムをコンパイルする, the Compiler Comparison shall 入力・上限の値に依存せず、同一のプログラムから同一の成果物を毎回生成する
3. When コンパイルした成果物を、異なる入力・異なる命令数上限で複数回実行する, the Compiler Comparison shall 再コンパイルせずに実行でき、各実行を互いに独立させる
4. When 命令が 0 個のプログラムを実行する, the Compiler Comparison shall 初期状態のまま正常停止の結果を返す
5. If コンパイルがプログラムの規模または構造により成果物の上限を超える, then the Compiler Comparison shall その方式を「未対応」と理由付きで報告し、素朴実装と異なる結果を観測結果として返さない
6. The Compiler Comparison shall 括弧エラーなどの解析エラーを新たに定義せず、`brainfuck-core` の解析が返すエラーを唯一の解析エラーとする
7. The Compiler Comparison shall コンパイルの成否にかかわらず、後続の他方式の実行結果を変更しない

### Requirement 3: 素朴実装との観測的一致
**Objective:** As a 実行方式の比較を行う利用者, I want コンパイル実行方式の結果が素朴インタプリタの結果と完全に一致すること, so that 方式を切り替えても結果が変わらず、素朴実装を唯一のオラクルとして比較できる

#### Acceptance Criteria
1. When 同一のプログラム・入力・命令数上限でコンパイル実行方式と素朴インタプリタを実行する, the Compiler Comparison shall 出力バイト列、最終テープ(30,000 セル全体)、終了種別(正常停止、ポインタ範囲外、命令数上限超過)を素朴実装と完全に一致させる
2. When 実行がエラーで中断した, the Compiler Comparison shall エラー発生時点の出力バイト列、テープ全体、データポインタ、入力読取位置、素朴実装換算の実行数を、素朴実装が同じエラーで中断した時点の状態と完全に一致させる
3. The Compiler Comparison shall ポインタ範囲外エラーの `pc`・`dp`・移動方向と、命令数上限超過エラーの上限値・`pc` を、素朴実装と完全に一致させる。ここで `pc` は素朴実装の命令添字で表す
4. When 実行が正常に停止した, the Compiler Comparison shall 最終的なデータポインタ、入力読取位置、素朴実装換算の実行数、`pc`(命令数)を素朴実装と完全に一致させる
5. The Compiler Comparison shall コンパイル実行方式の観測結果を、素朴実装の観測結果との直接比較(同値性)で検証できる形で提供する
6. The Compiler Comparison shall 実行中のエラー(ポインタ範囲外、命令数上限超過)以外の失敗として、素朴実装が成功する実行を異常終了させない
7. If コンパイル実行方式の観測結果が素朴実装と一致しない, then the Compiler Comparison shall その不一致を正しい結果として扱わず、不一致の項目を呼び出し元が識別できる形で報告する

### Requirement 4: 命令数上限と資源制限の維持
**Objective:** As a 信頼できないソースを実行する利用者, I want コンパイル実行でも命令数上限と資源制限が素朴実装と同じ境界で働くこと, so that コンパイル実行によって上限を回避したり、無制限の計算資源を消費したりできない

#### Acceptance Criteria
1. The Compiler Comparison shall 命令数の上限を素朴実装換算の実行数に対して適用し、素朴実装と同じ境界で停止させる
2. When 上限がちょうど素朴実装が必要とする実行数と等しい, the Compiler Comparison shall エラーを返さずに正常停止として扱う
3. When 上限が素朴実装が必要とする実行数より 1 小さい, the Compiler Comparison shall 命令数上限超過エラーを返し、上限と同数の命令に相当する実行を行った時点の状態で中断する
4. When 上限が、連続する命令のまとまりや最適化された反復の実行の途中に当たる, the Compiler Comparison shall 素朴実装が同じ上限で中断した時点と同一の状態(出力、テープ、データポインタ、入力読取位置、実行数、エラーの `pc`)で中断する
5. When 上限が 0 に設定されており、命令が 1 個以上ある, the Compiler Comparison shall 命令を一つも実行せずに命令数上限超過エラーを返す
6. The Compiler Comparison shall 比較の実行において命令数の上限を必須とし、上限なしの実行、および比較用に定めた最大値を超える上限の実行を行わない
7. The Compiler Comparison shall コンパイルした成果物の実行が使用するメモリを、テープと入出力のバッファに必要な量に限定し、成果物の実行中に増やさない
8. The Compiler Comparison shall コンパイルした成果物の実行時間を、命令数の上限に比例する有限の量に制限し、成果物の不具合による無限実行が命令数の上限の機構だけに依存しないようにする
9. If コンパイルした成果物の実行が資源の制限(実行量の監視、メモリ、出力量)に違反して中断された, then the Compiler Comparison shall それを Brainfuck の実行結果ではなく「方式の失敗」として、素朴実装のエラーと区別して報告する

### Requirement 5: 実行範囲の限定(サンドボックス方針)
**Objective:** As a 利用者, I want コンパイル実行方式が任意のコードを実行したり、環境に副作用を及ぼしたりしないこと, so that 信頼できないソースを安全に比較できる

#### Acceptance Criteria
1. The Compiler Comparison shall コンパイル実行の対象を、Brainfuck ソースから本 feature 自身が生成した成果物に限定し、利用者または外部が供給する実行コードを読み込まない
2. The Compiler Comparison shall 成果物が外部と行える操作を、入力バイトの取得と出力バイトの送出に限定し、ファイル、ネットワーク、プロセス、環境変数、時刻、乱数へのアクセスを成果物へ提供しない
3. The Compiler Comparison shall 成果物の生成後・実行前に、成果物が宣言する外部参照と資源の宣言が方針の範囲内であることを検査し、範囲外であれば実行しない
4. The Compiler Comparison shall コンパイルに外部のコンパイラ、リンカ、シェルなどの外部プロセスを使用しない
5. The Compiler Comparison shall コンパイル実行の過程で、ソース全文・入力バイト列・絶対パスなどの機微情報を診断やログへ出力しない
6. The Compiler Comparison shall コンパイル実行方式の採用方式と、その安全性の根拠(実行範囲の限定、資源制限)を文書として参照できるようにする

### Requirement 6: 入出力の意味保存
**Objective:** As a 後続の保守者, I want コンパイル実行でも入出力がバイト列として素朴実装と同一に扱われること, so that CLI・GUI・比較画面で同一入力から同一出力が得られる

#### Acceptance Criteria
1. When `.` を実行する, the Compiler Comparison shall 現在セルの値を 1 バイトとして出力バイト列の末尾へ、実行された順序で追加する
2. When `,` を実行し未読の入力バイトが残っている, the Compiler Comparison shall 次の 1 バイトを現在セルへ格納し、入力読取位置を 1 進める
3. If `,` を実行したとき入力バイトを使い果たしている, then the Compiler Comparison shall 現在セルに 0 を格納し、入力読取位置を進めず、エラーを発生させない
4. The Compiler Comparison shall 入力および出力の各バイトを 0 から 255 の任意の値として扱い、UTF-8 として有効であることを要求しない
5. The Compiler Comparison shall 入出力命令の実行順序と実行回数を、最適化や生成の有無によらず素朴実装と同一に保つ

### Requirement 7: 実行数の定義と報告
**Objective:** As a 実行方式の比較を行う利用者, I want 実行方式ごとの「実行数」の意味が明確に区別されていること, so that 方式間の数値を誤解なく比較できる

#### Acceptance Criteria
1. The Compiler Comparison shall すべての実行方式について、「素朴実装換算の実行数」(素朴実装が同じ実行で数える実行数と同じ値)を報告する
2. The Compiler Comparison shall 素朴インタプリタについては方式固有の実行単位として「実行した命令数」を、最適化インタプリタについては「IR 操作の実行回数」を、素朴実装換算の実行数とは別の値として報告する
3. Where コンパイル実行方式が方式固有の実行単位を数えない, the Compiler Comparison shall 「方式固有の実行単位なし」であることを、値の欠落ではなく明示的な状態として報告する
4. When 実行がエラーで中断した, the Compiler Comparison shall 素朴実装換算の実行数を、素朴実装が同一時点で数える値と一致させる
5. The Compiler Comparison shall 報告する各実行数について、どの定義の数値であるかを名称または文書によって呼び出し元が判別できるようにする
6. The Compiler Comparison shall 素朴実装換算の実行数を方式間の一致判定の対象とし、方式固有の実行単位数を一致判定の対象としない

### Requirement 8: 実行メトリクスの収集
**Objective:** As a 実行方式の性能差を学習する利用者, I want 方式ごとの準備時間と実行時間を同一条件で計測できること, so that コンパイルと実行のコストの違いを数値で確認できる

#### Acceptance Criteria
1. When 比較を実行する, the Compiler Comparison shall 方式ごとに、準備にかかった時間(解析済みプログラムから実行可能になるまでのコンパイル・変換の時間)と、実行にかかった時間を別の値として計測する
2. When 比較を実行する, the Compiler Comparison shall すべての方式を、同一のプログラム・入力・命令数上限で実行する
3. Where 同一方式の実行を複数回繰り返す設定が指定されている, the Compiler Comparison shall 各回の実行時間のうち中央値を実行時間として報告し、回数を併せて報告する
4. Where 繰り返しの設定が指定されていない, the Compiler Comparison shall 既定の回数で繰り返して計測する
5. When 同一方式を複数回実行する, the Compiler Comparison shall すべての回の観測結果が互いに一致することを確認し、一致しない場合はその方式を「方式の失敗」として報告する
6. Where コンパイル実行方式が成果物を生成する, the Compiler Comparison shall 成果物の大きさ(バイト数)を併せて報告する
7. The Compiler Comparison shall 計測した時間の大小を、一致判定・テストの合否・方式の成否の判定に用いない
8. The Compiler Comparison shall 計測の対象に、結果の整形・転送・表示にかかる時間を含めない

### Requirement 9: 比較の実行と一致判定
**Objective:** As a 実行方式の比較を行う利用者, I want 選んだ方式を同じ条件で実行し、結果の一致・不一致が分かること, so that どの方式がどの結果を返したかを一目で確認できる

#### Acceptance Criteria
1. When 比較を要求する, the Compiler Comparison shall ソース、入力バイト列、命令数上限、実行する方式の選択、繰り返し回数を受け取り、方式ごとの結果を 1 つの応答として返す
2. The Compiler Comparison shall 素朴インタプリタを常に基準(オラクル)として実行し、他の方式の結果を基準の結果と比較する
3. Where 実行する方式の選択に最適化インタプリタまたはコンパイル実行方式が含まれている, the Compiler Comparison shall 選択された方式のみを基準に加えて実行する
4. When 方式の結果が基準と完全に一致する, the Compiler Comparison shall その方式を「一致」として報告する
5. If 方式の結果が基準と一致しない, then the Compiler Comparison shall その方式を「不一致」とし、どの観測項目(終了種別、エラー値、`pc`、`dp`、現在セル、実行数、入力読取位置、出力、テープ)が異なるかを報告する
6. If ソースに括弧の不整合などの解析エラーがある, then the Compiler Comparison shall どの方式も実行せず、`brainfuck-core` が報告する種類・命令添字・位置を含む解析エラーとして報告する
7. If ある方式が未対応または方式の失敗となった, then the Compiler Comparison shall その方式を他の方式の結果と区別して報告し、他の方式の結果の報告を妨げない
8. When 比較が完了する, the Compiler Comparison shall すべての実行方式が基準と一致したかどうかを 1 つの総合結果として報告する
9. The Compiler Comparison shall 比較の応答に含める出力バイト列とテープを、巨大な量を高頻度に転送しないよう、末尾の一部と全体の長さ、およびテープのポインタ周辺の窓に限定して返す
10. The Compiler Comparison shall 基準(素朴インタプリタ)の結果を、同じソース・入力・命令数上限で CLI および GUI のデバッガ画面が返す結果(出力、終了時の `pc` / `dp` / 実行数、失敗の種類)と一致させる
11. When 同一の要求を繰り返す, the Compiler Comparison shall 実行時間を除き、同一の結果(一致判定、終了状態、実行数、出力、テープ)を返す

### Requirement 10: 比較画面の表示
**Objective:** As a Brainfuck 学習者, I want 比較画面で方式ごとの実行時間・実行数・終了状態を並べて見られること, so that 実行方式の違いを学習できる

#### Acceptance Criteria
1. The Compiler Comparison shall デスクトップアプリに、既存のデバッガ画面とは別に「比較」画面を追加し、画面の切り替え操作から到達できるようにする
2. The Compiler Comparison shall 比較画面で、方式ごとに、方式名、準備時間、実行時間(繰り返し回数を含む)、素朴実装換算の実行数、方式固有の実行単位(名称と値)、終了状態、基準との一致・不一致を表示する
3. When 方式の終了状態を表示する, the Compiler Comparison shall 終了種別(正常停止、ポインタ範囲外、命令数上限超過)、エラーのフィールド(`pc` / `dp` / 移動方向、上限 / `pc`)、最終の `pc` / `dp` / 現在セル / 入力読取位置、出力の長さと末尾、ポインタ周辺のテープを利用者が確認できるようにする
4. The Compiler Comparison shall 比較画面に、実行数の 2 つの定義(素朴実装換算と方式固有)の意味を説明する凡例を表示する
5. Where コンパイル実行方式が方式固有の実行単位を数えない, the Compiler Comparison shall 比較画面でその状態を「なし」などの明示的な表示にし、空欄にしない
6. If 方式が不一致である, then the Compiler Comparison shall 比較画面で、色だけに頼らない表示(文言・記号)で不一致と、異なる観測項目を示す
7. If 方式が未対応または方式の失敗である, then the Compiler Comparison shall 比較画面で、その理由の分類を、一致・不一致とは別の表示として示す
8. When 総合結果が全方式の一致である, the Compiler Comparison shall 比較画面でそれを一致している旨として示す
9. If 解析エラーが報告された, then the Compiler Comparison shall 比較画面で種類・命令添字・行/列を表示し、方式ごとの結果を表示しない
10. The Compiler Comparison shall 比較画面の表示言語を日本語とする
11. The Compiler Comparison shall 比較画面の実行時間の大小について、順位づけや優劣の判定を表示しない

### Requirement 11: 比較画面の操作と入力共有
**Objective:** As a Brainfuck 学習者, I want デバッガ画面で編集したソース・入力・上限をそのまま比較できること, so that 同じプログラムを別の観点で確認できる

#### Acceptance Criteria
1. The Compiler Comparison shall 比較画面で、デバッガ画面と共有されているソース(未保存の編集内容を含む)、入力、命令数上限を使用する
2. When 利用者が比較の実行を操作する, the Compiler Comparison shall 共有されているソース・入力・命令数上限、選択された方式、繰り返し回数で比較を開始する
3. While 比較が実行中である, the Compiler Comparison shall 実行中であることを表示し、比較の再実行の操作を無効にする
4. While 比較が実行中である, the Compiler Comparison shall 画面の切り替え、およびデバッガ画面の操作を妨げない
5. If 命令数の上限が「上限なし」または不正な値である, then the Compiler Comparison shall 比較の実行を無効にし、理由を比較画面に表示する
6. If 命令数の上限が比較用の最大値を超えている, then the Compiler Comparison shall 比較の実行を無効にし、最大値と理由を比較画面に表示する
7. If 入力が不正である(16 進入力の不正など), then the Compiler Comparison shall 比較の実行を無効にし、理由を比較画面に表示する
8. When 共有されているソース・入力・上限が比較結果の取得後に変更された, the Compiler Comparison shall 表示中の結果が現在の内容と異なる(古い)ことを示す
9. The Compiler Comparison shall 比較画面で、実行する方式(最適化インタプリタ、コンパイル実行方式)の選択と、繰り返し回数の指定を提供する
10. The Compiler Comparison shall すべての操作をキーボードで実行できるようにする
11. The Compiler Comparison shall 比較の実行がデバッガ画面のセッション・実行状態・表示内容を変更しないようにする

### Requirement 12: エラーと診断情報
**Objective:** As a 利用者および保守者, I want 比較機能のエラーが種類ごとに区別され、機微情報を含まないこと, so that 原因を分類して対処でき、安全に診断できる

#### Acceptance Criteria
1. The Compiler Comparison shall 実行エラー(素朴実装と同じ型のポインタ範囲外・命令数上限超過)、解析エラー、方式の失敗、方式の未対応、操作エラー(不正な要求)を互いに区別して報告する
2. The Compiler Comparison shall 実行エラー・解析エラーの内容に、素朴実装と同一の型とフィールドを使用し、新しい実行エラー種別を追加しない
3. If 比較の要求の値が許容範囲外である(ソース・入力の大きさ、命令数上限、繰り返し回数、不正な入力表現), then the Compiler Comparison shall 要求を拒否し、どの項目が不正かを示す操作エラーを返し、状態を変更しない
4. The Compiler Comparison shall エラーおよび診断の内容(フィールドと表示文字列)に、ソース全文、入力バイト列、絶対パスなどの機微情報を含めない
5. If 比較機能の内部で予期しない失敗が起きた, then the Compiler Comparison shall アプリを異常終了させず、内部エラーとして報告し、以降の比較を継続できる状態に保つ

### Requirement 13: 差分テスト(正規テスト)
**Objective:** As a プロジェクト管理者, I want コンパイル実行方式の正しさが素朴インタプリタとの差分テストで継続的に検証されること, so that 生成の誤りによる意味の変化を確実に検出できる

#### Acceptance Criteria
1. The Compiler Comparison shall 素朴インタプリタをオラクルとし、同一のプログラム・入力・命令数上限について Requirement 3 の観測結果を比較する差分テストを正規テストとして含む
2. The Compiler Comparison shall 命令数が小さい範囲のプログラム(括弧が対応するものすべて)を網羅的に列挙して比較する差分テストを含む
3. The Compiler Comparison shall 再現可能な擬似乱数(固定シード)から生成した小規模プログラムを比較する差分テストを含み、不一致時にはシードまたは生成されたソースから再現できる情報を失敗メッセージに含める
4. The Compiler Comparison shall 連続命令、クリア、移動/加算/乗算、走査の各パターンと、それらに近いが条件を満たさない形を対象にした差分テストを含む
5. The Compiler Comparison shall 命令数上限を 0 から素朴実装の必要実行数より大きい値まで 1 ずつ変化させ、連続命令や最適化された反復の途中を含む全境界で素朴実装と比較する差分テストを含む
6. The Compiler Comparison shall ポインタが左端・右端に達する(範囲外エラー、境界上の成功の両方を含む)プログラムを対象にした差分テストを含む
7. The Compiler Comparison shall 入力が空、入力が尽きる、および 0 と 255 を含むバイト列の入力を対象にした差分テストを含む
8. The Compiler Comparison shall 成果物が方針の範囲外の外部参照や資源の宣言を持たないこと、および資源の制限(メモリ、実行量の監視)が実際に働くことを検証するテストを含む
9. The Compiler Comparison shall 比較の一致判定が、意図的に異なる観測結果を不一致として検出できることを検証するテストを含む
10. When 不具合を発見し修正する, the Compiler Comparison shall 最小の再現プログラムと対応する回帰テストを追加できる構成でテストを管理する
11. When ワークスペース全体のテストを実行する, the Compiler Comparison shall 差分テストを含むすべての正規テストが成功し、実行時間の大小に依存しない

### Requirement 14: 文書
**Objective:** As a 利用者および保守者, I want 比較機能の使い方と方針が文書として参照できること, so that 再現可能な手順で比較・検証でき、方式の位置づけを理解できる

#### Acceptance Criteria
1. The Compiler Comparison shall 比較画面の使い方、比較できる方式、各方式の実行数の定義、比較用の上限値を、利用者が参照できる文書に記載する
2. The Compiler Comparison shall 採用したコンパイル実行方式の概要、安全性の方針、既知の制約(未対応となる条件、時間の読み方)を、利用者が参照できる文書に記載する
3. The Compiler Comparison shall 実行時間は観察の対象であり、最適化やコンパイルの正しさの証明でも性能の保証でもないことを、画面または文書に明記する
4. The Compiler Comparison shall 手動での確認手順(代表的なプログラムでの比較、不一致・未対応の表示確認)を文書に記載する
