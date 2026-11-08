# Requirements Document

## Project Description (Input)
# Brief: desktop-gui-e2e-webview(要約。全文は同じディレクトリの `brief.md`)

## Problem
`desktop-gui-e2e-browser` は Chromium と IPC の差し替えで画面を検証するが、実際の配布形(Tauri 2 + WebView2 + Rust の IPC)で起きること(実ウィンドウの閉じる要求、WebView2 の既定の右クリックメニューとリロード、CSP の実効、実 IPC の往復、実 WebView2 での描画)は確認できない。これらは 2026-11-07 に `orca computer` で一度だけ手動確認したが、繰り返せない。少数の重要な流れを、実アプリに対して自動で通したい。

## Desired Outcome
Windows で webview 用の E2E コマンドを実行すると、配布と同じ画面を組み込んだ debug ビルドの実アプリが起動し、Playwright が CDP(`connectOverCDP`)で接続して、起動・Hello World・Pause と再開・閉じる確認・WebView2 の既定動作の抑止・CSP の実効・エディタのフォーカス枠を自動で検証する。`desktop-gui-e2e-browser` の Playwright 設定・ページオブジェクト・補助を再利用し、重複を持たない。CI では Windows の job として実行するが初期は非必須とし、ローカルの実行手順(Avast の扱い、WebView2 の版、ポート)を README に書く。自動化できない項目(ネイティブダイアログ、ウィンドウのサイズ変更、OS の配色、NVDA)は手動のまま README と traceability に明記する。

## Constraints
Windows 11 / WebView2 Evergreen、`workers: 1`、ポートと利用者データのフォルダはシナリオごとに独立、配布ビルドにテスト専用の仕組みを含めない、Rust の debug ビルドをキャッシュする、Avast の影響を README に明記、Node 26、Markdown 成果物は日本語。

## Introduction

`desktop-gui-e2e-webview` は、desktop-gui の実アプリ(Tauri 2 + WebView2、Windows)を起動し、実際のウィンドウ・実際の IPC・実際の WebView2 の上で、少数の重要な流れを自動で検証する E2E スイートを提供する。対象の利用者は、desktop-gui とその後続(compiler-comparison など)を変更する開発者と、Windows の CI を運用する開発者である。

本 feature は製品の機能を追加・変更しない。2026-11-07 に一度だけ手作業で行った実機の確認のうち、`desktop-gui-e2e-browser`(Chromium と IPC の差し替え)では確かめられない「実配布形でしか起きないこと」だけを対象にし、ブラウザのスイートが担う項目は重複して実装しない。

以下、手動スモークの番号(例: 「スモーク 3.1」)は `apps/desktop/README.md` の「手動スモーク手順」の番号を指す。「webview スイート」は本 feature の E2E スイート、「ブラウザのスイート」は `desktop-gui-e2e-browser` の E2E スイートを指す。

## Boundary Context

- **In scope**:
  - Windows でのみ動く webview スイートの実行コマンド、準備、実アプリの起動と終了、失敗時の調査材料の保存
  - スモーク 1(起動と初期状態)、3.1(Hello World)、4(上限なしの Pause と再開)、9.3(実行数の増加)、11.1〜11.3・11.5(閉じる確認)、11.6(リロードの抑止)、11.7(右クリックの既定メニューの抑止)、13.3(エディタのフォーカス枠)の実アプリでの検証
  - CSP の実効(インラインスクリプト・外部のスクリプト・外部への取得が拒否されること)の実アプリでの検証
  - Windows の CI の job(初期は非必須)と、ローカルの実行手順・検証結果の記録・traceability
- **Out of scope**:
  - ネイティブダイアログ(開く・保存・名前を付けて保存)の操作と見た目。ブラウザのスイートの代替と手動の確認に委ねる
  - ウィンドウのサイズ変更と最小サイズの強制(スモーク 12)、OS のライトテーマ・高コントラストモードの切り替え(スモーク 14.1・14.2)、スクリーンリーダー(スモーク 14.4)
  - スモーク 11.4(保存済み・無変更での確認なしの終了)。ブラウザのスイートが模擬で担い、実ウィンドウでの確認は手動のまま
  - タイトルバーの閉じるボタンを画面上で物理的にクリックすること。閉じる要求は、閉じるボタンと同じ OS の閉じる要求の経路で代替する
  - 並列実行、Windows 以外の OS
  - ブラウザのスイートが担う項目の重複実装、desktop-gui の機能変更・不具合修正
- **Adjacent expectations**:
  - ブラウザのスイートは、Playwright の設定・ページオブジェクト・既定動作の観測とフォーカス枠の検証の補助を提供し、本 feature はそれらを再利用する。ブラウザのスイートの基盤の変更が必要な場合は、本 feature が黙って変更するのではなく、ブラウザの spec への変更として扱う
  - desktop-gui は、画面のアクセシブルな名前・役割、閉じる要求の振る舞い(未保存のときだけ確認を出し、確定で終了する)、CSP・ウィンドウの設定を維持する。これらが変わった場合、webview スイートは失敗として変更を知らせる
  - `examples/hello.bf` の実行結果(出力 `Hello World!`、`pc` 106、`dp` 6、実行数 906)は既存の検証で確定した期待値である
  - E2E が製品の不具合を見つけた場合、その修正は desktop-gui 側の変更として扱い、本 feature は失敗の記録と追跡のみを行う

## Requirements

### Requirement 1: 実行コマンドと実行環境
**Objective:** As a 開発者, I want Windows で 1 つのコマンドから実アプリの E2E を準備・実行でき、既存のテストと分かれていること, so that 実配布形でしか起きない回帰を手軽に繰り返し確認できる

#### Acceptance Criteria
1. When 開発者が `apps/desktop` で webview スイートの準備のコマンドを実行する, the webview スイート shall 配布と同じ手順でビルドした画面を組み込んだ、デバッグ用の実アプリの実行ファイルを作る
2. When 開発者が `apps/desktop` で webview スイートの実行のコマンドを実行する, the webview スイート shall 実アプリを起動して全シナリオを実行し、結果を合否と件数で報告する
3. The webview スイート shall 既存の単体テストのコマンド(`npm test`)とブラウザのスイートのコマンドの実行対象に含まれず、それらのコマンドの振る舞いと結果を変えない
4. If Windows 以外の OS で webview スイートが実行される, the webview スイート shall Windows 限定であることを示すメッセージを出して失敗し、合格として報告しない
5. If 実行に必要な準備物(実アプリの実行ファイル、WebView2 のランタイム、OS の操作に使うシェル)が見つからない, the webview スイート shall 不足しているものと準備の方法を示すメッセージを出して失敗する
6. The webview スイート shall シナリオを 1 つずつ順に実行し、シナリオごとに実アプリを新しく起動し、デバッグ用の接続口(ポート)と WebView2 の利用者データのフォルダをシナリオごとに分け、シナリオどうしで状態を共有しない
7. If 実アプリの起動が上限時間内に接続可能な状態にならない, the webview スイート shall 実アプリを終了させ、ウイルス対策ソフトの除外・一時停止と新しいビルドディレクトリでの再ビルドを案内するメッセージで失敗する
8. When シナリオが終わる(成功・失敗・中断のいずれでも), the webview スイート shall そのシナリオで起動した実アプリと子プロセスを終了させ、一時的な利用者データのフォルダを削除する
9. If シナリオが失敗する, the webview スイート shall 失敗した時点の実アプリの画面のスクリーンショットと、実アプリの標準出力・標準エラーの末尾を、調査用の出力ディレクトリに保存する
10. The webview スイート shall 実行に使った WebView2 のランタイムの版を結果に記録する
11. The webview スイート shall 型検査・lint・単体テストの対象に webview スイートのコードを含め、既存の型検査・lint・単体テスト・Rust のテストを成功させ続ける

### Requirement 2: 配布形との一致と製品への影響
**Objective:** As a 開発者, I want 検証の対象が配布と同じ画面・設定の実アプリであり、テストの仕組みが製品に混入しないこと, so that 配布物での振る舞いを正しく確かめられ、配布物の安全性を損なわない

#### Acceptance Criteria
1. The webview スイート shall 開発用のサーバではなく、配布と同じ手順でビルドして実行ファイルに組み込んだ画面と、配布と同じウィンドウ・セキュリティの設定(CSP を含む)で動く実アプリを検証の対象にする
2. The webview スイート shall テスト専用のコマンド・権限・ケイパビリティを、配布用のビルドとその既定の設定に追加しない
3. The webview スイート shall デバッグ用の接続口を、webview スイートが起動した実アプリのプロセスでだけ有効にし、利用者が通常の手順で起動した実アプリでは有効にしない
4. The webview スイート shall E2E のために変更した製品のソースと設定ファイルを spec の設計に一覧として明記し、一覧に無い製品のソースを変更しない
5. The webview スイート shall 開発用のサーバが使うポート(1420)を使わず、開発用のサーバやブラウザのスイートの配信サーバが起動していても実行できる

### Requirement 3: ブラウザのスイートとの共有と非重複
**Objective:** As a 開発者, I want webview スイートがブラウザのスイートの設定と画面操作の部品を共有し、同じ検証を二重に持たないこと, so that 保守の手間が増えず、画面の変更への追随が 1 か所で済む

#### Acceptance Criteria
1. The webview スイート shall ブラウザのスイートと同じ E2E のランナーの設定ファイルを使い、その中の独立した実行単位として追加され、別の設定ファイルを作らない
2. The webview スイート shall 画面の要素の特定と操作、既定動作の観測、フォーカス枠の描画の検証に、ブラウザのスイートが提供する部品を使い、同じ役割の部品を別に作らない
3. The webview スイート shall Boundary Context の In scope に挙げたスモーク項目と CSP の実効だけをシナリオにし、ブラウザのスイートが担う項目をシナリオにしない
4. If ブラウザのスイートの基盤に変更が必要になる, the webview スイート shall その変更を本 spec の設計に「ブラウザの spec への必要な変更」として明記し、ブラウザのスイートの部品を黙って変更しない

### Requirement 4: 起動・実行制御のシナリオ
**Objective:** As a 開発者, I want 実アプリの起動直後の状態と、実際の IPC を通した実行と一時停止を毎回確認できること, so that 実配布形での起動と実行制御の回帰を検出できる

#### Acceptance Criteria
1. When 実アプリが起動する, the webview スイート shall OS のウィンドウのタイトルが `Brainfuck Studio` であること、編集領域に `examples/hello.bf` の内容が表示され文書の表示が「無題」であること、状態ラベルが「未開始」であること、Run・Step が有効で Pause・Reset が無効であることを検証する(スモーク 1)
2. When 初期の Hello World を Run する, the webview スイート shall 出力に `Hello World!` が表示され、状態ラベルが「正常終了」、`pc` 106、`dp` 6、実行数 906 になること、および読み込み・開始・実行の要求が実アプリの IPC を通って処理されたことを検証する(スモーク 3.1)
3. When 命令数上限を「上限なし」にして `+[]` を Run する, the webview スイート shall 上限なしの警告が表示されること、Pause で状態ラベルが「一時停止」になり実行数の増加が止まること、Run で再開して実行数が再び増えること、それぞれの時点で Run・Pause・Step・Reset の有効・無効が正しいことを検証する(スモーク 4)
4. While 上限なしの `+[]` を実行している, the webview スイート shall 実行数の表示が連続する複数回の読み取りで増え続けることを、時間の閾値に依存しない方法で検証する(スモーク 9.3)

### Requirement 5: 実ウィンドウの閉じる確認のシナリオ
**Objective:** As a 開発者, I want 実ウィンドウへの閉じる要求で未保存の文書が守られることを毎回確認できること, so that 文書の喪失につながる回帰を実配布形で防げる

#### Acceptance Criteria
1. While 未保存の変更がある, when webview スイートが実ウィンドウに OS の閉じる要求(タイトルバーの閉じるボタンと同じ経路)を送る, the webview スイート shall 実アプリのプロセスが終了せず、確認ダイアログが 1 回だけ表示されることを検証する(スモーク 11.1)
2. When 確認ダイアログで「キャンセル」を選ぶ, the webview スイート shall 実アプリのプロセスが動き続け、編集内容が残り、続けて画面を操作できることを検証する(スモーク 11.2)
3. When 確認ダイアログで「破棄して閉じる」を選ぶ, the webview スイート shall 実アプリのプロセスが正常終了の終了コードで終わることを検証する(スモーク 11.3)
4. While 確認ダイアログが表示されている, when 閉じる要求を重ねて送る, the webview スイート shall 重ねた要求が画面に届いたことを確かめた上で、確認ダイアログが二重に表示されないことを検証する(スモーク 11.5)
5. If 閉じる要求を実ウィンドウへ送れない(ウィンドウが見つからない等), the webview スイート shall そのシナリオを理由つきで失敗させ、合格として報告しない
6. The webview スイート shall 閉じる要求を確認する前に、未保存の変更が実アプリの側へ伝わったことを確かめ、伝達の遅れによって確認なしで閉じる誤りを合否の判定に混ぜない

### Requirement 6: WebView2 の既定動作の抑止のシナリオ
**Objective:** As a 開発者, I want 実 WebView2 で、リロードと既定の右クリックメニューが実際に抑止されていることを確認できること, so that ブラウザの模擬では分からない実環境の既定動作による文書の喪失を防げる

#### Acceptance Criteria
1. When OS のキーボード入力として F5 の連打、Ctrl+R、Ctrl+F5 を実アプリへ送る, the webview スイート shall ページが再読み込みされず(ページ内の目印が残り、ページの遷移が起きない)、編集内容が残り、それぞれのキー入力の既定動作が抑止されたことを検証する(スモーク 11.6)
2. When アプリの抑止を無効にした状態で同じリロードのキー入力を送る(陽性対照), the webview スイート shall ページが再読み込みされることを確かめ、入力の経路が WebView2 の既定動作に届くことを示す。再読み込みされない場合は、11.6 の検証が成り立たないものとしてシナリオを失敗させる
3. When 編集可能でない要素の上で右クリックする, the webview スイート shall 右クリックの既定動作が抑止され、WebView2 の既定のコンテキストメニューが表示されないことを検証する(スモーク 11.7)
4. When アプリの抑止を無効にした状態で同じ右クリックを行う(陽性対照), the webview スイート shall WebView2 の既定のコンテキストメニューが表示されることを検出できることを確かめる。検出できない場合は、メニューが表示されないことの検証が成り立たないものとしてシナリオを失敗させる
5. If OS の入力を実アプリへ届けられない(ウィンドウを前面にできない等), the webview スイート shall そのシナリオを理由つきで失敗させ、合格として報告しない

### Requirement 7: CSP の実効のシナリオ
**Objective:** As a 開発者, I want 実アプリで CSP が実際に効いていることを確認できること, so that 設定ファイルの値だけでなく、実行時にスクリプトの注入や外部への通信が拒否されることを保証できる

#### Acceptance Criteria
1. When 実アプリのページにインラインのスクリプトを挿入する, the webview スイート shall そのスクリプトが実行されず、CSP の違反が報告されることを検証する
2. When 実アプリのページに外部のオリジンのスクリプトを読み込む要素を挿入する, the webview スイート shall そのスクリプトが実行されず、外部のオリジンに要求が届かず、CSP の違反が報告されることを検証する
3. When 実アプリのページから外部のオリジンへ取得の要求を出す, the webview スイート shall 要求が拒否され、外部のオリジンに要求が届かず、CSP の違反が報告されることを検証する
4. When 実アプリのページから同じオリジンの資源を取得する(陽性対照), the webview スイート shall 取得が成功することを確かめ、拒否が CSP によるものであることを示す

### Requirement 8: 実 WebView2 でのフォーカス枠のシナリオ
**Objective:** As a 開発者, I want エディタのフォーカス枠が実 WebView2 の描画で見えることを確認できること, so that 2026-11-07 に実機で見つかった「エディタのフォーカス枠が見えない」不具合の再発を実環境で検出できる

#### Acceptance Criteria
1. When キーボードの操作でエディタにフォーカスが移る, the webview スイート shall エディタのフォーカス枠が実 WebView2 の描画結果で実際に描画され、他の要素に隠れていないことを検証する(スモーク 13.3)
2. The webview スイート shall 検証したときの配色(ライト・ダーク)を結果に記録し、配色の切り替えの確認を本シナリオで行ったと扱わない

### Requirement 9: Windows の CI での実行
**Objective:** As a CI の運用者, I want webview スイートが Windows の CI で自動的に実行され、安定するまでは取り込みを妨げないこと, so that 実配布形の回帰を早く知りつつ、不安定な失敗で開発を止めない

#### Acceptance Criteria
1. When main ブランチへの push、プルリクエスト、または手動の実行の指示がある, the webview スイート shall Windows の CI ランナーで webview スイートを実行する
2. The webview スイート shall CI で、テストの前に WebView2 のランタイム(Evergreen)を導入または更新し、使った版を結果に記録する
3. The webview スイート shall 初期の運用では、webview スイートの失敗を結果に表示するが、CI 全体の失敗やプルリクエストの取り込みの妨げにしない
4. If CI で webview スイートが失敗する, the webview スイート shall 失敗の調査用の出力(レポート、スクリーンショット、実アプリのログ)を CI の成果物として保存する
5. The webview スイート shall CI の設定として自分の job だけを追加し、基本 CI(Rust とフロントエンドの build / lint / test)とブラウザのスイートの CI の設定を変更しない
6. The webview スイート shall CI で実アプリの Rust のビルド結果をキャッシュし、変更が無いときの再ビルドを避ける

### Requirement 10: 追跡可能性と記録
**Objective:** As a 開発者, I want 実機で確認すべき項目が、自動化済み・手動のまま・対象外のどれかを一覧で確認できること, so that 実機での確認の残りを見落とさない

#### Acceptance Criteria
1. The webview スイート shall Boundary Context の In scope の各項目を検証するシナリオとテスト名に対応づけ、Out of scope の各項目を「手動のまま(理由)」または「対象外(担当する spec またはテスト)」に分類した対応表を、本 spec の traceability に記載する
2. The webview スイート shall 閉じる要求を、閉じるボタンの物理的なクリックではなく OS の閉じる要求で代替したことを、traceability と README に明記する
3. The webview スイート shall `apps/desktop/README.md` に、webview スイートの準備と実行の手順、Windows 限定であること、WebView2 のランタイムの版の確認方法、デバッグ用の接続口の扱い、実行中にキーボードとマウスを操作しないこと、開発 PC でウイルス対策ソフトが新しい実行ファイルを止めて固まる場合の対処(除外の設定、一時停止、新しいビルドディレクトリ、再起動)を記載する
4. When webview スイートを実行して結果を確認する, the webview スイート shall その実施日・WebView2 の版・結果(件数、失敗、`fixme` とその理由)を `apps/desktop/README.md` の「検証結果の記録」に追記し、実施していない項目を合格と書かない
5. The webview スイート shall desktop-gui とブラウザのスイートの既存の traceability を変更しない
