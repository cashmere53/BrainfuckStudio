//! フロントとバックエンドの JSON 契約(DTO)。
//! 上流(`brainfuck-core` / `debugger-state`)の値から DTO への写像のみを行う。
//!
//! 規則: JSON は camelCase、判別付き列挙は `kind` タグ、`u64` は TypeScript で `number`
//! (値は JSON の安全整数範囲 2^53−1 以下に制限する。`limits` を参照)。
//! TypeScript 型は `export_bindings` で `apps/desktop/src/generated/` へ書き出す。
//!
//! 要求 DTO は未知のキーを拒否する(`deny_unknown_fields`)。応答 DTO は未知のキーに寛容
//! (応答の `Deserialize` は往復テスト用であり、フロントから届く値ではないため)。

use serde::{Deserialize, Serialize};
use ts_rs::{Config, ExportError, TS};

/// ソース上の 1 点の位置。行・列は 1 始まり、列はスカラ値個数 + 1、`utf16Offset` はコード単位数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourcePositionDto {
    /// ソース先頭からの UTF-8 バイトオフセット。
    pub byte_offset: usize,
    /// ドキュメント先頭からの UTF-16 コード単位数。
    pub utf16_offset: usize,
    /// 行番号(1 始まり)。
    pub line: usize,
    /// 行頭からの列(スカラ値個数 + 1、1 始まり)。
    pub column: usize,
}

/// ソース上の範囲(半開区間 `[start, end)`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceRangeDto {
    /// 範囲の先頭位置。
    pub start: SourcePositionDto,
    /// 範囲の終点位置(半開区間の終点)。
    pub end: SourcePositionDto,
}

/// 解析エラーの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ParseErrorKind {
    /// 対応する `[` を持たない `]`。
    UnmatchedClosingBracket,
    /// 対応する `]` を持たない `[`。
    UnmatchedOpeningBracket,
}

/// 解析エラー(括弧の不整合)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ParseErrorDto {
    /// エラーの種類。
    pub kind: ParseErrorKind,
    /// 違反した括弧の命令添字。
    pub instruction_index: usize,
    /// 括弧文字の位置。範囲は `[utf16Offset, utf16Offset + 1)`。
    pub position: SourcePositionDto,
}

/// データポインタの移動方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PointerDirectionDto {
    /// 左(`<`)への移動。
    Left,
    /// 右(`>`)への移動。
    Right,
}

/// 実行時エラー。状態は直前の値を保持する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeErrorDto {
    /// データポインタが範囲外へ移動しようとした。
    PointerOutOfBounds {
        /// 発生時の命令ポインタ。
        pc: usize,
        /// 発生時のデータポインタ。
        dp: usize,
        /// 範囲外へ移動しようとした方向。
        direction: PointerDirectionDto,
    },
    /// 命令数上限に達し、未停止のまま次の命令を実行できない。
    StepLimitExceeded {
        /// 設定された命令数の上限(2^53−1 以下)。
        #[ts(type = "number")]
        limit: u64,
        /// 上限到達時の命令ポインタ。
        pc: usize,
    },
}

/// 出力チャンク。`dataStart` から `totalLen` までの末尾部分を Base64 で運ぶ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutputChunkDto {
    /// `dataBase64` の先頭が出力全体のどのバイト位置か。
    pub data_start: usize,
    /// 出力全体の総バイト数。
    pub total_len: usize,
    /// `dataStart` から末尾までの出力バイト列(標準 Base64、パディングあり)。
    pub data_base64: String,
}

/// API エラーの種類(固定集合)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ApiErrorKind {
    /// 要求の値が不正。
    InvalidArgument,
    /// 指定のセッションが存在しない。
    SessionNotFound,
    /// 指定のプログラムが読み込まれていない。
    ProgramNotLoaded,
    /// 古いプログラム ID への要求。
    StaleProgram,
    /// ソースが上限を超えている。
    SourceTooLarge,
    /// 入力が上限を超えている。
    InputTooLarge,
    /// ファイルが上限を超えている。
    FileTooLarge,
    /// ファイル入出力の失敗。
    Io,
    /// 想定外の内部エラー。
    Internal,
}

/// API エラー。`message` は種類から決まる固定の英語文面で、パス・ソース・入力などの機微情報を含まない。
/// 唯一の例外は読み書きの失敗(`kind: io`)で、[`ApiError::io`] が作る文面は
/// `failed to <read|write> file: <OS のエラー文>` である(要求 2.11。OS 文のみでパスを含まない)。
///
/// 生成は [`ApiError::new`] または [`ApiError::io`] を使う。`message` / `field` は
/// 公開フィールドのまま(応答の形と Deserialize のため)。サービス層は `ApiError { .. }` の
/// リテラルで任意の文面を作らないこと(`tests/session_dto_contract.rs` で文面の形を検査する)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// エラーの種類。
    pub kind: ApiErrorKind,
    /// 固定の英語文面(`io` のみ末尾に OS のエラー文を含む)。
    pub message: String,
    /// 不正だった要求の項目名(該当しない場合は `null`)。
    pub field: Option<String>,
}

/// ファイル入出力の操作の種類(`io` エラーの文面に使う)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoOperation {
    /// 読み込み。
    Read,
    /// 書き込み。
    Write,
}

impl ApiError {
    /// 種類から固定文面を決めてエラーを作る。`field` は要求の項目名のみ(値は含めない)。
    ///
    /// `kind: io` で使うと固定文面「file input or output failed」になる。読み書きの失敗は
    /// [`ApiError::io`] を使うこと。
    pub fn new(kind: ApiErrorKind, field: Option<&str>) -> Self {
        Self {
            kind,
            message: kind.fixed_message().to_owned(),
            field: field.map(str::to_owned),
        }
    }

    /// 読み書き失敗のエラーを作る。文面は `failed to read file: <OS 文>` / `failed to write file: <OS 文>`。
    ///
    /// `os_error` には OS が報告したエラー(標準ライブラリの `io::Error`)の `Display` だけを渡す。
    /// `io::Error` の `Display` は OS の文面のみで、パス・ソース・入力バイトを含まない。
    /// この関数はパスや内容を受け取らず、`field` は `None`。
    /// (型を `io::Error` に限定せず `Display` にしたのは、この crate で `std::io` を名指ししない
    /// という `design.md` の依存方針に合わせるため。呼び出し側が渡すのは OS 文のみとする。)
    pub fn io(operation: IoOperation, os_error: impl std::fmt::Display) -> Self {
        let verb = match operation {
            IoOperation::Read => "read",
            IoOperation::Write => "write",
        };
        Self {
            kind: ApiErrorKind::Io,
            message: format!("failed to {verb} file: {os_error}"),
            field: None,
        }
    }
}

impl ApiErrorKind {
    /// 種類ごとの固定の英語文面。入力・内容・パスを含めない。
    pub const fn fixed_message(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalid argument",
            Self::SessionNotFound => "session not found",
            Self::ProgramNotLoaded => "program not loaded",
            Self::StaleProgram => "stale program id",
            Self::SourceTooLarge => "source is too large",
            Self::InputTooLarge => "input is too large",
            Self::FileTooLarge => "file is too large",
            Self::Io => "file input or output failed",
            Self::Internal => "internal error",
        }
    }
}

// ---- 要求 DTO ----
//
// 要求の受理方針(フロントとの契約。`tests/session_dto_contract.rs` で検査する):
// - 未知のキーは拒否する(`deny_unknown_fields`)。
// - 必須項目(`Option` 以外)の欠落は拒否する。`Option` 項目は省略と `null` を同じ `None` として受理する。
// - `u64` は JSON の非負整数として受理する(小数・負数・文字列は拒否)。上限(1..=2^53−1、
//   バッチ幅 1..=1,000,000、半径 0..=64、ブレークポイント 10 万件 など)の検査はサービス層が
//   `invalidArgument`(項目名つき)で行う。DTO は形式のみを検査する。
// - `sessionId` / `programId` は発行された値のみがフロントから届く。範囲外・未知の値は
//   サービス層が `sessionNotFound` / `staleProgram` で拒否する。

/// 表示窓の要求(テープ窓の半径と出力の開始位置)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewRequestDto {
    /// テープ窓の半径(0..=64)。
    pub tape_radius: usize,
    /// 出力差分の開始位置(総バイト数より大きい場合は末尾側へ切り詰める)。
    pub output_from: usize,
}

/// 命令数上限の指定。`limited` の `maxSteps` は 1..=2^53−1。
///
/// JSON は `{"kind":"limited","maxSteps":n}` または `{"kind":"unlimited"}`。
/// 未知のキーは拒否する。serde は内部タグ enum の variant に `deny_unknown_fields` を効かせられず、
/// ts-rs も enum の `deny_unknown_fields` を解釈できない(警告になる)ため、`Deserialize` は手で実装する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StepLimitDto {
    /// 命令数に上限を設ける。
    Limited {
        /// 命令数の上限(1..=2^53−1)。
        #[ts(type = "number")]
        max_steps: u64,
    },
    /// 命令数に上限を設けない。
    Unlimited,
}

impl<'de> Deserialize<'de> for StepLimitDto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Raw {
            kind: RawKind,
            #[serde(default)]
            max_steps: Option<u64>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        enum RawKind {
            Limited,
            Unlimited,
        }

        let raw = Raw::deserialize(deserializer)?;
        match (raw.kind, raw.max_steps) {
            (RawKind::Limited, Some(max_steps)) => Ok(Self::Limited { max_steps }),
            (RawKind::Limited, None) => {
                Err(<D::Error as serde::de::Error>::missing_field("maxSteps"))
            }
            (RawKind::Unlimited, None) => Ok(Self::Unlimited),
            (RawKind::Unlimited, Some(_)) => Err(<D::Error as serde::de::Error>::custom(
                "unexpected field `maxSteps` for kind `unlimited`",
            )),
        }
    }
}

/// ソース読み込みの要求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoadSourceRequest {
    /// ソース全体(LF 正規化済み)。上限は 1 MiB(サービス層が検査)。
    pub source: String,
}

/// セッション開始の要求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartSessionRequest {
    /// 読み込み済みプログラムの ID。現在と異なれば `staleProgram`。
    #[ts(type = "number")]
    pub program_id: u64,
    /// 入力バイト列の標準 Base64(パディングあり)。デコード前に長さで上限を検査する。
    pub input_base64: String,
    /// 命令数上限の指定。
    pub limit: StepLimitDto,
    /// ブレークポイントの UTF-16 オフセット(最大 100,000 件。重複は 1 件とみなす)。
    pub breakpoint_offsets: Vec<usize>,
    /// 初期の表示窓。
    pub view: ViewRequestDto,
}

/// セッションに対する表示窓の要求(Step・リセット・現在状態・スナップショット)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionViewRequest {
    /// 対象セッションの ID。
    #[ts(type = "number")]
    pub session_id: u64,
    /// 表示窓。
    pub view: ViewRequestDto,
}

/// バッチ実行の要求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunBatchRequest {
    /// 対象セッションの ID。
    #[ts(type = "number")]
    pub session_id: u64,
    /// バッチ幅。省略または `null` は既定の 1,000。指定時は 1..=1,000,000。
    #[ts(type = "number | null")]
    pub batch_size: Option<u64>,
    /// 表示窓。
    pub view: ViewRequestDto,
}

/// テープ窓の再取得の要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TapeWindowRequest {
    /// 対象セッションの ID。
    #[ts(type = "number")]
    pub session_id: u64,
    /// テープ窓の半径(0..=64)。
    pub radius: usize,
}

/// ブレークポイント置換の要求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetBreakpointsRequest {
    /// 対象セッションの ID。
    #[ts(type = "number")]
    pub session_id: u64,
    /// 置換後のブレークポイントの UTF-16 オフセット(最大 100,000 件)。
    pub offsets: Vec<usize>,
}

/// 一時停止の要求。`sessionId` が現在のセッションと一致するときだけ効く(それ以外は無視)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PauseRequest {
    /// 一時停止の対象セッションの ID。
    #[ts(type = "number")]
    pub session_id: u64,
}

/// 保存の要求。ソース文字列と「別名で保存」の指定のみを運ぶ(パスは含まない)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveFileRequest {
    /// 保存するソース(LF 正規化済み。保存時に元の改行コードへ戻す)。
    pub source: String,
    /// `true` なら保存先を尋ねる(別名で保存)。
    pub save_as: bool,
}

/// 未保存フラグの要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetDocumentDirtyRequest {
    /// 未保存の変更があるかどうか。
    pub dirty: bool,
}

// ---- 応答 DTO ----

/// ソース読み込みの結果。解析エラーは `parseError`(プログラムは登録されない)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LoadSourceResult {
    /// 解析に成功した。
    Ok {
        /// 新しく採番したプログラム ID。
        #[ts(type = "number")]
        program_id: u64,
        /// 命令数。
        instruction_count: usize,
        /// 行数。
        line_count: usize,
    },
    /// 解析に失敗した(括弧の不整合)。
    ParseError {
        /// 解析エラー。
        error: ParseErrorDto,
    },
}

/// 現在の機械状態のスナップショット。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDto {
    /// 命令ポインタ。
    pub pc: usize,
    /// データポインタ。
    pub dp: usize,
    /// `dp` が指すセルの値。
    pub current_cell: u8,
    /// これまでに実行した命令数。
    #[ts(type = "number")]
    pub steps_executed: u64,
    /// 正常終了に到達しているか。
    pub halted: bool,
}

/// テープ窓。`start` は窓先頭セルのテープ上の添字、`pointerOffset` は窓内での `dp` の位置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TapeWindowDto {
    /// 窓先頭セルのテープ上の添字。
    pub start: usize,
    /// `start` から昇順の連続セル値。
    pub cells: Vec<u8>,
    /// 窓内での `dp` の位置。
    pub pointer_offset: usize,
}

/// 直近の停止理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum StopReasonDto {
    /// 正常終了した。
    Halted,
    /// ブレークポイントで停止した。
    Breakpoint,
    /// 一時停止(キャンセル旗)で停止した。
    Cancelled,
    /// バッチ幅に到達した。
    BatchReached,
}

/// セッションの状態(応答の共通部分)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionStateDto {
    /// セッション ID。
    #[ts(type = "number")]
    pub session_id: u64,
    /// スナップショット。
    pub snapshot: SnapshotDto,
    /// テープ窓。
    pub tape_window: TapeWindowDto,
    /// 出力の差分(末尾 65,536 バイトまで)。
    pub output: OutputChunkDto,
    /// 読み取り済みの入力バイト数。
    pub input_consumed: usize,
    /// 入力の総バイト数。
    pub input_total: usize,
    /// 直近の停止理由。未開始・リセット後、および終了に達しない Step 後は `null`。
    /// Step で終了状態に達した場合は `halted`。
    pub last_stop: Option<StopReasonDto>,
    /// 現在の命令の範囲(停止状態では `null`。実行エラー時はエラーを起こした命令)。
    pub current_range: Option<SourceRangeDto>,
}

/// バッチ実行の停止理由。`error` は実行エラーで、状態はエラー直前の値を保持する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BatchStopDto {
    /// 正常終了した。
    Halted,
    /// ブレークポイントで停止した。
    Breakpoint,
    /// 一時停止で停止した。
    Cancelled,
    /// バッチ幅に到達した。
    BatchReached,
    /// 実行エラー。
    Error {
        /// 実行エラー。
        error: RuntimeErrorDto,
    },
}

/// バッチ実行の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BatchResultDto {
    /// 停止理由。
    pub stop: BatchStopDto,
    /// この呼び出しで実行した命令数。
    ///
    /// 上流の `brainfuck_core` の `BatchReport.executed` は `u64`。2.3 の変換層で
    /// `usize::try_from` により変換し、失敗は `internal` とする(通常の 64 ビット環境では起きない)。
    pub executed: usize,
    /// 停止時点の状態。
    pub state: SessionStateDto,
}

/// 1 命令の実行結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StepOutcomeDto {
    /// 1 命令実行した。
    Executed,
    /// 実行前に正常終了していた。
    Halted,
    /// 実行エラー。
    Error {
        /// 実行エラー。
        error: RuntimeErrorDto,
    },
}

/// Step の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StepResultDto {
    /// 実行結果。
    pub outcome: StepOutcomeDto,
    /// 実行後の状態。
    pub state: SessionStateDto,
}

/// 解決済みブレークポイント。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BreakpointDto {
    /// 要求された UTF-16 オフセット。
    pub utf16_offset: usize,
    /// 解決された命令添字。
    pub instruction_index: usize,
    /// 解決された命令の範囲。
    pub range: SourceRangeDto,
}

/// ブレークポイントの設定結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BreakpointsDto {
    /// 解決済み(重複は 1 件)。
    pub breakpoints: Vec<BreakpointDto>,
    /// 未解決の入力オフセット(命令が無く設定されなかったもの)。
    pub unresolved: Vec<usize>,
}

/// セッション開始の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartedDto {
    /// 初期状態。
    pub state: SessionStateDto,
    /// 設定されたブレークポイント。
    pub breakpoints: BreakpointsDto,
}

/// ファイルを開いた結果(パスは含まない。表示名のみ)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OpenFileResult {
    /// 開いた。`lossy` は不正な UTF-8 を置換したことを示す。
    Opened {
        /// 読み込んだソース(LF 正規化済み)。
        source: String,
        /// 表示用のファイル名(ディレクトリを含まない)。
        file_name: String,
        /// 不正な UTF-8 を置換文字へ置き換えたか。
        lossy: bool,
    },
    /// 利用者が取り消した。
    Cancelled,
}

/// 保存の結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SaveFileResult {
    /// 保存した。
    Saved {
        /// 表示用のファイル名(ディレクトリを含まない)。
        file_name: String,
    },
    /// 利用者が取り消した。
    Cancelled,
}

/// DTO の TypeScript 型を `cfg` の出力先ディレクトリへ書き出す。
///
/// 書き出す全 DTO 型をここに列挙する。新しい DTO を追加したらここへも追加する。
/// 手動更新用の入口は `examples/export_bindings.rs`。テストは一時ディレクトリへ書き出して比較する。
pub fn export_bindings(cfg: &Config) -> Result<(), ExportError> {
    SourcePositionDto::export_all(cfg)?;
    SourceRangeDto::export_all(cfg)?;
    ParseErrorKind::export_all(cfg)?;
    ParseErrorDto::export_all(cfg)?;
    PointerDirectionDto::export_all(cfg)?;
    RuntimeErrorDto::export_all(cfg)?;
    OutputChunkDto::export_all(cfg)?;
    ApiErrorKind::export_all(cfg)?;
    ApiError::export_all(cfg)?;
    ViewRequestDto::export_all(cfg)?;
    StepLimitDto::export_all(cfg)?;
    LoadSourceRequest::export_all(cfg)?;
    StartSessionRequest::export_all(cfg)?;
    SessionViewRequest::export_all(cfg)?;
    RunBatchRequest::export_all(cfg)?;
    TapeWindowRequest::export_all(cfg)?;
    SetBreakpointsRequest::export_all(cfg)?;
    PauseRequest::export_all(cfg)?;
    SaveFileRequest::export_all(cfg)?;
    SetDocumentDirtyRequest::export_all(cfg)?;
    LoadSourceResult::export_all(cfg)?;
    SnapshotDto::export_all(cfg)?;
    TapeWindowDto::export_all(cfg)?;
    StopReasonDto::export_all(cfg)?;
    SessionStateDto::export_all(cfg)?;
    BatchStopDto::export_all(cfg)?;
    BatchResultDto::export_all(cfg)?;
    StepOutcomeDto::export_all(cfg)?;
    StepResultDto::export_all(cfg)?;
    BreakpointDto::export_all(cfg)?;
    BreakpointsDto::export_all(cfg)?;
    SessionStartedDto::export_all(cfg)?;
    OpenFileResult::export_all(cfg)?;
    SaveFileResult::export_all(cfg)?;
    Ok(())
}
