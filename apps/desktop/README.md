# Brainfuck Studio デスクトップ版(apps/desktop)

Brainfuck のソースを編集し、実行・一時停止・1 命令ずつのステップ実行・ブレークポイント・リセットを行えるデスクトップ GUI です。
フロントエンドは React + TypeScript(Vite)、ホストは Tauri 2 です。実行そのものは Rust 側の `brainfuck-gui-api`(`DebugSession` を包むサービス)が行い、WebView は表示と操作の要求だけを担当します。

## 前提

- **Node.js 26**: リポジトリ直下の `mise.toml`(`node = "26"`)に従います。PATH の既定の Node が 26 でない場合があるため、`npm` と `node` は必ず `mise exec node@26 -- ...` の形で実行してください。
- **Rust**(stable、`cargo`): `rustup` で導入します。Windows では Microsoft C++ Build Tools(MSVC)も必要です。
- **WebView2 ランタイム**(Windows): Windows 11 では標準で含まれます。無い場合は Microsoft の WebView2 ランタイムを導入してください。
- **OS**: Windows 11 で確認しています。

以下のコマンドはすべて、特記がない限り次のいずれかのディレクトリで実行します。

- フロントエンドのコマンド: `apps/desktop`
- Rust のコマンド: リポジトリのルート(`cargo test --workspace` など)、または `apps/desktop/src-tauri`(`cargo test --manifest-path ...`)

## 依存の導入

```sh
cd apps/desktop
mise exec node@26 -- npm ci
```

`package-lock.json` に従って依存を入れます。`npm ci` は lock ファイルと一致しない場合に失敗するため、依存を変えたい場合は `npm install` を使い、更新された lock ファイルをコミットしてください。

## 開発起動

```sh
cd apps/desktop
mise exec node@26 -- npm run tauri dev
```

Vite の開発サーバ(`http://localhost:1420`)と Tauri のウィンドウを起動します。初回は Rust 側の依存をビルドするため時間がかかります。フロントエンドだけを確認したい場合は `mise exec node@26 -- npm run dev` で Vite のみを起動できます(この場合、Tauri のコマンドは呼べないため、実行ボタンは使えません)。

## ビルド

```sh
cd apps/desktop
mise exec node@26 -- npm run build
```

`npm run build` は型検査(`typecheck`)のあと `vite build` を行い、成果物を `apps/desktop/dist/` に出力します。`src-tauri/tauri.conf.json` の `frontendDist`(`../dist`)はこの成果物を指すため、Rust 側をビルドする前に一度実行しておくと確実です。

配布用のインストーラーは次のコマンドで作ります。

```sh
cd apps/desktop
mise exec node@26 -- npm run tauri build
```

`tauri build` は `beforeBuildCommand` で `npm run build` を実行してから、Rust 側をリリースビルドします。

## テスト

### フロントエンド(`apps/desktop`)

```sh
mise exec node@26 -- npm run typecheck   # tsc --noEmit
mise exec node@26 -- npm run lint        # eslint
mise exec node@26 -- npm test            # vitest run(ウィンドウは起動しない)
```

### Rust(リポジトリのルート)

```sh
cargo test --workspace                                  # ルートのワークスペース(フロントのビルド成果物は不要)
cargo test -p brainfuck-gui-api --test bindings_up_to_date  # 生成した型と生成物の一致
```

### Rust(アプリ側 `apps/desktop/src-tauri`)

`src-tauri` は独立した `[workspace]` のため、ルートの `cargo test --workspace` には含まれません。別に実行してください。

```sh
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
```

アプリ側のテストには、コマンド名の登録・ビルド設定・ケイパビリティの一致検査、ケイパビリティの内容検査(`fs`・`shell`・`http`・`dialog`・`core:window:*` の許可が無いこと)、I/O エラーの変換、終了ガードの判定が含まれます。

## E2E テスト(ブラウザ)

手動スモーク手順の 1〜15 のうち、ブラウザで観測できる項目を Playwright で自動化したテストです。画面は `npm run build` の成果物を `vite preview` で配信し、Chromium で描画・操作します。バックエンドはテストごとに起動する Rust のブリッジ(`crates/brainfuck-e2e-bridge`)が担い、アプリと同じ `brainfuck-gui-api` の実装を通します。シナリオは `apps/desktop/e2e/specs/` にあります。

以下のコマンドはすべて `apps/desktop` で実行します。

### 準備

前提(Node 26、Rust)に加えて、Chromium を導入します。

```sh
cd apps/desktop
mise exec node@26 -- npm ci
mise exec node@26 -- npx playwright install chromium
mise exec node@26 -- npm run e2e:prepare
```

`npm run e2e:prepare` は次の 3 つを作ります。

- 画面のビルド成果物: `dist/`(`npm run build`)
- ブリッジの実行ファイル: `target/debug/brainfuck-e2e-bridge.exe`(Windows の場合。`npm run e2e:bridge` が `cargo build -p brainfuck-e2e-bridge` を実行する)
- IPC シムのビルド成果物: `e2e/.cache/ipc-shim.js`(`npm run e2e:shim`)

準備物が足りないと、テストの起動前に不足の一覧と準備のコマンドを示して止まります。

### 実行

```sh
cd apps/desktop
mise exec node@26 -- npm run e2e                  # 準備を含めて実行(chromium プロジェクト。@perf を除く)
mise exec node@26 -- npm run e2e -- --headed      # ブラウザを表示して実行
mise exec node@26 -- npm run e2e:perf             # 計測(@perf を付けたシナリオだけ。準備を含む)
mise exec node@26 -- npm run e2e:run              # 準備を省いて実行(Playwright の引数をそのまま渡せる)
```

`npm run e2e:run` は準備を行わないため、準備済みの成果物を使って繰り返し実行するときや、特定のシナリオだけを試すときに使います(例: `npm run e2e:run -- e2e/specs/smoke-03-hello-world.spec.ts`)。`--project` を付けないと、`@perf` のシナリオも実行されます(計測の専用プロジェクトも含めて全件)。通常のシナリオだけを実行するには `npm run e2e:run -- --project=chromium` を使ってください。

#### 環境変数

| 変数 | 用途 |
| --- | --- |
| `E2E_PORT` | 配信ポート。既定は `14273`(開発サーバの `1420` と分けている)。例: `E2E_PORT=14300 npm run e2e`(PowerShell では `$env:E2E_PORT=14300; npm run e2e`) |
| `E2E_SKIP_PREREQ=1` | 準備物の事前確認を省く。`--list`・`show-report` のように準備物が要らない用途向けです。準備物が無い状態で実行すると、テストは失敗します |
| `E2E_BRIDGE_READY_MS` | ブリッジの起動確認(`e2e:ping` への応答)を待つ上限のミリ秒。既定は 30000。正の整数以外を指定すると、テストは失敗します。ウイルス対策ソフトの検査などで起動が遅いときに延ばします |
| `E2E_BRIDGE_BIN` | ブリッジの実行ファイルを明示する(相対パスは `apps/desktop` 基準)。事前確認と実行で使う実行ファイルが決まります。`npm run e2e` の準備は既定の場所にビルドするため、別の場所のブリッジを使うときは、そのブリッジを指定して `npm run e2e:run` を使ってください |

ブリッジの実行ファイルの場所は、次の順で決まります。`E2E_BRIDGE_BIN` があればその場所、無ければ `CARGO_TARGET_DIR` の下の `debug/`、どちらも無ければリポジトリの `target/debug/` です。

### レポートと失敗時の出力

- **HTML レポート**: `apps/desktop/playwright-report/`。実行の後に生成されます(ローカルでは自動では開きません)。開くには `E2E_SKIP_PREREQ=1 mise exec node@26 -- npx playwright show-report` を実行します。
- **失敗時の出力**: `apps/desktop/test-results/`。失敗したテストのスクリーンショット(`only-on-failure`)、トレース(`retain-on-failure`)、エラーの文脈が入ります。トレースは `npx playwright show-trace <trace.zip のパス>` で開けます。
- **再試行**: CI でもローカルでも、失敗したテストを 1 回だけ再試行します(`playwright.config.ts` の `retries: 1`)。再試行で通ったものは flaky として報告に残ります。再試行の理由は「並列度と製品の不具合の記録」の節を参照してください。

### 並列度と製品の不具合の記録

- 複数のブラウザやブリッジが同時に動くマシンでは、`page.goto` が `ERR_ABORTED` や 30 秒のタイムアウトになることがあります。単独で実行すると通るため、そのときは `mise exec node@26 -- npm run e2e:run -- --project=chromium --workers=1` で 1 つずつ実行してください(準備は済んでいること)。
- 起動直後の `page.goto` の遅れ(`npm run build` の直後に 4 ワーカーが同時に読み込むと、最初のテストが 30 秒のタイムアウトになることがある)には、`playwright.config.ts` の `retries: 1` で対処しています。ウォームアップの取得や `globalSetup` は足していません(再試行で通ったものは flaky として報告されるため、遅れが常態化していないかは報告で確認できます)。アサーションの失敗は再試行で隠さず、続けて失敗すれば失敗として報告されます。
- 製品の不具合を見つけたシナリオは、当初は製品を直さず `test.fixme` にして注釈(`type: "issue"`)に現象と追跡先を書いていました。次の 2 件は desktop-gui の PR #12 で修正済みで、`test.fixme` を外して通常のテストにしました。
  1. スモーク 2.1(要件 4.2、`smoke-02-editing.spec.ts`): 取り消しで保存時点と同じ内容に戻ったら、未保存の表示も消える(上流 desktop-gui 2.2。修正前は `apps/desktop/src/state/workspace.ts` の `setSource` が dirty を無条件に立てていた)。
  2. スモーク 5.3(`smoke-05-step.spec.ts`): 最後の命令を Step した後、Step と Run が無効になる(上流 4.7・4.8。修正前は `apps/desktop/src/state/debuggerStore.ts` の `applyStep` が停止を反映していなかった)。
- 追跡先は `.kiro/specs/desktop-gui-e2e-browser/traceability.md` の「見つけた製品の不具合」です。

### 型検査の分担

- `npm run typecheck`: 製品(`tsconfig.json`)と E2E(`tsconfig.e2e.json`)の両方を検査します。
- `npm run build`: 製品だけを検査してから `vite build` を行います。E2E の型の誤りは `npm run build` では検出されません。

### ブリッジと Tauri 層の乖離を検出するテスト

ブリッジは `src-tauri` の振る舞いを複製しています。次の変更があると、対応するテストが失敗します。

- **コマンドの追加・削除**(`src-tauri/src/commands/names.rs` の `COMMAND_NAMES` が基準): `apps/desktop/e2e/support/bridge/commandContract.test.ts` が、登録・ブリッジ・除外の不一致を、差のあるコマンド名とともに示します。対処は、ブリッジで対応する(`crates/brainfuck-e2e-bridge/src/dispatch.rs` の `COMMANDS` と分岐に足す)か、理由つきで `EXCLUDED_COMMANDS` に入れることです。
- **複製している区間の変更**(`src-tauri/src/app_state.rs`、`commands/files.rs`、`window.rs` の該当部分): `apps/desktop/e2e/support/bridge/tauriFingerprint.test.ts` が、区間の名前・固定値・現在値を示します。対処は次の順です。
  1. 複製先(`crates/brainfuck-e2e-bridge/src/document.rs`、`dispatch.rs`)を、変更後の Tauri 層の振る舞いに合わせる(ブリッジで対応する、または差を理由つきで残す)。
  2. `cargo test -p brainfuck-e2e-bridge` が通ることを確認する。
  3. `tauriFingerprint.test.ts` の固定値を、失敗の文面に出た現在値に置き換える。

### E2E が検証しないこと

E2E は実ウィンドウと WebView2 の挙動を検証しません。次は別の手段で扱います。

- `desktop-gui-e2e-webview` が扱うもの: 実ウィンドウの × による閉じる確認(スモーク 11.1〜11.3、11.5)、実際のリロード(11.6)、WebView2 の右クリックメニュー(11.7)、CSP の実効、13.3 の実 WebView2 での描画、1・3.1・4・9.3 の実 IPC での確認。E2E では、リロードのキー入力(Ctrl+R など)で既定動作が抑止されることを、`defaultPrevented` と `framenavigated` が 0 回であることで確認します。実際にリロードされないことは、キー入力では確かめられません。
- 手動のまま(webview も対象外)のもの: ネイティブダイアログの見た目とモーダル性(10.1)、ウィンドウの最小サイズの強制(12)、スクリーンリーダー(NVDA)での読み上げ(14.4)。
- Rust の別のテストが担うもの: スモーク 3.2(CLI との結果一致。`crates/brainfuck-gui-api/tests/cli_parity_real.rs`)。

### 開発 PC でウイルス対策ソフトが止める場合

新しく作られた実行ファイル(ブリッジやテストの実行ファイル)を、ウイルス対策ソフトが止めて固まることがあります。プロセスが CPU を使わずに進まない場合は、リポジトリのフォルダー、`CARGO_TARGET_DIR`、`apps/desktop/e2e/.cache` を除外に追加するか、リアルタイム保護を一時停止してからやり直してください。同じビルドディレクトリで繰り返すとリンクに失敗することがあるため、新しい `CARGO_TARGET_DIR`(例: `C:/Users/<ユーザー名>/AppData/Local/Temp/kgtarget_e2e_<任意の名前>`)を指定すると回避できます。

## E2E テスト(実アプリ・WebView2)

ブラウザのスイートとは別に、実アプリ(`brainfuck-studio.exe` の debug ビルド)を起動し、WebView2 の上で検証する webview スイートがあります。**Windows 専用**です(Windows PowerShell 5.1 の `powershell.exe` で動かす補助スクリプト、`taskkill /T /F`、WebView2 を使います)。シナリオは `apps/desktop/e2e/webview/` にあり、支援のコードは `apps/desktop/e2e/support/webview/` にあります。

以下のコマンドはすべて `apps/desktop` で実行します。

### 準備と実行

```sh
cd apps/desktop
mise exec node@26 -- npm ci
mise exec node@26 -- npm run e2e:webview:prepare                 # 実アプリの debug ビルド(tauri build --debug --no-bundle)
mise exec node@26 -- npm run e2e:webview:run                     # 準備の確認を行ってから webview のシナリオを実行
mise exec node@26 -- npm run e2e:webview:run -- --list           # シナリオの一覧だけを表示
mise exec node@26 -- npm run e2e:webview:run -- e2e/webview/smoke-01-startup.webview.spec.ts  # 1 つのシナリオだけ
mise exec node@26 -- npm run e2e:webview                         # 準備と実行をまとめて行う
```

- `npm run e2e:webview:prepare` は実アプリの実行ファイル(`<CARGO_TARGET_DIR>/debug/brainfuck-studio.exe`。`CARGO_TARGET_DIR` が無ければ `src-tauri/target/debug/brainfuck-studio.exe`)と画面のビルド成果物 `dist/` を作ります。ビルドには数分かかります。
- `tauri build` は `apps/desktop/src-tauri/Cargo.toml` を書き換えることがあります(feature の同期)。準備の後に `git status` で製品の差分が出たら、`git checkout -- src-tauri/Cargo.toml` で戻してください。
- `npm run e2e:webview:run` は、実行の前に準備の確認を行います。Windows かどうか、WebView2 の版、実行ファイル、`dist/` の有無を見て、不足があれば一覧と準備のコマンドを示して終了コード 1 で止まります。
- `E2E_APP_BIN` で実行ファイルを明示できます(相対パスは `apps/desktop` 基準)。別の場所で再ビルドした実行ファイルを試すときに使います。

#### 環境変数

| 変数 | 用途 |
| --- | --- |
| `E2E_APP_BIN` | 実アプリの実行ファイルを明示する。未指定なら `CARGO_TARGET_DIR` の下の `debug/`、それも無ければ `src-tauri/target/debug/` の `brainfuck-studio.exe` |
| `E2E_APP_READY_MS` | アプリのデバッグ用の口が応答するまで待つ上限のミリ秒。既定は 60000。正の整数以外は既定になります。ウイルス対策ソフトの検査などで起動が遅いときに延ばします |
| `E2E_PORT` | 配信ポート(`vite preview` が使う)。未指定なら、`npm run e2e:webview:run` が空きポートを選びます。指定すると、その番号を使います |

アプリのデバッグ用の口(CDP)のポートは、シナリオごとに空きポートを自動で選びます。指定する手段はありません。

### WebView2 の版の確認

- 準備の確認は、WebView2 の版を EdgeUpdate の登録(`pv`。HKLM の `WOW6432Node`、続けて HKCU)から読み、結果の注釈に記録します。版が無ければ、準備の確認で失敗します。
- シナリオごとに、アプリのデバッグ用の口の `/json/version` の `Browser`(`Edg/<版>`)も読み、そのシナリオの結果に記録します。
- 手動で確かめるときは、Microsoft Edge WebView2 のランタイムの版を Windows の「インストールされているアプリ」で見るか、上記のレジストリの `pv` を見てください。

### デバッグ用の口の扱い

- デバッグ用の口は、テストが起動したアプリのプロセスだけに、環境変数 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=0` で開きます。アプリの既定の引数は消えず、テストの起動のブラウザプロセスの引数に追加されるだけです。
- ポートは固定せず 0 番(OS が空きポートを選ぶ)にします。空きポートを先に取って渡す方式は、取ったあとに別のプロセスが使う競合が起きうるためです。実際のポートは、WebView2 が利用者データのフォルダ(`<一時フォルダ>/bfstudio-e2e-webview/scenario-<uuid>`)の下の `EBWebView/DevToolsActivePort` に書く 1 行目(ポート番号。2 行目は WebSocket のパス)から読みます。このファイルは起動の前に消し、現れるまで待ちます。上限(`E2E_APP_READY_MS`)は、ファイルを待つ段と CDP の応答を待つ段のそれぞれに適用されるため、合計では最大で上限の 2 倍まで待つことがあります。現れる前にアプリが終了したら早期終了として失敗します。
- CI(`windows-latest`)では、ランナーのアプリが管理者権限(昇格)で動きます。昇格したホストの WebView2(Runtime 150 以降)は環境変数を無視するため、上の方式だけではデバッグ口が開きません。そこで `e2e-webview` の job の手順で、HKLM のポリシー `HKLM\SOFTWARE\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments` の値 `brainfuck-studio.exe` に `--remote-debugging-port=0` を書きます(WebView2Feedback #5640)。この設定は管理者で動くランナー専用で、昇格していないローカルの PC には影響しません(ローカルでは環境変数が効きます)。
- 口は `127.0.0.1` にだけ開きます。テストの終了時には、起動したプロセスのツリーを `taskkill /T /F` で終わらせます。
- 利用者データのフォルダは、シナリオごとの一時フォルダ(`<一時フォルダ>/bfstudio-e2e-webview/scenario-<uuid>`)です。通常の起動の利用者データには触れません。

### CDP の口が開かないときの早期確定

- 実行の中で、最初に CDP の口が応答しなかった(`AppLaunchError` の `notReady`)シナリオが出ると、その事実を実行ごとのマーカー `<一時フォルダ>/bfstudio-e2e-webview/launch-gate.json` に記録します。以降のシナリオは実アプリを起動せず、理由つきでスキップします(スキップの理由に元の失敗の要約を含みます)。最初のシナリオは失敗として残り、その再試行はスキップになります。これにより、GitHub のランナーのように口が開かない環境で、約 40 分かかる job が数分で終わります。
- マーカーは `npm run e2e:webview:run` の開始時に setup プロジェクト(`prerequisites.setup.ts`)が消します。消せないときは、準備の確認そのものが失敗して実行は始まりません(古い記録で後続が誤ってスキップされないため)。
- 起動の失敗が `exitedEarly`(早期終了)・`notFound`・アサーションの失敗のときは記録しません。個別の失敗はこれまでどおりすべて実行されます。
- 最初の失敗したシナリオは失敗として残るため、実行全体は失敗(赤)になります。後続はスキップ(Playwright の結果では「skipped」)になり、結果の注釈に理由が出ます。CI の job は `continue-on-error` の非必須なので、PR の取り込みは妨げません。
- setup プロジェクトを飛ばす実行(`--no-deps` など)では、古いマーカーが残っていると後続が誤ってスキップされます。その場合は上記のマーカーのファイルを手で消してください。
- ローカルで試すときは、`E2E_APP_READY_MS` を 1 などに極端に小さくすると、`notReady` を意図的に起こせます。実行の後にマーカーが残っても、次の実行の開始時に消えます。

### 実行中の注意

- **実行中は、キーボードとマウスを操作しないでください。** シナリオは OS のキー入力(F5、Ctrl+R など)と、実アプリのウィンドウの前面化・OS の閉じる要求を使います。途中で操作すると、結果が誤ったものになります。
- 同時に複数の実行を走らせないでください。実アプリは 1 度に 1 つだけ起動します。
- ウィンドウを前面にできない場合、キー入力は誰にも送らずに失敗します。そのシナリオは理由つきで失敗し、合格にはなりません。
- 固まったアプリのプロセスは、再起動まで消えないことがあります。その場合は、タスクマネージャーなどで `brainfuck-studio.exe` を終了させてからやり直してください。

### 自動化の範囲

- **閉じる要求**は、タイトルバーの × ボタンの物理的なクリックではなく、OS の閉じる要求(ボタンと同じ経路)で代替しています。この代替は `.kiro/specs/desktop-gui-e2e-webview/traceability.md` にも記載しています。
- 次の項目は自動化せず、手動のまま残しています。ネイティブダイアログの見た目とモーダル性、ウィンドウのサイズ変更と最小サイズ、OS の配色(ライト・高コントラスト)、スクリーンリーダー(NVDA)、実ウィンドウでの 11.4。
- フォーカス枠(13.3)は、OS の現在の配色だけで確認します。配色の切り替えは確認していません。
- 9.3 の 1 秒の応答性の計測は、ブラウザのスイートの `@perf` が担い、実アプリでの計測は手動のままです。webview では、上限なしの実行で実行数が増え続けることだけを確かめます。

### 開発 PC でウイルス対策ソフトが止める場合

新しく作られた実行ファイルが止められて固まる場合の対処(除外の設定、一時停止、新しい `CARGO_TARGET_DIR`)は、[開発 PC でウイルス対策ソフトが止める場合](#開発-pc-でウイルス対策ソフトが止める場合)(ブラウザのスイート)と同じです。ここでは重複して書きません。

### CI の job(非必須)

GitHub Actions の `e2e-webview` job(`windows-latest`)は、`continue-on-error: true` の非必須です。webview スイートの失敗は結果に表示されますが、CI 全体の失敗や、プルリクエストの取り込みの妨げにはなりません。失敗時のレポートとスクリーンショット、実アプリのログは成果物として保存されます。`main` で連続 10 回成功したら、`continue-on-error` を外して必須のチェックへの登録を検討してください。

## 型の再生成

Rust 側の DTO を変えたときは、TypeScript の型を再生成します。

```sh
cargo run -p brainfuck-gui-api --example export_bindings
```

出力先は `apps/desktop/src/generated/` です。生成物はコミットし、手で編集しないでください。`bindings_up_to_date` テストが生成物と Rust 側の定義の不一致を検出します。

Windows の `autocrlf` 設定によっては、内容が同じでも改行コードだけが差分として見えることがあります。その場合は `git diff --ignore-cr-at-eol` で内容の差分だけを確認してください。

## キーボードショートカット

| 操作 | ショートカット | 備考 |
| --- | --- | --- |
| Run(実行) | `F5` | 押しっぱなしでも連続して発火しません |
| Pause(一時停止) | `F6` | |
| Step(1 命令実行) | `F11` | |
| Reset(リセット) | `Shift+F5` | ブレークポイントと入力は保持します |
| ブレークポイントの切替 | `F9` | エディタで現在行に対して切り替えます。入力欄では発火しません |
| 新規 | `Ctrl+N` | 未保存の変更がある場合は確認します |
| 開く | `Ctrl+O` | 未保存の変更がある場合は確認します |
| 保存 | `Ctrl+S` | |
| 名前を付けて保存 | `Ctrl+Shift+S` | |

次のキーは割り当てがありません。ブラウザのリロードに当たるため、既定の動作だけを抑止しています(未保存の編集が確認なしで失われるのを防ぐため)。

- `Ctrl+R`、`Ctrl+Shift+R`、`Ctrl+F5`、`Ctrl+Shift+F5`、および `F5` の押しっぱなし

ツールバーのボタンにも同じショートカットを表示しています。

## CLI との一致検証の対象外

`brainfuck-cli` との結果一致(要件 11)は、GUI が受け付ける入力の範囲で検証しています。次の 2 点は意図的に対象外です。

1. **命令数上限 `0`**: GUI は命令数上限に 1 以上の値だけを受け付けます(要件 5.3)。CLI の `--max-steps 0` は受け付けるため、結果を比較しません。
2. **非 UTF-8 ファイルのバイトオフセット**: GUI は不正なバイトを置換文字へ置き換えて読み込みます(要件 2.12)。そのため、解析エラーの `byteOffset` は置換後の文字列上の値になり、CLI が実ファイルのバイト列に対して報告するバイトオフセットとは異なることがあります。命令の添字・エラーの種類・`pc`・`dp`・出力には影響しません。

## 手動スモーク手順

自動テストは WebView を起動せずに動作を検証します。次の項目は実際のウィンドウで確認する必要があるため、手動で確認してください。各項目の結果は、下の「検証結果の記録」に実施の有無と結果を書いてください。

### 1. 起動と初期状態

1. `npm run tauri dev` で起動する。
2. 次を確認する: `hello.bf` が編集領域に読み込まれている、状態ラベルが「未開始」、Run・Step が使える状態(Pause・Reset は無効)、ウィンドウのタイトルが「Brainfuck Studio」。

### 2. 編集

1. 編集領域に文字を入力・削除する。未保存の変更が表示されること、Ctrl+Z で取り消せること。
2. 括弧の対応強調を確認する: 対応する括弧が両方強調され、対応が無い括弧は対応ありとは区別できる表示になること。
3. 日本語・絵文字(サロゲートペア)を含む行を編集し、強調の位置がずれないこと。
4. 改行コードが CRLF のテキストを貼り付け、現在命令の強調位置がずれないこと。

### 3. Hello World の実行と CLI との比較

1. 初期の `hello.bf` を Run する。出力に `Hello World!` が表示され、状態の表示で pc が 106、dp が 6、命令数が 906 になること。
2. 同じプログラムを CLI で実行し、画面の結果と比べる。
   ```sh
   cargo run -p brainfuck-cli -- examples/hello.bf --dump-state
   ```
   出力が `Hello World!` と改行 1 つになること、`--dump-state` の `pc`・`dp`・`steps` が画面の値と一致すること。

### 4. 上限なしの無限ループの Pause と再開

1. 命令数上限を「上限なし」にし、`+[]` を入力して Run する。
2. Run の後、画面が固まらずに 1 秒以内に Pause を押して一時停止できること。状態ラベルが「一時停止」になること。
3. Run で再開できること。Pause ボタンを押した後、ボタンの有効・無効が正しいこと。
4. 上限なしの確認時にも警告が表示されること。

### 5. Step(1 命令実行)

1. ブレークポイント無しで F11 を連続して押す。1 回ごとに `pc` が進み、出力と状態が更新されること。
2. ループ(`[` ・ `]`)の内側と外側で、Step の動作が自然であること。
3. 文書の終端で Step が無効になり、状態ラベルが「正常終了」になること。
4. Step を続けると、現在命令の強調が進んだ位置に追従し、位置が画面の外にあるときはエディタがスクロールして現在命令が見えること(Run で停止した場合、ブレークポイント停止の位置も同様)。

### 6. Reset

1. 一時停止または停止の後に Shift+F5 を押し、初期状態に戻ること。出力と状態がクリアされること。実行中は Reset が無効で、Shift+F5 を押しても何も起きないこと。
2. 設定したブレークポイントと入力(16 進またはテキスト)が保持されていること。

### 7. ブレークポイント

1. F9 と、ガター(行番号の右側)のクリックの両方で、ブレークポイントを付け外しできること。F9 は、カーソル以降で最初の命令に付くこと(要件 6.2)。ブレークポイントのマーカーは行番号の右側に表示されること。
2. ブレークポイントを置いた状態で Run し、その位置で停止して状態ラベルが「ブレークポイントで停止」になること。
3. Run で続きから再開し、同じ位置で再び止まらずに実行が進むこと。
4. 入力欄(入力の編集中)で F9 を押しても、ブレークポイントが切り替わらないこと。

### 8. 入力と命令数上限

1. 入力を 16 進とテキストで切り替え、表示と入力内容が対応すること。
2. 不正な 16 進(奇数桁、16 進でない文字)を入力すると、理由が表示されること。
3. 命令数上限に不正な値(0・負数・小数・空・数字以外)を入れると、値を受け付けず、直前の有効な上限を維持し、理由が表示されること(要件 5.3)。
4. 命令数上限に達したプログラムが、上限で停止すること。

### 9. バッチ実行の応答性

1. 長時間かかるプログラム(上限なしの `+[]` など)を Run しても、ウィンドウが応答し続けること(ボタンのホバーやテキスト選択が反応すること)。
2. Pause の後に表示が更新され、出力とテープの表示が 1 秒以内に追いつくこと。
3. `+[]` を上限なしで Run し、命令数の表示が 1 秒以内ごとに増え続けること。

### 10. 保存・開く・名前を付けて保存

1. Ctrl+S(保存)と Ctrl+Shift+S(名前を付けて保存)が、ネイティブのファイルダイアログを開くこと。ダイアログがメインウィンドウのモーダルになること。
2. ダイアログをキャンセルしたとき、ファイルが書き込まれず、未保存の状態が保たれること。
3. 保存に成功したとき、未保存の表示が消え、ウィンドウに保存したファイル名が表示されること(パスは表示されないこと)。
4. 保存に失敗したとき(読み取り専用のファイル、存在しないディレクトリ)、原因つきのエラーが「操作エラー」の見出しの領域に表示され、編集内容が残ること。
5. 保存のダイアログを開いている間に編集を続け、保存後も未保存のままであること(保存時点の内容が保存されるため)。
6. CRLF の改行コードのファイルを開いて保存し、改行コードが CRLF のまま保たれること。
7. UTF-8 以外のファイル(例: Shift_JIS)を開いたとき、不正なバイトが置換文字に置き換えられ、見出しが「通知」の通知が表示されること。
8. 1 MiB を超えるファイルを開くと、拒否されて編集内容が変わらないこと。
9. Ctrl+N(新規)と Ctrl+O(開く)も、未保存の変更がある場合に確認を出すこと。

### 11. 未保存での閉じる確認

1. 未保存の変更がある状態で、ウィンドウの × を押す。アプリは開いたまま、確認ダイアログが 1 回だけ表示されること。
2. 「キャンセル」を選ぶと、アプリは開いたままで編集内容が残ること。
3. 「破棄して閉じる」を選ぶと、アプリが終了すること。
4. 保存済み(または無変更)の状態で × を押すと、確認なしで即座に終了すること。
5. 確認ダイアログが二重に出ないこと。
6. F5 を押しっぱなしにしても、ページがリロードされず、編集内容が残ること。Ctrl+R と Ctrl+F5 でもリロードされないこと。
7. 右クリックしても既定のメニュー(「最新の情報に更新」など)が出ないこと。出ても未保存の文書が失われないこと。

### 12. 最小ウィンドウとレイアウト

1. ウィンドウを最小サイズ(800 × 600)まで縮め、必要な領域(ツールバー・編集領域・入力・テープ・出力・状態)に到達できること。
2. 800 × 600 でツールバーが折り返され、スクロールで全ての操作に届くこと。画面外に切れて操作できない領域が無いこと。
3. ウィンドウを大きくしても、レイアウトが崩れないこと。

### 13. キーボードのみの操作

1. マウスを使わず、Tab・Shift+Tab で全ての操作領域とボタンに移動できること。
2. Tab の移動順が、画面の視覚的な順序と一致すること(CSS grid の配置と DOM の順序が一致すること)。
3. フォーカスの枠線(`:focus-visible` の outline)が見えること。エディタ(`.cue-frame` など)と重なる箇所でも見えること。ライトテーマとダークテーマの両方で確認すること。
4. Enter・Space で Run・Pause・Step・Reset などのボタンが動くこと。

### 14. 見た目・アクセシビリティ

1. ライトテーマとダークテーマの両方で、文字と背景のコントラストが十分であること。
2. OS の高コントラストモードで、枠・ボタン・フォーカス・波線の下線が見えること。
3. 波線の下線(エラー位置)、枠、ブレークポイントのマーカー、括弧の一致表示が描画されること。
4. 非表示の見出し(h1)と、状態ラベルのライブ領域が、スクリーンリーダー(NVDA など)で読み上げられること。

### 15. 実行エラーの表示

1. 命令数上限を超えた場合、実行エラー(上限到達)の内容が状態・エラーの領域に表示されること。
2. 解析エラー(括弧の不整合など)のあるソースでは、エラー位置がエディタで強調されること。
3. 実行エラーが発生した後も、編集と再実行(Reset)ができること。

## 検証結果の記録

### 自動テスト

| 項目 | 結果 |
| --- | --- |
| Rust: `cargo test --workspace`(ルート) | 成功(exit 0)。全 55 テストバイナリ・938 件が成功、失敗 0 |
| Rust: `cargo test -p brainfuck-gui-api --test bindings_up_to_date` | 成功。4 件が成功 |
| Rust: `cargo test`(`apps/desktop/src-tauri`) | 成功(exit 0)。統合テスト 27 件(`commands` 9、`files` 13、`window` 5)が成功、失敗 0。新しいビルドディレクトリのフルビルドで実行した |
| Rust: `cargo check`(`apps/desktop/src-tauri`) | 成功(exit 0) |
| Rust: `cargo clippy --all-targets -- -D warnings`(`apps/desktop/src-tauri`) | 成功(警告なし) |
| Rust: `cargo fmt -- --check`(`apps/desktop/src-tauri`) | 成功 |
| フロント: `npm run typecheck` | 成功(exit 0) |
| フロント: `npm run lint` | 成功(exit 0) |
| フロント: `npm test`(vitest) | 成功。41 ファイル・780 件が成功、失敗 0 |
| フロント: `npm run build` | 成功(exit 0) |
| 型の再生成: `cargo run -p brainfuck-gui-api --example export_bindings` | 実行して内容差分が無いことを確認。差分は改行コード(CRLF)のみで、生成物は HEAD に戻した |
| CLI: `cargo run -p brainfuck-cli -- examples/hello.bf --dump-state` | 出力は `Hello World!` と `state: pc=106 dp=6 steps=906`(CLI 側の値。GUI 側は手動スモーク 3 で未実施) |

### 権限と lint

- **`unsafe_code` の lint**: `apps/desktop/src-tauri/Cargo.toml` に `[lints.rust] unsafe_code = "forbid"` を宣言しています。`cargo check` と `cargo clippy --all-targets -- -D warnings` が通ることを確認しており、tauri のマクロ展開と衝突しません。`deny` へは落としていません。アプリ側のソースに `unsafe` はありません。
- **Cargo.lock の一致**: `cargo tree -i serde -i ts-rs` で比較した結果、アプリ側とルートの解決バージョンは一致しています(`serde` 1.0.229、`ts-rs` 12.0.1)。ビルドは伴わない読み取りのみの確認です。
- **ケイパビリティ**: `capabilities/default.json` の許可は `core:event:default` と自アプリの `allow-*` のみで、`fs`・`shell`・`http`・`dialog`・`core:window:*` の許可はありません(ファイルの目視確認)。自動テスト(`tests/commands.rs` のケイパビリティの内容検査。許可の集合が登録済みコマンドと一致すること、`fs`・`dialog`・`shell`・`http`・`core:window:*` を含まないこと)も成功しています。

### 手動スモーク

2026-11-07 に、`npm run tauri dev` で起動した開発ビルドの実ウィンドウ(WebView2)を、`orca computer`(アクセシビリティツリーとクリック操作)で操作して確認しました。上記の手順のうち、次の項目だけを実施しています。残りの項目は**未実施**です。未実施の項目を「合格」と書かないでください。

| 項目 | 結果 |
| --- | --- |
| 1(起動と初期状態) | 合格。ウィンドウのタイトルは「Brainfuck Studio」。Hello World が「無題」の文書として表示された。Run・Step が有効で Pause は無効 |
| 3.1(Hello World の実行) | 合格。出力は `Hello World!`、状態は「正常終了」、pc 106・dp 6・実行数 906(CLI の `--dump-state` と一致)。3.2(CLI との並べた比較)は、CLI 側の値を上の記録と照合したのみ |
| Reset(4.9・4.10) | 合格。Reset で「未開始」・出力なしに戻り、Run・Step・Reset が有効、Pause が無効になった |
| 4(上限なしの `+[]` と Pause・再開) | 合格。「上限なしで実行します」の警告が出て、実行数が 2 秒ごとに増え続けた(約 3.7 億→4.6 億。要件 5.9)。Pause で値が止まり(約 6.1 億で停止)、Run で再開して増え始めた |
| 13(未保存で閉じる) | 合格。未保存の状態で閉じるボタンを押すと「未保存の変更」の確認が出てウィンドウは閉じず、「破棄して閉じる」でプロセスが終了した |

同日に、残りの項目も同じ方法で実施しました(F5・F6・F9・F11 のキー、Ctrl+R、ネイティブのダイアログ、ウィンドウのサイズ変更は、`orca computer` の `press-key` が受け付けないため、PowerShell の SendKeys、UI Automation、Win32 API で代用)。

| 項目 | 結果 |
| --- | --- |
| 2(編集、括弧の強調、日本語・絵文字、CRLF の貼り付け) | 合格。2.4 はエディタが貼り付け時に LF へ正規化するため、強調位置のみ確認(ファイル側の CRLF 保持は 10.6) |
| 5(Step、強調への追従) | 合格 |
| 6(Reset、ブレークポイントと入力の保持、実行中は無効) | 合格 |
| 7(ブレークポイントの切り替え・停止・再開、入力欄での F9) | 合格 |
| 8(入力形式、不正な 16 進、不正な上限、上限での停止) | 合格(空欄の上限は理由の文が空と報告があったが、修正後のコードでは表示される想定。実機では未再現) |
| 9(応答性、実行数の増加) | 部分合格。ウィンドウは応答し、実行数は増え続けた。1 秒以内の更新周期と Pause の 1 秒以内、ホバー・テキスト選択は計測できていない |
| 10(保存・開く・名前を付けて保存) | 合格。10.4 の存在しないディレクトリは OS のダイアログが拒否するため到達不能、10.5(モーダル中の編集)はモーダル中に編集できないため実施不能 |
| 11(確認ダイアログ、二重表示、リロード抑止、右クリック) | 合格。押しっぱなしの自動リピートは再現できず連打で代用。編集領域の右クリックでは WebView 既定の編集メニューが出るが、「最新の情報に更新」は無く文書は失われない |
| 12(最小サイズ 800×600、折り返し、最大化) | 合格 |
| 13(Tab 移動・順序・Enter/Space) | 合格。13.3 のエディタのフォーカス枠は、初回は見えなかったため修正し、修正後に実機で枠が出ることを確認(ライトテーマと高コントラストモードは未確認) |
| 14.3(描画) | 合格 |
| 15(実行エラー・解析エラーの表示、再実行) | 合格 |

実施不能として未確認のもの: 14.1(ライトテーマ。ダークのコントラスト比はトークンから算出して WCAG AA 以上)、14.2(高コントラストモード)、14.4(NVDA)。

気づいた点(不合格ではない):

- 実機の確認で見つかった、エディタのフォーカス枠、ガターの白い背景、幅 1040 px での右カラムの切れは修正済みです(ガターは暗い配色になり、右カラムが収まることを実機で確認)。
- 操作エラーの通知は、別の通知に置き換わるまで残ります。要件に消去の規定が見つからないため、仕様として扱っています。
- ソースが長いと、エディタが内容の高さまで伸びてページ全体がスクロールします。
- `npm run tauri build` は**未実施**です。

### E2E(ブラウザ)の検証結果

2026-11-08 に、`desktop-gui-e2e-browser` の最終検証を実施しました。E2E(ブラウザ)の行は、`fixme` を外した後の 2026-11-09 の再実施の結果です。

| 項目 | 結果 |
| --- | --- |
| `cargo fmt --all -- --check` | 成功 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 成功(警告なし) |
| `cargo test --workspace` | 成功。56 テストバイナリ・1,007 件が成功、失敗 0(ブリッジの crate `brainfuck-e2e-bridge` のテストを含む) |
| `npm run typecheck`(製品と E2E) | 成功 |
| `npm run lint` | 成功 |
| `npm test`(vitest) | 成功。57 ファイル・1,011 件が成功(E2E の補助の単体テストを含む) |
| `npm run build` | 成功。`dist/` に IPC シム(`__bfE2e`)は含まれない |
| `npx playwright test --project=chromium`(`npm run e2e:run`) | 成功。`--workers=1` で、88 件すべてが成功、flaky 0、スキップ 0(1.6 分)。`--workers=1` で `smoke-02-editing.spec.ts` と `smoke-05-step.spec.ts` を `--repeat-each=3` で実行すると、30 件すべてが成功(33.9 秒)。4 ワーカーの並列では、2 回目の実行で 88 件すべてが成功(84 件が初回で成功、4 件が再試行で成功 = flaky、失敗 0、2.0 分)。同じ 4 ワーカーの 1 回目の実行では、`page.goto` の `ERR_ABORTED` で 5 件が失敗したため(上の「並列度と製品の不具合の記録」を参照)、確認は `--workers=1` でも行った |
| `npx playwright test --project=perf`(`npm run e2e:perf`) | 成功。3 件が成功。実測値(直列、`--repeat-each=3` の範囲): Pause のクリックから `pause` の完了まで 164〜174 ms、実行数の表示の更新間隔の最大 69〜75 ms、Pause 後の表示の追いつき 22〜24 ms(いずれも閾値 1,500 ms 以下) |
| 製品のソースの差分 | なし(`apps/desktop/src`、`src-tauri`、`index.html`、`vite.config.ts`、`tsconfig.json`、既存の `crates/*` に変更なし。新設は `crates/brainfuck-e2e-bridge/`、設定、文書、ワークフローのみ) |
| `cargo tree -i brainfuck-e2e-bridge` | 自身のみ(他の crate はブリッジに依存しない) |

製品の不具合の記録(追跡先は `.kiro/specs/desktop-gui-e2e-browser/traceability.md` の「見つけた製品の不具合」)。以前は `test.fixme` の 2 件(スモーク 2.1 の未保存の表示、スモーク 5.3 の終端のボタンの無効化)でしたが、desktop-gui の PR #12 で修正され、`test.fixme` を外して通常のテストにしました。現在 `test.fixme` は 0 件です。

実施時の注意:

- 実行した E2E は、実機のテストではなく Chromium と実 Rust のブリッジによるものです。実ウィンドウ・WebView2 の挙動(OS の閉じるボタン、実リロード、WebView2 の右クリックメニュー、CSP の実効、ネイティブダイアログ)は `desktop-gui-e2e-webview` の範囲、または手動のままです。
- この開発 PC では、新しくビルドされたブリッジの実行ファイルがウイルス対策ソフトに止められ、起動時に固まりました(新しい `CARGO_TARGET_DIR` で再ビルドした実行ファイルが `e2e:ping` に応答しなかった)。上の E2E は、すでに実行済みで起動できるブリッジ(同じソースからビルドしたもの)を `E2E_BRIDGE_BIN` で指定して実行しました。新しいビルドで E2E を回すには、ウイルス対策ソフトの除外設定が必要です。GitHub Actions(Linux)の job は未実行で、PR の CI で初めて確認されます。
- 安定性: 複数のブラウザやブリッジが同時に動くマシン(例: 他の作業が走っている間)では、`page.goto` がタイムアウトすることがあります。単独の実行では通ります。

### E2E(実アプリ・WebView2)の検証結果

2026-11-09 に、`desktop-gui-e2e-webview` の最終検証を Windows の開発 PC で実施しました。WebView2 のランタイムは **158.0.4370.7**(結果の注釈にも記録)です。

| 項目 | 結果 |
| --- | --- |
| `npm run typecheck`(製品と E2E) | 成功 |
| `npm run lint` | 成功 |
| `npm test`(vitest) | 成功。67 ファイル・1,192 件が成功(webview の補助の単体テストを含む) |
| `cargo test`(`apps/desktop/src-tauri`) | 成功。27 件(`commands` 9、`files` 13、`window` 5)、失敗 0 |
| `npm run e2e:webview:prepare`(`tauri build --debug --no-bundle`) | 成功。実行ファイルは `<CARGO_TARGET_DIR>/debug/brainfuck-studio.exe`。画面(`dist/`)を組み込んだモードで、`devUrl` は使わない |
| `npm run e2e:webview:run`(`E2E_APP_BIN` に上の実行ファイルを指定) | 成功。**20 件が成功**(準備の確認 1、起動 3、Hello World 1、Pause と再開 1、閉じる確認 4、リロードと右クリックの抑止 4(本体 2 と陽性対照 2)、フォーカス枠 2、CSP 4)、失敗 0、スキップ 0(1.1 分)。実行後に実アプリのプロセスと一時フォルダは残らない |
| 上流の `npx playwright test --list`(`E2E_SKIP_PREREQ=1`) | 91 件・16 ファイルで変わらず(webview のプロジェクトは含まれない) |
| `npm test` の対象 | `e2e/webview/` のシナリオは含まれない(`e2e/support/**/*.test.ts` だけ) |
| 製品のソースの差分 | なし(`apps/desktop/src`、`src-tauri`、`crates/*/src`、`index.html`、`vite.config.ts`、`tsconfig.json` に変更なし。`tauri build` が書き換える `src-tauri/Cargo.toml` は、実行の後に `git checkout` で戻した) |

通常の起動とテストの起動のブラウザプロセスの引数の比較(実測): テストの起動では、アプリの既定の引数(`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` など)が残ったまま `--remote-debugging-port=<port>` が追加され、`--user-data-dir` はシナリオごとの一時フォルダ(`scenario-<uuid>/EBWebView`)を指します。差は `--remote-debugging-port` と `--user-data-dir` の値だけです(起動のたびに変わる mojo の pipe 名は除く)。`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` はアプリの指定に追加されるため、アプリの既定の引数は消えません。

実装で分かった事実:

- **IPC の観測は postMessage 方式です。** 製品の CSP(`connect-src` 未指定)が Tauri の `fetch` 方式の IPC(`http://ipc.localhost/<cmd>`)を拒否するため、Tauri は `window.chrome.webview.postMessage` に自動で切り替わり、応答は `__TAURI_INTERNALS__.runCallback` で返ります。ネットワーク要求の観測も `invoke` の包み込みも実アプリでは成立しませんでした(`research.md` に根拠)。実アプリは毎回起動時に、`fetch` の IPC が拒否されたことを示す `console.warn` を出しますが、機能には影響しません(CSP に `connect-src 'self' ipc: http://ipc.localhost` を足すと Tauri が想定する `fetch` 方式になりますが、製品の変更でこの feature の範囲外です)。
- **既定のコンテキストメニューの検出**: 最初の実装は UI Automation のルート直下の `Menu` を探しており、メニューが出ていても常に偽でした(11.7 の否定が何があっても合格する状態)。陽性対照が見つけ、`Chrome_WidgetWin_*` の最上位ウィンドウを列挙して UIA の部分木から `Menu` を探す形に直しました。
- **前面化**: Windows の前面化ロックで `AppActivate` だけでは実アプリが前面にならず、OS のキー入力が失敗しました。復元と `AttachThreadInput` による `SetForegroundWindow` を足しました(入力を送らずに前面化を試し、前面にならなければ何も送らずに失敗する。送信の直前直後に前面を再確認する)。
- 新しくビルドした実行ファイルは、この開発 PC で Avast に止められずに起動できました(以前は、新しくビルドした別の実行ファイルが固まったことがあります)。固まる場合は「トラブルシューティング」を参照してください。
- GitHub Actions の `e2e-webview` の job は PR #14 で実行しましたが、`windows-latest` のランナー(WebView2 ランタイム 157.0.4341.42)では、環境変数 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` が msedgewebview2 の起動引数に反映されず、CDP の口が開かないため全シナリオが `AppLaunchError`(notReady)で失敗しました(ローカル(ランタイム 158.0.4370.7)では成功)。原因は、ランナーのアプリが管理者権限(昇格)で動き、昇格したホストでは WebView2 が環境変数を無視することです(WebView2Feedback #5640)。CI の実験で、HKLM のポリシー(上の「デバッグ用の口の扱い」)と `DevToolsActivePort` の 1 行目からポートを読む方式で成功することを確かめ、job の手順にポリシーを書く手順を足しました。job は `continue-on-error` の非必須のままとし、必須への昇格は見送ります。

## トラブルシューティング

- **ビルドスクリプトやテストの実行ファイルが起動時に固まる**: 新しく作られた実行ファイルの起動を、ウイルス対策ソフトが止めている場合があります。プロセスが CPU を使わず進まない場合は、リポジトリのフォルダー、`CARGO_TARGET_DIR` に指定したフォルダー、`.cargo` を、ウイルス対策ソフトの除外に追加してから、やり直してください。固まったプロセスは終了できないことがあり、その場合は再起動で消えます。同じビルドディレクトリで繰り返すと、実行ファイルのロックでリンクに失敗することがあるため、新しい `CARGO_TARGET_DIR` を指定すると回避できます。
- **`npm` や `node` のバージョンが合わない**: `mise exec node@26 -- npm ...` の形で実行してください。
