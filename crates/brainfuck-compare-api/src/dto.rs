//! 比較用 DTO(フロントとバックエンドの JSON 契約)。上流(`brainfuck-compare` の結果と観測値)から
//! DTO への写像は本モジュールの責務外(タスク 4.3 のサービスが行う)。
//!
//! 規則(デバッガ API と同じ): JSON は camelCase、判別付き列挙は `kind` タグ、`u64` は TypeScript で
//! `number`(値は JSON の安全整数範囲 2^53−1 以下)。共通部品(解析エラー・実行エラー・出力チャンク・
//! テープ窓)は `brainfuck-gui-api` のものを参照し、再定義しない。
//!
//! 要求 DTO は未知のキーを拒否する(`deny_unknown_fields`)。応答 DTO は未知のキーに寛容。
//! TypeScript 型は `export_bindings` で `apps/desktop/src/generated/` へ書き出す(生成物はコミット対象)。

use brainfuck_gui_api::dto::{OutputChunkDto, ParseErrorDto, RuntimeErrorDto, TapeWindowDto};
use serde::{Deserialize, Serialize};
use ts_rs::{Config, ExportError, TS};

/// 本 crate が所有する TypeScript 型名(生成ファイル名と一致)。
///
/// 一致検査と衝突検査はこの一覧を使う。新しい DTO を追加したら、`export_bindings` と本一覧の両方へ追加する。
pub const OWNED_TYPE_NAMES: &[&str] = &[
    "ComparisonMethodDto",
    "ComparisonInfoDto",
    "RunComparisonRequest",
    "TerminationDto",
    "ExitStateDto",
    "UnitDto",
    "MetricsDto",
    "MismatchFieldDto",
    "VerdictDto",
    "UnsupportedReasonDto",
    "FailureKindDto",
    "MethodResultDto",
    "OverallDto",
    "ComparisonResultDto",
];

/// 比較の方式。`naive` は基準で常に実行される。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ComparisonMethodDto {
    /// 素朴(核の機械)。基準。
    Naive,
    /// 最適化(IR 機械)。
    Optimized,
    /// WASM(生成・監査・隔離実行)。
    Wasm,
}

/// 比較の情報(定数)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonInfoDto {
    /// 比較の命令数上限の最大値。
    #[ts(type = "number")]
    pub max_steps: u64,
    /// 繰り返し回数の最大値。
    pub max_repetitions: u32,
    /// 繰り返し回数の既定値。
    pub default_repetitions: u32,
    /// このビルドで利用できる方式。
    pub methods: Vec<ComparisonMethodDto>,
}

/// 比較の要求。未知のキーは拒否する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunComparisonRequest {
    /// ソース(未保存の編集内容を含む)。上限は `limits::MAX_SOURCE_BYTES`。
    pub source: String,
    /// 入力の標準 Base64。上限は `limits::MAX_INPUT_BYTES`(復号後)。
    pub input_base64: String,
    /// 命令数上限(1 以上、比較用の最大値以下)。
    #[ts(type = "number")]
    pub max_steps: u64,
    /// 追加で実行する方式。`naive` は常に実行されるため、重複と `naive` の指定は拒否する。
    pub methods: Vec<ComparisonMethodDto>,
    /// 繰り返し回数(1 以上、最大値以下)。
    pub repetitions: u32,
}

/// 終了の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TerminationDto {
    /// 正常停止した。
    Halted,
    /// 実行時エラーで停止した。
    Error {
        /// 実行時エラー。
        error: RuntimeErrorDto,
    },
}

/// 終了状態(観測値の DTO 表現)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExitStateDto {
    /// 終了の種類。
    pub termination: TerminationDto,
    /// 命令ポインタ。
    pub pc: usize,
    /// データポインタ。
    pub dp: usize,
    /// `dp` のセル値。
    pub current_cell: u8,
    /// 実行した命令数(素朴実装換算。全方式で同じ意味)。
    #[ts(type = "number")]
    pub steps_executed: u64,
    /// 消費した入力バイト数。
    pub input_consumed: usize,
    /// 入力の総バイト数。
    pub input_total: usize,
    /// 出力の末尾(上限は出力チャンクの規則に従う)。
    pub output_tail: OutputChunkDto,
    /// `dp` 周辺のテープ窓。
    pub tape_window: TapeWindowDto,
    /// テープ全体の指紋(FNV-1a 64 の 16 桁 16 進)。
    pub tape_digest: String,
}

/// 方式ごとの単位。方式どうしで比較しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UnitDto {
    /// 素朴: 実行した命令数。
    NaiveInstructions {
        /// 命令数。
        #[ts(type = "number")]
        count: u64,
    },
    /// 最適化: IR 操作の実行回数。
    IrOperations {
        /// 操作の回数。
        #[ts(type = "number")]
        count: u64,
    },
    /// 方式固有の単位なし(WASM)。
    None,
}

/// 計測値(準備・実行の時間はマイクロ秒)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetricsDto {
    /// 準備時間(マイクロ秒)。準備のない方式(素朴)は `null`。
    #[ts(type = "number | null")]
    pub prepare_micros: Option<u64>,
    /// 実行時間の中央値(マイクロ秒)。
    #[ts(type = "number")]
    pub run_micros: u64,
    /// 実際に実行した回数。
    pub repetitions: u32,
    /// 方式の単位。
    pub unit: UnitDto,
    /// WASM のモジュールのバイト数。WASM 以外は `null`。
    #[ts(type = "number | null")]
    pub artifact_bytes: Option<u64>,
    /// WASM のホスト再生の命令数(0 は離脱なし、または再生が 1 命令も進めず終了)。WASM 以外は `null`。
    #[ts(type = "number | null")]
    pub replayed_steps: Option<u64>,
}

/// 不一致となった観測項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum MismatchFieldDto {
    /// 終了の種類。
    Termination,
    /// 実行エラーの値。
    ErrorValue,
    /// 命令ポインタ。
    Pc,
    /// データポインタ。
    DataPointer,
    /// `dp` のセル値。
    CurrentCell,
    /// 実行した命令数。
    StepsExecuted,
    /// 入力の位置。
    InputPosition,
    /// 出力。
    Output,
    /// テープ全体。
    Tape,
}

/// 判定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VerdictDto {
    /// 基準(素朴)自身。判定の対象外。
    Oracle,
    /// 基準と一致した。
    Match,
    /// 基準と不一致。不一致の項目を持つ。
    Mismatch {
        /// 不一致の項目。
        fields: Vec<MismatchFieldDto>,
    },
}

/// 未対応の理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UnsupportedReasonDto {
    /// モジュールのサイズが上限を超える。
    ModuleTooLarge,
    /// ループの入れ子が上限を超える。
    NestingTooDeep,
}

/// 失敗の理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FailureKindDto {
    /// 監視(ウォッチドッグ)違反。
    WatchdogTripped,
    /// 出力量の上限超過。
    OutputLimitExceeded,
    /// エンジンの失敗。
    EngineError,
    /// 方針違反(生成・監査の拒否)。
    PolicyRejected,
    /// 繰り返しの観測が一致しない。
    InconsistentRepetitions,
}

/// 方式ごとの結果(完了 / 未対応 / 失敗)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MethodResultDto {
    /// 完了した。
    Completed {
        /// 方式。
        method: ComparisonMethodDto,
        /// 判定。
        verdict: VerdictDto,
        /// 終了状態。`Box` は JSON 上は透過(ネストした値そのもの)。大きい変種による列挙の肥大を避ける。
        exit: Box<ExitStateDto>,
        /// 計測値。
        metrics: MetricsDto,
    },
    /// 未対応(エラーではない)。
    Unsupported {
        /// 方式。
        method: ComparisonMethodDto,
        /// 理由。
        reason: UnsupportedReasonDto,
    },
    /// 失敗。
    Failed {
        /// 方式。
        method: ComparisonMethodDto,
        /// 理由。
        failure: FailureKindDto,
    },
}

/// 総合結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum OverallDto {
    /// 完了した全方式が基準と一致した。
    AllMatch,
    /// 基準と不一致の方式がある。
    Mismatch,
    /// 完了していない方式がある(未対応・失敗)。
    Incomplete,
}

/// 比較結果(解析エラー / 完了)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComparisonResultDto {
    /// 解析エラー。方式は実行していない。
    ParseError {
        /// 解析エラー(デバッガ API と同一の規則)。
        error: ParseErrorDto,
    },
    /// 比較を完了した。
    Completed {
        /// 適用した命令数上限。
        #[ts(type = "number")]
        limit: u64,
        /// 総合結果。
        overall: OverallDto,
        /// 方式ごとの結果。先頭は常に `naive`。
        methods: Vec<MethodResultDto>,
    },
}

/// 比較用 DTO の TypeScript 型を `cfg` の出力先ディレクトリへ書き出す。
///
/// 書き出す全 DTO 型をここに列挙する(`OWNED_TYPE_NAMES` と同期すること)。
/// 依存する共通 DTO(gui-api 由来)も同じディレクトリへ書かれるが、内容は gui-api の生成と同一。
/// 手動更新用の入口は `examples/export_bindings.rs`。
pub fn export_bindings(cfg: &Config) -> Result<(), ExportError> {
    ComparisonMethodDto::export_all(cfg)?;
    ComparisonInfoDto::export_all(cfg)?;
    RunComparisonRequest::export_all(cfg)?;
    TerminationDto::export_all(cfg)?;
    ExitStateDto::export_all(cfg)?;
    UnitDto::export_all(cfg)?;
    MetricsDto::export_all(cfg)?;
    MismatchFieldDto::export_all(cfg)?;
    VerdictDto::export_all(cfg)?;
    UnsupportedReasonDto::export_all(cfg)?;
    FailureKindDto::export_all(cfg)?;
    MethodResultDto::export_all(cfg)?;
    OverallDto::export_all(cfg)?;
    ComparisonResultDto::export_all(cfg)?;
    Ok(())
}
