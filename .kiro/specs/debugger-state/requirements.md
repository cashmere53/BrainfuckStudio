# Requirements Document

## Introduction

`debugger-state` は、GUI がフリーズせずに Brainfuck プログラムを実行・停止・ステップできるようにするための「デバッグ契約」を、`brainfuck-core` の後方互換な拡張として提供する。本 spec は、停止理由(`Halted` / `StepLimitReached` / `Breakpoint` / `Cancelled`)、命令添字に正規化されたブレークポイント、既定 1,000 命令のバッチ実行、呼び出し側が供給する指示によるキャンセル、ポインタ周辺(既定 ±16 セル)のテープ表示窓、命令添字とソース範囲(バイトオフセット・行・列)の相互変換を、GUI・スレッド生成・標準入出力なしで利用できる形で定義する。

デバッガの単位は「実行された命令」であり、ソース文字ではない。コメント文字上には停止しない。ここで追加する機能は `brainfuck-core` の意味論(出力・最終テープ・エラー)を一切変更せず、同じプログラム・入力・設定から得られる結果は、デバッグ機能の有無や分割の仕方によらず一致しなければならない。

利用者は、デバッグ状態を組み込む後続コンポーネントの開発者(主に `desktop-gui`)と、本契約を検証するテスト実装者である。

## Boundary Context

- **In scope**:
  - デバッグセッション(1 つのプログラムの実行状態と、ブレークポイント・直近の停止理由の保持)とリセット
  - 停止理由の定義と、ブレークポイント集合(命令添字)の管理
  - ブレークポイント停止、キャンセル、バッチ実行(既定 1,000 命令)、1 命令ステップ(Step Into)
  - ポインタ周辺のテープ表示窓(既定 ±16 セル)と、状態ビューの取得。機械を持たない呼び出し元(テープのバイト列のみを持つ観測値など)向けの、テープのバイト列とポインタからの表示窓の取得
  - 命令添字 ↔ ソース範囲(バイトオフセット・行・列・エディタ向けオフセット)の変換と、ソース位置から命令添字へのブレークポイント正規化。プログラムを持たない場合(解析エラーの位置など)向けの、ソース文字列のみからのバイトオフセット → 位置の変換と、エディタ向け(UTF-16)オフセットからの命令の逆引き
  - 上記の自動テスト
- **Out of scope**:
  - Step Back / 可逆実行、Step Over / Step Out、条件付きブレークポイント、ウォッチポイント
  - Tauri コマンド、JSON DTO、IPC、UI、スレッドの生成と管理(`desktop-gui`)
  - Brainfuck の基本意味論・エラー型・ステップ制限の規約の変更(`brainfuck-core`)
  - CLI の挙動変更(`brainfuck-cli`)
  - 最適化実行・コンパイル実行でのデバッグ(`optimized-interpreter` / `compiler-comparison`)
- **Adjacent expectations**:
  - `brainfuck-core` は、命令列・各命令のソースバイトオフセット・括弧対応表・1 命令実行・スナップショットを提供し、その公開契約と意味論は本 feature によって変更されない。`run()` はこれまでどおりステップ制限をエラーとして返し、`Breakpoint` / `Cancelled` / `StepLimitReached` を返さない
  - `desktop-gui` は、本 feature の型を JSON DTO へ変換し、Run ループ(バッチの反復呼び出し)、Pause 操作(キャンセル指示の設定)、表示を所有する。本 feature はシリアライズ形式を定めない
  - `brainfuck-cli` は本 feature に依存せず、その挙動は変わらない
  - `desktop-gui` と `compiler-comparison` は、行・列・UTF-16 の位置計算、UTF-16 → 命令の逆引き、テープ表示窓の切り出しを自前で再実装せず、本 feature の公開関数に委ねる
  - 本 feature の追加物はすべて `brainfuck-core` crate の内部モジュールとして提供され、`brainfuck-core` のテスト用ディレクトリ(`tests/regression/` を含む)と衝突しない名前で自動テストを配置する

## Requirements

### Requirement 1: 後方互換な拡張と依存・安全性の制約
**Objective:** As a 後続 feature の開発者, I want 既存の実行エンジンの契約を壊さずにデバッグ機能が追加されること, so that CLI・最適化実装・GUI が同一の意味論を共有し続けられる

#### Acceptance Criteria
1. The Debugger State shall `brainfuck-core` が既に公開している型・関数・バリアント・フィールドのシグネチャと意味論を変更せず、追加のみで提供される
2. The Debugger State shall デスクトップ UI フレームワーク、標準入力、ファイルシステム、スレッドの生成に依存せずに利用できる
3. The Debugger State shall `unsafe` コードを一切含まない
4. When 外部プロセスを起動せずにテストを実行する, the Debugger State shall すべての単体・結合・回帰テストを完走できる
5. The Debugger State shall 特定のシリアライズ形式に依存しない値型として状態を提供し、JSON 等への変換を利用側が行えるようにする
6. The Debugger State shall 公開する型を、後続 feature が後方互換のまま拡張できる形で提供する
7. The Debugger State shall デバッグセッションおよび状態を表す値を、別の実行コンテキスト(スレッド)へ移動して利用できる形で提供する
8. While デバッグ機能を使用しない, the Debugger State shall `brainfuck-core` の `step` と `run` の結果(出力・テープ・`pc`・`dp`・実行済み命令数・エラー)を変化させない

### Requirement 2: デバッグセッションと停止理由
**Objective:** As a デバッガ実装者, I want 1 つのプログラムの実行状態・ブレークポイント・直近の停止理由を 1 つの単位として扱えること, so that 実行、観測、リセットを一貫して制御できる

#### Acceptance Criteria
1. When プログラム・入力バイト列・実行設定からデバッグセッションを生成する, the Debugger State shall `brainfuck-core` の機械を生成した直後と同一の初期状態を持ち、ブレークポイントを持たず、直近の停止理由を持たない状態にする
2. The Debugger State shall 停止理由として、正常終了(`Halted`)、バッチ命令数への到達(`StepLimitReached`)、ブレークポイント(`Breakpoint`)、キャンセル(`Cancelled`)を互いに区別できる形で提供する
3. The Debugger State shall `StepLimitReached` を「その実行呼び出しに指定した命令数を実行し終えたが、プログラムは未停止である」ことを示す停止理由として定義し、機械の実行設定が定める命令数上限(ステップ制限)の超過とは区別する
4. If 機械の実行設定が定める命令数上限に到達済みで未停止のまま次の命令を実行しようとする, then the Debugger State shall `brainfuck-core` と同じ実行エラー(ステップ制限超過)として返し、停止理由として返さない
5. If ポインタ範囲外が発生する, then the Debugger State shall `brainfuck-core` と同じ実行エラーとして返し、停止理由として返さない
6. When リセットを要求する, the Debugger State shall 機械を同じプログラム・同じ入力・同じ実行設定の初期状態へ戻し、直近の停止理由を消去し、設定済みのブレークポイントを保持する
7. When セッションの状態を参照する, the Debugger State shall スナップショット、テープ全体、出力バイト列全体、プログラム、直近の停止理由を、状態を変更せずに読み取り専用で返す
8. When 同一のプログラム・入力・設定から複数のセッションを生成する, the Debugger State shall 互いに独立した状態を持たせ、一方の操作が他方に影響しない
9. When バッチ実行またはステップが正常に完了する, the Debugger State shall 直近の停止理由を、その実行結果の停止理由へ更新する(Step Into が停止状態に達しない形で成功した場合は消去する)
10. If バッチ実行またはステップが実行エラーで終了する, then the Debugger State shall 直近の停止理由を消去する(機械の状態は変更しない)

### Requirement 3: ブレークポイント集合(命令添字)
**Objective:** As a デバッガ実装者, I want ブレークポイントを命令添字として管理できること, so that コメント文字ではなく実行される命令にのみ停止を設定できる

#### Acceptance Criteria
1. When 有効な命令添字(0 以上、命令数未満)にブレークポイントを追加する, the Debugger State shall その命令添字を集合へ加え、新規に追加されたことを呼び出し元が判別できる結果を返す
2. When 既に設定済みの命令添字にブレークポイントを追加する, the Debugger State shall 集合を変更せず、既に存在することを呼び出し元が判別できる結果を返す
3. If 命令数以上の命令添字にブレークポイントを追加しようとする, then the Debugger State shall 集合を変更せず、命令添字とプログラムの命令数を含むエラーを返す
4. When ブレークポイントを削除する, the Debugger State shall 設定済みなら削除して削除したことを、未設定なら何も変更せず該当なしであることを呼び出し元が判別できる結果を返す
5. When ブレークポイントを切り替える(トグルする), the Debugger State shall 未設定なら追加し、設定済みなら削除し、その結果を呼び出し元が判別できる形で返す
6. When ブレークポイントをすべて消去する, the Debugger State shall 集合を空にする
7. When ブレークポイントの一覧を取得する, the Debugger State shall 命令添字の昇順で重複なく返す
8. When 命令添字がブレークポイントであるかを問い合わせる, the Debugger State shall 実行状態を変更せずに結果を返す
9. While セッションが存在する, the Debugger State shall ブレークポイントの追加・削除・消去を、実行の前後およびリセットの前後のいずれでも受け付ける

### Requirement 4: バッチ実行と停止通知
**Objective:** As a GUI 実装者, I want 実行を一定の命令数ごとに区切って行い、区切りごとに状態を受け取れること, so that UI を固めずに長いプログラムを実行できる

#### Acceptance Criteria
1. The Debugger State shall バッチ実行の既定命令数を 1,000 として公開し、呼び出し元が 1 以上の任意の命令数を指定できるようにする
2. If バッチ実行の命令数として 0 が指定される, then the Debugger State shall 指定を受け付けず、機械の状態を変更しない
3. When バッチ実行を要求する, the Debugger State shall 指定された命令数を超えない範囲で、`brainfuck-core` の 1 命令実行と同一の意味論で命令を繰り返し実行する
4. When バッチ実行が終了する, the Debugger State shall 停止理由、そのバッチで実際に実行した命令数、終了後のスナップショットを呼び出し元へ通知する
5. When 停止していないプログラムで指定された命令数をすべて実行し終え、停止・ブレークポイント・キャンセルのいずれにも該当しない, the Debugger State shall `StepLimitReached` を通知する
6. When プログラムが停止状態に達する(最後の命令の実行によって達する場合を含み、その実行がバッチの最後の 1 命令であっても), the Debugger State shall `Halted` を通知する
7. When すでに停止状態のセッションでバッチ実行を要求する, the Debugger State shall 状態を変更せず、実行した命令数 0 で `Halted` を通知する
8. If バッチ実行中に実行エラー(ポインタ範囲外またはステップ制限超過)が発生する, then the Debugger State shall その時点で実行を中断し、当該エラーとそのバッチで実行済みの命令数を呼び出し元へ通知し、機械はエラー発生直前の状態を保持する
9. When 実行エラーで中断したセッションでバッチ実行を再度要求する, the Debugger State shall 状態を変更せず、同じ実行エラーを通知する
10. When 同一のプログラム・入力・設定で、ブレークポイントなし・キャンセルなしの条件のもと、任意のバッチ命令数で `StepLimitReached` 以外が通知されるまでバッチ実行を繰り返す, the Debugger State shall `brainfuck-core` の `run` と最終的な出力・テープ・`pc`・`dp`・実行済み命令数・エラー種別を完全に一致させる
11. The Debugger State shall 実行済み命令数を、実際に実行した命令 1 個につき 1 ずつ加算し、コメント文字や通知の回数を数えない

### Requirement 5: ブレークポイントでの停止
**Objective:** As a Brainfuck 学習者, I want 指定した命令の直前でプログラムが止まること, so that その時点の状態を観察できる

#### Acceptance Criteria
1. When 命令を実行しようとする直前の `pc` がブレークポイントの命令添字と一致する, the Debugger State shall その命令を実行せずに `Breakpoint` を通知してバッチ実行を終了する
2. When `Breakpoint` で停止する, the Debugger State shall `pc` をブレークポイントの命令添字に保ち、`dp`・テープ・出力・入力読取位置・実行済み命令数を、停止を通知するために変更しない
3. When 新規作成直後またはリセット直後で `pc` が 0 の命令にブレークポイントがある, the Debugger State shall 最初のバッチ実行で命令を 1 つも実行せずに `Breakpoint` を通知する
4. When `Breakpoint` で停止した直後にバッチ実行を再度要求する, the Debugger State shall 停止位置の命令を実行して前進し、同じ位置で再び即座に停止し続けない
5. When ループ内の命令にブレークポイントがある, the Debugger State shall そのループの反復ごとに当該命令の直前で停止する
6. When 1 命令ステップ(Step Into)が実行を終えた位置の `pc` がブレークポイントである状態でバッチ実行を要求する, the Debugger State shall その位置の命令を実行して前進する
7. When あるバッチの最後の命令を実行し終えた直後の `pc` がブレークポイントである, the Debugger State shall 次のバッチ実行で当該位置を見逃さず、その命令の直前で `Breakpoint` を通知する
8. When プログラムが停止状態である, the Debugger State shall ブレークポイントの有無にかかわらず `Halted` を通知する
9. When ブレークポイントの追加・削除がバッチ実行の間に行われる, the Debugger State shall 次のバッチ実行から変更後の集合を使用する
10. The Debugger State shall コメント文字の位置では停止せず、停止位置を常に実行される命令の添字として報告する

### Requirement 6: キャンセル
**Objective:** As a GUI 実装者, I want 実行中のバッチを呼び出し側の指示で安全に中断できること, so that Pause 操作でフリーズや状態破損を起こさない

#### Acceptance Criteria
1. When バッチ実行を要求する, the Debugger State shall 呼び出し元が供給するキャンセル指示を受け取ることができ、キャンセル指示を供給しない呼び出しではキャンセルが発生しない
2. While バッチ実行中, the Debugger State shall 各命令を実行する直前にキャンセル指示を確認する
3. When キャンセル指示が観測可能になる, the Debugger State shall 指示を観測した後の命令を実行せずに実行を中断し、`Cancelled` を通知する
4. When キャンセル指示が既に有効な状態でバッチ実行を要求し、プログラムが停止していない, the Debugger State shall 命令を 1 つも実行せずに状態を変更せず `Cancelled` を通知する
5. When `Cancelled` で中断する, the Debugger State shall それまでに実行済みの命令の効果を保持し、`pc`・`dp`・テープ・出力・入力読取位置・実行済み命令数を一貫した(いずれかの命令の実行前または実行後の)状態に保つ
6. When `Cancelled` で中断した後にバッチ実行を再度要求する(キャンセル指示が有効でない), the Debugger State shall 中断位置から実行を再開し、キャンセルを挟まなかった場合と同一の最終結果を得る
7. If プログラムが既に停止状態である, then the Debugger State shall キャンセル指示の有無にかかわらず `Halted` を通知する
8. The Debugger State shall キャンセル指示の供給元が別の実行コンテキストからその指示を変更できる形(共有される真偽値のような指示)を受け付けられ、かつテストが別の実行コンテキストを待たずに決定的にキャンセルを再現できる形(一定回数の確認後に有効になる指示など)も受け付ける
9. If キャンセル指示とブレークポイントが同じ位置で同時に該当する, then the Debugger State shall `Cancelled` を通知し、ブレークポイントは次のバッチ実行で通知される

### Requirement 7: 1 命令ステップ(Step Into)
**Objective:** As a Brainfuck 学習者, I want 1 回の操作でちょうど 1 命令だけ実行できること, so that 状態遷移を 1 つずつ観察できる

#### Acceptance Criteria
1. When 停止していないセッションで Step Into を実行する, the Debugger State shall ブレークポイントの有無にかかわらず、ちょうど 1 個の命令を `brainfuck-core` の 1 命令実行と同一の意味論で実行し、命令を実行したことを示す結果を返す
2. When 停止状態のセッションで Step Into を実行する, the Debugger State shall 停止したことを示す結果を返し、状態を変更しない
3. If Step Into でポインタ範囲外またはステップ制限超過が発生する, then the Debugger State shall `brainfuck-core` と同じ実行エラーを返し、機械の状態を変更しない
4. When Step Into によってプログラムが停止状態に達する, the Debugger State shall 直近の停止理由を `Halted` とする
5. When Step Into が停止状態に達しない形で成功する, the Debugger State shall 直近の停止理由を消去する
6. When Step Into の前後でスナップショットを取得する, the Debugger State shall 実行済み命令数がちょうど 1 増え、`pc` / `dp` / 現在セルの変化から実行された命令の効果を観測できるようにする

### Requirement 8: テープ表示窓と状態ビュー
**Objective:** As a GUI 実装者, I want ポインタ周辺のテープだけを取得できること, so that 30,000 セル全体を高頻度に受け渡さずにテープを表示できる

#### Acceptance Criteria
1. The Debugger State shall テープ表示窓の既定の半径を 16 セルとして公開し、呼び出し元が 0 以上の任意の半径を指定できるようにする
2. When テープ表示窓を要求する, the Debugger State shall 現在の `dp` を中心に `dp - 半径` から `dp + 半径` までのセルを、有効なテープ範囲(0 以上 29,999 以下)に切り詰めて、昇順のセル値として返す
3. When テープ表示窓を返す, the Debugger State shall 窓の先頭セルのテープ上の添字と、窓内での `dp` の位置を含める
4. When `dp` がテープの端に近く窓が範囲外にはみ出す, the Debugger State shall 範囲外のセルを含めず、範囲内のセルだけを返す
5. When 半径が 0 である, the Debugger State shall 現在セルのみを含む窓を返す
6. When 半径がテープ全体を覆う大きさ以上である, the Debugger State shall テープ 30,000 セル全体を返し、範囲外のセルを返さない
7. The Debugger State shall テープ表示窓のセル値を、テープ全体の同じ添字の現在値と常に一致させる
8. When テープ表示窓を取得する, the Debugger State shall 機械の状態を変更せず、返す大きさを半径とテープ長のみによって決める
9. When 状態ビューを要求する, the Debugger State shall `pc`、`dp`、現在セルの値、実行済み命令数、停止しているかどうか、テープ表示窓、直近の停止理由を一度の呼び出しで一貫した時点の値として返し、状態を変更しない
10. When 状態ビューまたはセッションから出力を参照する, the Debugger State shall 出力バイト列を UTF-8 としての変換や置換をせずにバイト列のまま返す
11. When 機械を持たない呼び出し元がテープのバイト列、ポインタ位置、半径を与えてテープ表示窓を要求する, the Debugger State shall 機械を必要とせずに、要件 8.2〜8.8 と同一の規則(`ポインタ - 半径` から `ポインタ + 半径` までを与えられたテープの範囲に切り詰め、先頭セルの添字と窓内のポインタ位置を含める)で窓を返し、セッションまたは機械を経由した窓の取得はこの関数と同一の結果を返す。ポインタがテープの範囲外である場合は異常終了せず、セルを含まない窓を返す

### Requirement 9: 命令添字とソース位置の変換
**Objective:** As a GUI 実装者, I want 実行中の命令をソース上の範囲・行・列として示せること, so that エディタ上で現在の命令を強調表示できる

#### Acceptance Criteria
1. When 解析済みプログラムと、その解析に用いたソース文字列から位置変換情報を生成する, the Debugger State shall 以降の問い合わせをソース文字列を再解析せずに答えられるようにする
2. If 与えられたソース文字列が解析済みプログラムと整合しない(各命令のバイトオフセットがソース範囲外、または命令文字と一致しない), then the Debugger State shall 位置変換情報の生成を失敗させ、数値のみを含む説明を返し、異常終了しない
3. When 命令添字を与えてソース範囲を問い合わせる, the Debugger State shall その命令のソースバイトオフセットを範囲の始点とし、始点より 1 バイト後を範囲の終点(半開区間)として返す
4. When ソース範囲を返す, the Debugger State shall 始点と終点のそれぞれについて、バイトオフセット、1 始まりの行番号、その行内で 1 始まりの列番号(Unicode のスカラ値の個数で数える)、ドキュメント先頭からの UTF-16 コード単位でのオフセットを含める
5. When 行末として `\n`、`\r\n`、`\r` のいずれかが現れる, the Debugger State shall それぞれを 1 個の改行として扱い、`\r\n` を 1 個の改行として数える
6. When 命令の前にマルチバイト文字(例: 日本語、サロゲートペアを要する文字)が含まれる, the Debugger State shall バイトオフセット、列(スカラ値)、UTF-16 オフセットをそれぞれの単位で正しく区別して報告する
7. If 範囲外の命令添字でソース範囲を問い合わせる, then the Debugger State shall 異常終了せず、該当なしであることを呼び出し元が判別できる結果を返す
8. When 現在の命令(`pc`)のソース範囲を問い合わせる, the Debugger State shall `pc` が命令を指しているなら当該命令の範囲を返し、停止状態(`pc` が命令数と等しい)なら該当なしを返す
9. When ソースのバイトオフセットを与えて、その位置に命令文字があるか問い合わせる, the Debugger State shall 命令文字ならその命令添字を、コメント文字・範囲外・文字境界でない位置なら該当なしを返す
10. When ソースのバイトオフセットを与えて、その位置以降で最初の命令を問い合わせる, the Debugger State shall 該当する命令添字を返し、そのような命令が無ければ(範囲外のオフセットを含む)該当なしを返す
11. When 1 始まりの行番号を与えて、その行の先頭以降で最初の命令を問い合わせる, the Debugger State shall 該当する命令添字を返し、行が存在しない、またはそのような命令が無ければ該当なしを返す
12. When 命令が 0 個のプログラム(空ソースまたはコメントのみ)の位置変換情報を使う, the Debugger State shall すべての問い合わせで異常終了せず該当なしを返す
13. The Debugger State shall 位置変換のエラーの内容(フィールドおよび表示文字列)に、ソース全文や入力バイト列などの機微情報を含めない
14. When 解析済みプログラムを持たない呼び出し元(解析エラーの位置を示す場合など)がソース文字列とバイトオフセットを与えて位置を問い合わせる, the Debugger State shall 位置変換情報の生成を要さずに、そのオフセットの位置(バイトオフセット、1 始まりの行、1 始まりの列(Unicode スカラ値の個数)、UTF-16 オフセット)を、要件 9.4・9.5 と同一の規則で返す。オフセットはソース末尾(ソース長と等しい位置)を含み、文字境界でない位置およびソース長を超える位置は異常終了せず該当なしを返す。位置変換情報の生成も同じ規則を用いるため、任意の命令について、その命令のバイトオフセットで問い合わせた結果は、位置変換情報が返す範囲の始点と一致する
15. When エディタ向け(UTF-16 コード単位)のオフセットを与えて、その位置に命令文字があるか問い合わせる, the Debugger State shall 命令文字ならその命令添字を、コメント文字・範囲外・サロゲートペアの途中の位置なら該当なしを返す(バイトオフセット版 9.9 と同じ意味論)
16. When エディタ向け(UTF-16 コード単位)のオフセットを与えて、その位置以降で最初の命令を問い合わせる, the Debugger State shall 該当する命令添字を返し、そのような命令が無ければ(範囲外のオフセットを含む)該当なしを返す(バイトオフセット版 9.10 と同じ意味論)。命令 0 個のプログラムでは要件 9.12 のとおり異常終了せず該当なしを返す

### Requirement 10: ソース位置からのブレークポイント正規化
**Objective:** As a GUI 実装者, I want エディタ上の位置(オフセットまたは行)で指定されたブレークポイントを実行される命令へ対応付けられること, so that コメントや空行を指定されても停止位置が命令として一意に定まる

#### Acceptance Criteria
1. When ソースのバイトオフセットでブレークポイントの設定が要求される, the Debugger State shall そのオフセット以降で最初の命令の添字へ正規化し、その命令添字を呼び出し元へ返す
2. When 行番号でブレークポイントの設定が要求される, the Debugger State shall その行の先頭以降で最初の命令の添字へ正規化し、その命令添字を呼び出し元へ返す
3. If 正規化の結果に対応する命令が存在しない, then the Debugger State shall ブレークポイントを設定せず、設定できなかったことを呼び出し元が判別できる結果を返す
4. When 正規化された命令添字を呼び出し元へ返す, the Debugger State shall その命令のソース範囲を問い合わせられるようにし、呼び出し元が実際の停止位置(行・列)を表示できるようにする
5. The Debugger State shall 正規化されたブレークポイントを、ソース位置ではなく命令添字として保持し、以降のブレークポイント判定を命令添字の一致だけで行う
6. When エディタ向け(UTF-16 コード単位)のオフセットでブレークポイントの設定が要求される, the Debugger State shall そのオフセット以降で最初の命令の添字へ正規化し、その命令添字を呼び出し元へ返す(命令が無ければ要件 10.3 と同じく設定しない)。この正規化は要件 9.16 の逆引きのみで行え、呼び出し側が位置変換の表を独自に構築することを要しない

### Requirement 11: 検証可能性(テスト)
**Objective:** As a プロジェクト管理者, I want デバッグ契約が自動テストで検証されること, so that GUI が信頼できるデバッグ状態に依存できる

#### Acceptance Criteria
1. The Debugger State shall 停止理由・バッチ実行について、`Halted`、`StepLimitReached`、`Breakpoint`、`Cancelled`、実行エラーの各通知と、停止直前・バッチ境界ちょうどの境界値を検証する自動テストを含む
2. The Debugger State shall ブレークポイントについて、追加・削除・トグル・昇順列挙・範囲外エラー、ループ内での反復ごとの停止、再開時の前進、Step Into 直後の再開、バッチ境界での見逃しがないことを検証する自動テストを含む
3. The Debugger State shall キャンセルについて、開始前の指示、実行途中の指示(一定回数の確認後に有効になる指示による決定的な再現)、中断後の再開で最終結果が一致すること、停止済みでの優先、ブレークポイントとの同時該当を検証する自動テストを含む
4. The Debugger State shall 複数のバッチ命令数(1、素数、既定値、全体を覆う値)について、バッチ実行の反復結果が `brainfuck-core` の `run` と一致することを検証する差分テストを含む
5. The Debugger State shall テープ表示窓について、先頭付近・末尾付近・半径 0・テープ全体を覆う半径・既定半径での範囲と先頭添字、窓内の `dp` の位置を検証する自動テストを含む
6. The Debugger State shall 位置変換について、ASCII のみ、マルチバイト文字、サロゲートペア、`\n` / `\r\n` / `\r`、コメントのみ、空ソース、範囲外の問い合わせ、ソース不整合のエラー、ソース位置からの正規化を検証する自動テストを含む
7. The Debugger State shall 既存の `brainfuck-core` のテストがデバッグ機能の追加後も変更なしで成功することを検証対象に含める
8. When 不具合を発見し修正する, the Debugger State shall 最小の再現プログラムと対応する回帰テストを追加できる構成でテストを管理する
9. When ワークスペース全体のテストを実行する, the Debugger State shall すべての自動テストが成功する
10. The Debugger State shall ソース文字列のみからの位置変換(ASCII、マルチバイト、サロゲートペア、改行 3 種、ソース末尾、`\r\n` の途中、文字境界でない位置、範囲外、位置変換情報との一致)、UTF-16 オフセットからの命令の逆引き(厳密・以降、コメント上、サロゲートペアの途中、範囲外、命令 0 個)、およびテープのバイト列からの表示窓(端・半径 0・全体・範囲外のポインタ、機械経由の窓との一致)を検証する自動テストを含む
