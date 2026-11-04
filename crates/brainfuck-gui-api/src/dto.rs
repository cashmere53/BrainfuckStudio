//! フロントとバックエンドの JSON 契約(DTO)。
//! 上流(`brainfuck-core` / `debugger-state`)の値から DTO への写像のみを行う。
//!
//! 規則: JSON は camelCase、判別付き列挙は `kind` タグ、`u64` は TypeScript で `number`
//! (値は JSON の安全整数範囲 2^53−1 以下に制限する。`limits` を参照)。
//! TypeScript 型は `export_bindings` で `apps/desktop/src/generated/` へ書き出す。

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

/// API エラー。`message` は固定の英語文面で、パス・ソース・入力などの機微情報を含まない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// エラーの種類。
    pub kind: ApiErrorKind,
    /// 固定の英語文面。
    pub message: String,
    /// 不正だった要求の項目名(該当しない場合は `null`)。
    pub field: Option<String>,
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
    Ok(())
}
