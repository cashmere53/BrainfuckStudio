# Research & Design Decisions(desktop-gui-e2e-browser)

## Summary
- **Feature**: `desktop-gui-e2e-browser`
- **Discovery Scope**: Extension(完成済みの desktop-gui に、製品を変えずにテスト層を足す。新しい外部依存は `@playwright/test` のみ)
- **Key Findings**:
  - フロントが Tauri に触れる点は `src/ipc/backend.ts` の `invoke`(`@tauri-apps/api/core`)と `listen`(`@tauri-apps/api/event`、`close-requested` のみ)だけで、どちらも呼び出し時に `window.__TAURI_INTERNALS__` を参照する。初期化スクリプトでこれを差し替えれば、製品のソースを 1 行も変えずに IPC を橋渡しできる
  - Tauri コマンド層(`src-tauri/src/commands/{debugger,files}.rs`)は `DebuggerService` と `source_file` への薄い委譲で、Tauri に固有なのはダイアログ(`blocking_pick_file` / `blocking_save_file`)と終了(`app.exit`)、文書状態(`app_state.rs` の `DocumentState`)、閉じる判定(`window.rs`)だけ。これらは数十行で、テスト専用バイナリで同じ順序を再現できる
  - `run_batch` は `Mutex` を 1 バッチの間保持し、`pause` は `AtomicU64` と `AtomicBool` だけを触る。Tauri の `async` コマンドは別スレッドで並行に動くため、ブリッジも「要求ごとにスレッド」で処理すれば Pause の経路が実アプリと同じになる

## Research Log

### IPC の差し込み口
- **Context**: brief は (a) `VITE_E2E` で切る製品側の入口と (b) `addInitScript` で注入するバンドルの二択を design に委ねた
- **Sources Consulted**: `apps/desktop/src/ipc/backend.ts`、`src/main.tsx`、`src/app/DebuggerScreen.tsx`(`backend` prop は既定で `tauriBackend`)、`@tauri-apps/api/mocks` の `mockIPC` / `clearMocks`(検証済みの事実: `window.__TAURI_INTERNALS__` だけを触り、実ブラウザで動く。`shouldMockEvents` は api ≥ 2.7 で `listen` / `emit` を模擬し、アプリは ^2.12.4)、Playwright の `page.addInitScript`(ページのスクリプトより前に実行)
- **Findings**:
  - (a) は `main.tsx` に条件付きの動的 import を足す。`import.meta.env` の静的置換で配布物から除去できるが、製品のエントリにテストの分岐が入り、「配布物にテストの仕組みを含めない」ことを別途検査する必要がある
  - (b) は製品のソースを変えない。`vite build` の成果物(配布と同じ)をそのまま検証できる。代わりに、`mockIPC` を含む小さな IIFE バンドルを別に作る手順が要る
  - Playwright の `addInitScript` を複数登録した場合の実行順は保証されないため、設定値と本体は 1 つのスクリプト文字列にまとめる必要がある
- **Implications**: (b) を採用(Design Decisions 参照)。製品のソースの変更はゼロになる

### ブラウザから Node への中継の経路
- **Context**: brief は Node 側に HTTP 中継を置く案だった。ワーカーごとのポート割り当てと CORS が必要になる
- **Sources Consulted**: Playwright の `page.exposeFunction`(ページ内の関数呼び出しを Node の非同期関数へ転送し、ナビゲーション後も残る。初期化スクリプトと同じくページのスクリプトより前に定義される)
- **Findings**:
  - `exposeFunction` を使うと、ブラウザ→Node の転送にネットワークのポートが要らず、CORS もプリフライトも無い。呼び出しは並行に処理される(実行中の `run_batch` を待たずに `pause` が届く)
  - Node 関数が投げた例外はページ側では `Error` の文字列になり、構造化された `ApiError` が失われる。Node 側は常に `{ ok, value | error }` の封筒で resolve し、ページ側のシムが封筒を開いて元の値で reject する必要がある
- **Implications**: 中継は Node のテストプロセス内に置き、HTTP サーバは作らない。ポートを使うのは静的配信(`vite preview`)だけになる

### Tauri の引数・応答・エラーの形
- **Context**: ブリッジが実アプリと同じ形を返すため
- **Sources Consulted**: `backend.ts` の `invokeCommand`(引数は `{ args }` の 1 個。引数なしのコマンドは `undefined`。`setDocumentDirty` は `{ args: { dirty } }`)、`errors.ts` の `normalizeApiError`(`ApiError` の形でない拒否値は `internal` に正規化)、`dto.rs`(要求 DTO は `deny_unknown_fields`、`ApiError` は `kind`・`message`・`field`)
- **Findings**:
  - 実アプリでは、コマンドの `Err(ApiError)` は JSON の `ApiError` のまま reject され、引数のデシリアライズ失敗は文字列で reject される(フロントでは `internal` になる)。`()` の応答は `null`
  - `pause` は常に `Ok(())`。`confirm_exit` は `app.exit(0)` で、応答は事実上返らない
- **Implications**: ブリッジは要求の `payload.args` を各要求 DTO へ serde でデシリアライズし、失敗は文字列のエラーで返す。`ApiError` は値のまま返す

### ファイル操作と文書状態の再現
- **Context**: スモーク 10 と 11 をダイアログ以外で自動化するため
- **Sources Consulted**: `src-tauri/src/commands/files.rs`、`app_state.rs`、`window.rs`、`crates/brainfuck-gui-api/src/source_file.rs`、`limits.rs`
- **Findings**:
  - 開く: ダイアログ → `metadata` で 1 MiB 超を中身を読まずに拒否(`fileTooLarge`)→ `decode_source`(置換と改行の記録)→ 文書状態を `opened`
  - 保存: 文書状態の読み取り → `encode_source`(ダイアログより前に変換を検査)→ 紐づけ先が無いか別名なら保存ダイアログ → `std::fs::write` → `saved`。I/O の失敗は `ApiError::io`(パスを含まない)
  - 閉じる: `dirty` が真のときだけ `close-requested` を送り、偽なら確認なしで閉じる
  - `DocumentState` と `read_source` は `src-tauri`(独立ワークスペース、tauri 依存)にあり、`brainfuck-gui-api` からは使えない
- **Implications**: ブリッジは文書状態と読み書きの順序を複製する(数十行)。乖離の検出のため、`files.rs`・`app_state.rs`・`window.rs` の変更を Revalidation Trigger とし、ブリッジのテストで同じ順序(変換の検査がダイアログより前、1 MiB 超は読まずに拒否など)を固定する。`DebuggerService` と `source_file` の関数は再実装せずに呼ぶ

### ダイアログの代替の方式
- **Context**: スモーク 10.5(保存先の選択中に編集を続ける)は、選択の完了を遅らせる必要がある
- **Findings**:
  - Node が要求の送信時にダイアログの結果を添える方式では、保存でダイアログが要るかどうか(紐づけ先の有無)を Node が知らず、選択を遅らせることもできない
  - ブリッジが処理の途中で「ダイアログの結果をください」と Node へ問い合わせ、Node の返事を待つ方式なら、実アプリの `blocking_save_file` と同じ位置で処理が止まり、テストが返事を遅らせられる
- **Implications**: 双方向の行プロトコル(要求・応答に加えて、ダイアログの問い合わせと返事)を採用する

### ブラウザの既定動作(リロード・右クリック)の検証
- **Context**: スモーク 11.6・11.7
- **Findings**:
  - headless の Chromium では、F5・Ctrl+R などのブラウザのショートカットはブラウザ本体の処理で、テストが送るキー入力ではもともとリロードしない可能性が高い。「リロードされなかった」だけでは抑止の証明にならない
  - アプリはウィンドウの `keydown` / `contextmenu` で `preventDefault` するため、アプリより後に登録したリスナーで `defaultPrevented` を読めば、抑止をブラウザ上で直接確かめられる
  - Playwright の `keyboard.down` は、同じキーを離さずに繰り返すと `repeat: true` のイベントを送るため、押しっぱなし(自動リピート)を再現できる
- **Implications**: `defaultPrevented` を主な検証にし、加えてナビゲーションが起きていないこと(ページ内の目印と `framenavigated` の回数)を確認する。実 WebView2 での実際のリロードとメニューは `desktop-gui-e2e-webview` に残す

### 見た目の検証(配色・強制配色・フォーカス枠)
- **Sources Consulted**: `src/styles/global.css`(`color-scheme: light dark`、`@media (prefers-color-scheme: dark)`、全体の `:focus-visible`、波線は `text-decoration: underline wavy currentColor`)、`SourceEditor.css`(エディタの枠は `.cm-editor:has(.cm-content:focus-visible)`)、Playwright の `page.emulateMedia({ colorScheme, forcedColors })`
- **Findings**:
  - 製品には `forced-colors` 用の CSS が無い。強制配色で枠が消えるなどの不具合が見つかる可能性がある
  - 2026-11-07 の「エディタのフォーカス枠が見えない」は、算出スタイル上は `outline` があっても、重なりや切り抜きで描画されない種類の不具合だった。算出スタイルだけでは検出できない
- **Implications**: フォーカス枠は「フォーカス前後で、枠の帯の画素が変わること」で検証する(帯のスクリーンショットのバイト列の比較。PNG のデコードは不要)。コントラストは算出色から WCAG の式で計算する

### CI と実行環境
- **Findings**:
  - リポジトリに `.github/workflows/` は無い。基本 CI は `ci-baseline`(直接実装の候補)が担う
  - `brainfuck-gui-api` は Tauri に依存しないため、Linux でのブリッジのビルドに WebKitGTK などは要らない
  - 日本語のフォントが無い Linux では、ボタンの文字幅が実機と変わり、折り返しの検証が実態とずれる
- **Implications**: 本 spec は独立したワークフローファイルに自分の job だけを置く。CI では日本語フォント(`fonts-noto-cjk`)を入れる

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| A-1 実 Rust の橋渡し(採用) | テスト専用 stdio バイナリが `DebuggerService` と `source_file` を呼ぶ | 実際の意味論・DTO・エラー。Linux で動く | Tauri 層の数十行(文書状態・ファイルの順序)の複製 | Revalidation Trigger で乖離を管理 |
| A-2 TS での再実装 | 偽バックエンドを実行可能にする | 依存が少ない | debugger-state 相当の重複。乖離すると偽の合格 | brief で却下 |
| A-3 台本のみ | `fakeBackend` の拡張 | 簡単 | Run・Step・Pause の流れを扱えない | brief で却下 |
| 中継: HTTP サーバ | Node に HTTP 中継を置き `fetch` する | ブラウザ外からも叩ける | ワーカーごとのポート、CORS、プリフライト | 不採用 |
| 中継: `exposeFunction`(採用) | Playwright の関数公開で Node へ転送 | ポート不要、テストごとに独立 | Playwright の外では使えない | webview spec は実 Tauri を使うため不要 |

## Design Decisions

### Decision: IPC の差し込みは初期化スクリプトで行い、製品のソースを変えない
- **Context**: 要件 3.1〜3.3
- **Alternatives Considered**:
  1. `VITE_E2E` で切る製品側の入口(`main.tsx` の分岐)
  2. `mockIPC` を含む IIFE バンドルを `page.addInitScript` で注入
- **Selected Approach**: 2。`e2e/support/ipcShim/` を Vite のライブラリモード(IIFE)で `e2e/.cache/ipc-shim.js` へビルドし、フィクスチャがファイルの内容を 1 つの初期化スクリプトとして登録する
- **Rationale**: 製品のソースの変更がゼロで、配布と同じ `npm run build` の成果物をそのまま検証できる。配布物にテストの仕組みが入らないことが構造上保証される
- **Trade-offs**: シムのビルド手順が 1 つ増える。Playwright 以外(手動のブラウザ)からは差し込めない
- **Follow-up**: シムが使う `@tauri-apps/api` はアプリの依存と同じ版(lock ファイルで一致)

### Decision: 中継は Playwright の `exposeFunction` で行い、HTTP サーバを作らない
- **Context**: 要件 1.5、1.6、2.2
- **Selected Approach**: テストごとにブリッジのプロセスを起動し、そのページに `exposeFunction` で転送関数を公開する
- **Rationale**: ポートの割り当て(ワーカーごと)と CORS が不要になり、テストごとにバックエンドの状態が独立する
- **Trade-offs**: brief の「HTTP 中継」から変わる。中継の責務(転送・記録・ダイアログの代替)は同じで、経路だけが違う

### Decision: ブリッジは双方向の JSON Lines プロトコルで、要求ごとにスレッドを立てる
- **Context**: 要件 2.1〜2.6
- **Selected Approach**: 要求・応答に `id` を付け、ダイアログが必要になった処理はブリッジから問い合わせを送って返事を待つ。標準出力は行単位でロックして書く
- **Rationale**: Pause の経路(ロックを取らない)と、ダイアログの位置(変換の検査の後)が実アプリと同じになる

### Decision: ブリッジは独立した crate `brainfuck-e2e-bridge` として置く(改訂)
- **Context**: 要件 3.4、テスト専用で配布しない。spec 横断のレビューで、当初の `brainfuck-gui-api` の example では後続の spec と衝突することが分かった
- **Alternatives Considered**:
  1. `brainfuck-gui-api` の `src/bin/`(ライブラリの利用者に `serde_json` を本番依存として強いる)
  2. `brainfuck-gui-api` の example(当初の案)。compiler-comparison は `get_comparison_info` / `run_comparison` を新しい crate `brainfuck-compare-api`(`brainfuck-gui-api` に依存する)に置き、フロントからは文字列で呼ぶ(フロントの `COMMANDS` に足さない)。gui-api の example は依存の向きの都合で compare-api に依存できず、ブリッジでこれらを扱えない。また、ブリッジとフロントの `COMMANDS` を比べる契約テストでは、これらの追加を検出できない
  3. 独立した crate(採用)
- **Selected Approach**: `crates/brainfuck-e2e-bridge/`(`publish = false`、バイナリターゲット、`[lints] workspace = true`)。ルートの `members = ["crates/*"]` に自動で入る。依存は `brainfuck-gui-api`(path)、`serde`・`serde_json`、dev の `tempfile` だけ(いずれもワークスペースで既存。新しい外部 crate は無い)。単体テストは `cargo test --workspace` で走る。コマンドの契約テストの基準は、登録の唯一の情報源である `src-tauri` の `names.rs` の `COMMAND_NAMES` にし、ブリッジが扱わないコマンドは理由つきの除外一覧に明示する
- **Rationale**: `brainfuck-gui-api` の `Cargo.toml`・依存・API を一切変えない。後続の spec のコマンドを扱う必要が出たときは、本 crate から `brainfuck-compare-api` への依存を足せる(逆向きにならない)。`names.rs` に足されたコマンドは、ブリッジが対応するか除外するまで契約テストが失敗するため、見落とさない
- **Trade-offs**: ワークスペースのメンバーが 1 つ増える。steering の依存方針(外部依存は GUI 系と WASM 系のみ)では、本 crate を GUI のテスト用ツールとして GUI 系に数え、外部依存を既存の 3 つに限る

### Decision: E2E の型検査は別の tsconfig で行い、`build` は製品だけを型検査する
- **Context**: 要件 1.9、3.5。`tsconfig.json` の `include` に `e2e` を足すと、`build`(`npm run typecheck && vite build`)と、それを `beforeBuildCommand` に持つ `tauri build` が、E2E のコードの型の誤りで失敗する
- **Selected Approach**: 新規の `tsconfig.e2e.json`(`tsconfig.json` を extends、`include` は `e2e`・`playwright.config.ts`・`src/ipc/backend.ts`)。`typecheck` は両方の設定で型検査し、新設の `typecheck:app`(製品のみ)を `build` から呼ぶ。`tsconfig.json` は変えない
- **Rationale**: 開発者と CI の `npm run typecheck` は E2E の型の誤りを検出し、製品のビルドとインストーラは E2E の状態に左右されない。`build` の振る舞い(製品の型検査 + `vite build`)は従来と同じ
- **Trade-offs**: 既存のスクリプトのうち `typecheck` と `build` の 2 つを書き換える(design の Modified Files に明記)

### Decision: ブリッジが複製した Tauri 層の区間を指紋で固定する
- **Context**: ブリッジは `DocumentState`(`app_state.rs`)、開く・保存の順序(`files.rs`)、閉じる判定(`window.rs`)を複製する。Revalidation Trigger だけでは、Tauri 層の変更時に複製の見直しが漏れても検出できない
- **Selected Approach**: Vitest の単体テストで、複製元の関数・構造体の区間の本文を空白を正規化して SHA-256 にし、テストに固定した値と比べる。違えば、変わった区間・複製先・更新の手順を示して失敗する
- **Rationale**: Tauri 層の変更が、ブリッジの見直しを経ずに取り込まれることを防ぐ。`src-tauri` の依存(tauri)を Node 側のテストに持ち込まずに済む
- **Trade-offs**: 複製と無関係な書き換え(コメントの変更など)でも失敗する。その場合は指紋を更新するだけで済む

### Decision: 時間計測は Playwright のプロジェクトを分ける
- **Context**: 要件 10.2〜10.4、11.2
- **Selected Approach**: `@perf` タグのテストだけを含む `perf` プロジェクトと、それを除く `chromium` プロジェクトを設定する。閾値は要件の 1 秒の 1.5 倍(1,500 ms)とし、実測値を注釈と添付に残す

### Generalization / Build vs Adopt / Simplification
- **Generalization**: 「ダイアログの代替」「閉じる要求の模擬」「終了の記録」は、いずれも Tauri 層にしか無い副作用の代替で、ブリッジの `e2e:` 制御コマンドと Node のフィクスチャの 1 つの窓口(`backend`)にまとめる
- **Adopt**: IPC の模擬は `@tauri-apps/api/mocks`、ブラウザ操作と並列実行・レポート・トレースは `@playwright/test`、コントラストは WCAG 2.x の相対輝度の式(自前の小さな関数。ライブラリは使わない)
- **Simplification**: HTTP 中継、ブリッジの状態リセット用コマンド(テストごとにプロセスを作り直すため不要)、スクリーンショットの画素比較ライブラリ(帯のバイト列の比較で足りる)を設けない

## Risks & Mitigations
- Tauri 層とブリッジの乖離 — Revalidation Trigger に `files.rs`・`app_state.rs`・`window.rs`・`debugger.rs`・`names.rs`・`commands/mod.rs` を挙げ、ブリッジのコマンド一覧と `names.rs` の `COMMAND_NAMES`(除外一覧つき)の一致と、複製元の区間の指紋を単体テストで固定する
- 新しい実行ファイルがウイルス対策ソフトに止められる(開発 PC) — ブリッジの起動確認(`e2e:ping`)に待ち時間の上限を設け、超えたらウイルス対策ソフトの除外を案内するメッセージで失敗させる。README に対処を書く。`CARGO_TARGET_DIR` を尊重して新しいビルドディレクトリを使えるようにする
- 時間計測の揺らぎ — `perf` プロジェクトに分け、CI では失敗を表示するが必須にしない
- E2E が製品の不具合を見つける(例: 強制配色で枠が消える) — 製品は直さず、該当テストを `test.fixme` にして理由と追跡先を注記し、README と traceability に記録する
- Linux と Windows のフォント差による折り返しの違い — CI に日本語フォントを入れ、検証は「折り返しが 2 行以上」「はみ出しが無い」の性質で行い、画素位置を固定しない

## References
- `apps/desktop/src/ipc/backend.ts`、`src/ipc/errors.ts`、`src/app/shortcuts.ts`、`src/app/documentFlow.ts` — IPC 契約と抑止の実装
- `apps/desktop/src-tauri/src/commands/{debugger,files}.rs`、`app_state.rs`、`window.rs` — ブリッジが再現する対応
- `crates/brainfuck-gui-api/src/{service,source_file,limits,dto}.rs` — ブリッジが呼ぶ API
- `apps/desktop/README.md` — 手動スモーク手順 1〜15 と検証結果の記録
- Playwright: `page.addInitScript`、`page.exposeFunction`、`page.emulateMedia`、`keyboard.down`、`testProject.grep` / `grepInvert`、`webServer`
- `@tauri-apps/api/mocks`: `mockIPC(cb, { shouldMockEvents })`、`clearMocks`
- WCAG 2.2 の相対輝度とコントラスト比の定義(達成基準 1.4.3)
