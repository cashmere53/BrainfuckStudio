# Research & Design Decisions

## Summary
- **Feature**: `desktop-gui`
- **Discovery Scope**: New Feature(greenfield の UI アプリ)+ 上流契約の統合(`debugger-state` / `brainfuck-core` / `brainfuck-cli`)。完全探索(full discovery)を実施
- **Key Findings**:
  - 上流の型(`DebugSession` / `BatchReport` / `StateView` / `SourceMap` 等)はシリアライズ非依存の値型で、`DebugSession` は `Send`。Tauri 側で JSON DTO を定義し、`Mutex` 内にセッションを保持できる。キャンセルは `AtomicBool`(`CancelSignal` 実装済み)
  - CodeMirror の位置は **UTF-16 オフセット**。`debugger-state` の改版で `SourceMap::instruction_at_utf16` / `instruction_at_or_after_utf16` が提供されたため、GUI 側は UTF-16 始点表や二分探索を持たず、これらを直接呼ぶ(改版前に検討していた自前の `InstructionTable` は廃止)
  - `Program::parse` が失敗すると `SourceMap` は作れない(`SourceMap::new` は `Program` が必要)。解析エラーの行・列・UTF-16 は、`debugger-state` の改版で追加された `Program` 不要の `SourceMap::position_at(source, byte_offset)` で得る(GUI 側で数え方を実装しない)。テープ窓も同改版の `tape_window_of` / `DebugSession::tape_window` に一本化されている
  - `RuntimeError::StepLimitExceeded`(致命)と `StopReason::StepLimitReached`(バッチ到達・継続)は別物。DTO で別ラベルにする
  - 1 回のバッチを小さく保つと IPC 往復が多くなる(既定上限 10,000,000 命令 ÷ 1,000 = 10,000 往復)。キャンセルは命令ごとに確認されるため、バッチ幅を大きくしても Pause 遅延は増えない。フロントのバッチ幅を時間ベースで適応させる

## Research Log

### Tauri 2 のコマンド・状態・権限モデル
- **Context**: 非同期コマンドで `DebugSession` を保持し、Web コンテンツへ最小権限だけを与える方法
- **Sources Consulted**: Tauri v2 ドキュメント(State Management / Calling Rust / Capabilities)、`tauri-plugin-dialog` の README とソース(context7 経由)
- **Findings**:
  - `#[tauri::command] async fn` は、`State<'_, T>` など借用引数を取る場合 `Result<_, _>` を返す必要がある。エラー型は `serde::Serialize` を実装すればフロントへ構造化して渡せる
  - 登録した全コマンドは既定で全ウィンドウから呼べる。`tauri_build::AppManifest::new().commands(&[...])`(`build.rs`)でコマンドごとの権限(`allow-<command>`)を生成し、`capabilities/*.json` で明示的に許可する構成にできる
  - ダイアログプラグインは Rust 側から `app.dialog().file().blocking_pick_file()` / `blocking_save_file()`(`Option<FilePath>`)を呼べる。**Rust 側だけで使えば WebView にダイアログ/ファイル権限を与えずに済む**(パスは WebView に出ない)
  - `std::sync::Mutex` は `await` をまたいで保持しない限り使ってよい(コマンド内で同期的にロック→解放)
- **Implications**: ファイルの開く/保存はすべて Rust コマンド(`open_source_file` / `save_source_file`)として実装し、現在パスを Rust の状態に保持する。WebView の権限は `core:event:default` と自アプリのコマンド許可のみ。ウィンドウを閉じる確認は、Rust 側の `CloseRequested` フックと「未保存フラグ」の同期で実現し、`core:window:*` の破棄権限を渡さない

### TypeScript 型の機械生成
- **Context**: 要件 12.13(DTO 定義からフロント型を機械的に得る)
- **Sources Consulted**: `ts-rs` README / wiki(context7)。`#[ts(export)]`、`export_to`、`TS_RS_EXPORT_DIR`、`TS_RS_LARGE_INT`、serde の `tag` / `rename_all` 互換
- **Findings**: `ts-rs` は serde 属性(`rename_all = "camelCase"`、内部タグ付き enum)と互換。`u64` は既定で `bigint` になるため、フィールドごとの `#[ts(type = "number")]` か `TS_RS_LARGE_INT=number` が必要。エクスポート先と手段(環境変数/`Config`)はバージョンで差があり、実装時に採用バージョンの API を確認する
- **Implications**: DTO の `u64` は上限を 2^53−1 に制限して `number` で表現する(命令数上限の入力も同値で拒否)。生成物 `apps/desktop/src/generated/*.ts` をコミットし、Rust テストが「新規生成結果 = コミット済みファイル」を検査する。`tauri-specta` は採用しない(v2 対応が RC 段階であり、コマンド型の生成は薄い手書きラッパで足りる。`research` の決定参照)

### CodeMirror 6 の拡張点
- **Context**: 現在命令・ブレークポイント・括弧対応・エラー位置の表示
- **Findings**:
  - 現在命令/エラー位置は `StateField<DecorationSet>` + `StateEffect` で更新する mark decoration(`Decoration.mark`)。位置は UTF-16 コード単位で、`SourceMap` の `utf16_offset` と同一
  - ブレークポイントは `StateField<RangeSet<…>>` と `gutter({ lineMarker })`。`RangeSet.map(changes)` により編集へ自動追従する(対象文字を含む範囲が削除されると範囲が潰れる)
  - 括弧対応は `@codemirror/language` の `bracketMatching()` が言語指定なしでも平文の括弧走査にフォールバックし、対応なしを `cm-nonmatchingBracket` で示す。既定の走査上限(10,000 文字)は `maxScanDistance` で引き上げる。Brainfuck では `[` `]` がコメント内にあっても常に命令であるため、平文走査が意味論に一致する
  - 改行は CodeMirror の既定(`\n` / `\r\n` / `\r`)が `SourceMap` の規則と一致
- **Implications**: 括弧対応のためだけの IPC は不要。ただし走査結果が `jump_table` と一致することを、フロントのテストで固定コーパスに対して検証する

### 実行ループの置き場所と Pause の競合
- **Context**: 仕様書 §8 は「Rust 側でバッチ実行し、バッチ末尾・停止・ブレークポイント・エラー時に状態を通知」と定める
- **Findings**: コマンド `run_batch` が 1 バッチを実行して状態を返し、フロントの `RunController` が `StepLimitReached` の間ループする方式なら、キャンセルは別コマンド `pause`(`Arc<AtomicBool>` への `store` のみでセッションのロックを取らない)で届く。バックエンド主導のループ(スレッド + Channel)は、ロック・イベント順序・停止後の状態取得が複雑になる
- **Implications**: フロント主導ループを採用。キャンセルフラグは `run_batch` の入口でクリアし、`pause` がバッチとバッチの間に届いた場合はフロントの局所フラグで次のバッチを発行しない(両方で「Pause を取りこぼさない」を保証)

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| A. Tauri クレート内に DTO・サービスを同居 | `apps/desktop/src-tauri` に全 Rust コード | クレート数が少ない | `tauri::generate_context!` が `frontendDist` の存在を要求し、`cargo test --workspace` が未ビルドのフロントで失敗しうる。CLI 一致テストが Tauri に依存 | 却下 |
| B. Tauri 非依存の `brainfuck-gui-api` クレート + 薄い Tauri 層 | DTO・サービス・変換・CLI 一致テストを `crates/brainfuck-gui-api` に置く。`src-tauri` はコマンドの薄いラッパとファイル/ダイアログ | `cargo test --workspace`(既存の完了基準)が Web ビュー・ビルド成果物なしで成功。後続 spec が DTO/ヘルパを再利用できる | クレートが 1 つ増える | 採用 |
| C. バックエンド主導の実行ループ(スレッド + イベント) | Rust が実行し続け、イベントで通知 | IPC 往復が少ない | 競合・順序・停止後取得の複雑さ。テスト容易性が低い | 却下 |

## Design Decisions

### Decision: Tauri 非依存の `brainfuck-gui-api` クレートを追加する
- **Context**: `cargo test --workspace` を未ビルドのフロント/Web ビュー無しで成功させ、CLI 一致を自動検証したい(要件 11.8, 15.1, 15.4, 15.5)
- **Alternatives Considered**: (A) 単一 Tauri クレート、(B) 別クレート
- **Selected Approach**: B。`crates/brainfuck-gui-api`(core + debugger-state にのみ依存、serde + ts-rs)。`apps/desktop/src-tauri` は独立ワークスペース(自身の `[workspace]`)で、`path` 依存で `brainfuck-gui-api` を利用。ルート `Cargo.toml`(`members = ["crates/*"]`)は変更不要
- **Rationale**: 構造ステアリング(`crates/<name>/` = ライブラリ、`apps/desktop/` = Tauri アプリ)に整合し、UI 境界が薄く保てる
- **Trade-offs**: クレート 1 つ増。`src-tauri` のテストは `cargo test --manifest-path` で別実行(ただしロジックは API クレート側にあるため薄い)
- **Follow-up**: 構造ステアリングの Rust crates 一覧に追記候補(本 spec では steering を変更しない)

### Decision: 実行ループはフロント主導、`run_batch` は 1 バッチ
- **Context**: 6 コマンドの名称・意味が仕様書 §9 で固定。`StepLimitReached` は継続の合図
- **Selected Approach**: `RunController` が `run_batch` を反復。バッチ幅は既定 1,000 から開始し、往復が速ければ倍々で最大 1,000,000 まで増やす(時間ベースの適応)。表示は `requestAnimationFrame` で間引く
- **Rationale**: キャンセルは命令ごとに確認されるので Pause 遅延はバッチ幅に依存しない。往復回数を減らし 10,000,000 命令の実行を現実的な時間に収める
- **Trade-offs**: ブレークポイント停止の粒度には影響しない(バッチ幅より前に照合される)。バッチ幅が大きいと表示更新の間隔が伸びるが、時間ベース(目標 約 20〜50 ms)で制御する
- **Follow-up**: 実装時に往復時間を計測し目標値を調整

### Decision: ブレークポイントはフロントがソース位置(UTF-16)で保持し、バックエンドで正規化して反映
- **Context**: 要件 6.1〜6.12、解析不能な編集途中でも設定できるべき
- **Selected Approach**: エディタの `StateField` が「命令文字の位置」を保持(編集に追従)。トグル時はフロントが「行内の最初の命令」「カーソル以降の最初の命令」へスナップ(命令文字集合 `><+-.,[]` の走査)。セッション開始時と変更時に `set_breakpoints`(UTF-16 オフセットの配列)でバックエンドへ送り、バックエンドは`SourceMap::instruction_at_or_after_utf16` で命令添字へ正規化して `BreakpointSet` を置換、解決済みの位置を返す
- **Rationale**: 編集追従を CodeMirror の仕組みに任せ、実行側の照合は `debugger-state` の `BreakpointSet`(命令添字)に任せる
- **Trade-offs**: 命令文字集合(8 文字)をフロントが持つ。バックエンドの正規化(`instruction_at_or_after_utf16`)で冪等になるため、不一致は返却位置で自動修正される

### Decision: 出力はフロントが末尾 64 KiB だけ保持し、差分 + 総バイト数で受け取る
- **Context**: 要件 9.8、12.9。出力は最大 10,000,000 バイト以上になりうる
- **Selected Approach**: 要求に `outputFrom`(フロントが把握している総バイト数)を含め、応答は `max(outputFrom, total − 65,536)` 以降のバイトを Base64 で返す。応答の `dataStart` が保持済み末尾と連続しない場合はバッファを置換する
- **Rationale**: 応答サイズが常に 64 KiB 以下で、UTF-8 の文字またがりも連結後に解釈できる
- **Trade-offs**: 省略された先頭は表示できない(要件で許容)

### Decision: 解析エラーの位置は上流の `SourceMap::position_at` で得る
- **Context**: `ParseError` はバイトオフセットのみ。`SourceMap` の順引きは成功した `Program` が必要
- **Selected Approach**: `brainfuck-gui-api::convert` が `SourceMap::position_at(source, byte_offset)` を呼び、結果を `SourcePositionDto` に写すだけにする。`position_at` と `range_of` の一貫性テストは core 側(`debugger-state` 要件 9.14)に置き、GUI のテストは DTO 写像のみ
- **Trade-offs**: 当初は規則が 2 か所に存在する案だったが、クロス spec レビューで上流に一本化した。上流の `position_at` の仕様変更は Revalidation Trigger

### Decision: 起動時サンプルは `examples/hello.bf` を単一の情報源としてビルド時に取り込む
- **Selected Approach**: フロントのビルドが `../../examples/hello.bf?raw` を取り込む(Vite の `server.fs.allow` にリポジトリルートを追加)。複製ファイルを作らない
- **Rationale**: CLI と GUI が同一のサンプルを使い、要件 11.3 の手動確認と一致する

### Decision: WebView 権限を最小化し、ダイアログ/ファイルは Rust 内に閉じる
- **Selected Approach**: `build.rs` の `AppManifest::commands` で全アプリコマンドを列挙、`capabilities/default.json` は `core:event:default` とそれらの `allow-*` のみ。CSP は `default-src 'self'`、`style-src 'self' 'unsafe-inline'`(CodeMirror のスタイル注入のため)、`script-src 'self'`
- **Follow-up**: 実装時に Tauri のバージョンでの権限識別子形式(`allow-<command>` 形式)を `tauri build` の生成スキーマで確認

### Decision: UI 文言は日本語、コアの診断(英語固定文面)は表示に使わず DTO のフィールドから組み立てる
- **Rationale**: 要件 1.5。フィールド(`pc`・`dp`・方向・上限)は CLI の診断と同一の値であり、文面だけが異なる(一致の基準は値)

## Generalization / Simplification の結果
- **Generalization**: 「ソース + 入力 + 上限」の組を共有する `Workspace`(フロントの単一ストア)と、DTO の共通部品(`ParseErrorDto` / `RuntimeErrorDto` / `SourceRangeDto` / `OutputChunkDto` / `ApiError` / Base64 コーデック)を切り出す。`compiler-comparison` が同じ部品で別画面・別コマンドを追加できる(インターフェースのみ一般化し、実装は現要件分のみ)
- **Build vs Adopt**: エディタ(CodeMirror 6 標準拡張: `bracketMatching` / `gutter` / `Decoration`)、型生成(`ts-rs`)、ダイアログ(`tauri-plugin-dialog`)、テスト(Vitest + Testing Library)を採用。状態管理ライブラリ(Redux 等)は採用せず `useReducer` + 小さな外部ストア(`useSyncExternalStore`)。ルーターは採用せず、画面レジストリ(配列)のみ
- **Simplification**: バックエンド主導ループ、イベントストリーム、`tauri-specta`、設定の永続化、複数セッションは採用しない

## Risks & Mitigations
- IPC 往復が遅い環境で 10,000,000 命令の実行が遅い — 適応バッチ幅(最大 1,000,000)。性能は受け入れ基準に含めず、応答性(Pause 1 秒以内、更新 1 秒以内)のみ
- ts-rs のバージョン差(`u64`、エクスポート API) — 採用バージョンを固定し、`#[ts(type = "number")]` を明示、生成物一致テストで検出
- 平文括弧走査と `jump_table` の不一致 — 固定コーパスでの一致テスト、`maxScanDistance` を最大ソースサイズに設定
- 位置規則の二重化 — 上流の `SourceMap::position_at` / `instruction_at_or_after_utf16` / `tape_window_of` を呼ぶだけにして排除(一貫性テストは core 側)
- `src-tauri` を別ワークスペースにすることで `Cargo.lock` が 2 つになる — 受容(Tauri アプリの標準的な構成)。`serde` / `ts-rs` はルートと同一バージョンへ固定し、`[lints.rust] unsafe_code = "forbid"` を src-tauri 側で自前宣言する(独立 workspace はルートの `[workspace.lints]` を継承できない。`forbid` が tauri のマクロ展開と衝突する場合のみ `deny`)
- `run_batch` が 1 バッチの間 `Mutex` を保持するため、`pause` はロック外の `AtomicU64 current_session_id` と照合してから旗を立てる(未知/古い `sessionId` は無視)
- ウィンドウ表示そのものの自動 E2E が無い — 手動スモーク手順を README に文書化(要件 15.6)。フロントはモックバックエンドでコンポーネントテストする

## References
- [Tauri v2 State Management](https://v2.tauri.app/develop/state-management/) — `State` と `Mutex`
- [Tauri v2 Calling Rust](https://v2.tauri.app/develop/calling-rust/) — 非同期コマンドとエラー
- [Tauri v2 Capabilities](https://v2.tauri.app/security/capabilities/) — `AppManifest::commands`
- [tauri-plugin-dialog](https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/dialog) — Rust 側のファイルダイアログ
- [ts-rs](https://github.com/Aleph-Alpha/ts-rs) — Rust 型からの TypeScript 生成
- [CodeMirror 6 Reference](https://codemirror.net/docs/ref/) — Decoration / RangeSet / bracketMatching / gutter
- `.kiro/specs/debugger-state/design.md` — `DebugSession` / `BatchReport` / `SourceMap` の契約
- `.kiro/specs/brainfuck-cli/design.md` — 結果一致の基準(`state: pc=106 dp=6 steps=906`)
