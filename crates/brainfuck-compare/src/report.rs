//! 方式・単位・判定・結果の型。

use std::time::Duration;

use crate::observation::{Field, Observation};

/// 比較の対象となる実行方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Method {
    /// 素朴インタプリタ(基準)。
    Naive,
    /// 最適化インタプリタ。
    Optimized,
    /// WASM バックエンド。
    Wasm,
}

/// 方式固有の実行単位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Units {
    /// 素朴実装の実行命令数。
    NaiveInstructions(u64),
    /// 最適化 IR の操作回数。
    IrOperations(u64),
    /// 方式固有の単位を持たない。
    None,
}

/// 方式が対象外とした理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnsupportedReason {
    /// 生成物のサイズが上限を超える。
    ModuleTooLarge,
    /// ループの入れ子が上限を超える。
    NestingTooDeep,
}

/// 方式が失敗した理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FailureKind {
    /// 実行量の監視に違反した。
    WatchdogTripped,
    /// 出力量の上限を超えた。
    OutputLimitExceeded,
    /// 実行エンジンまたは途中状態の不整合による失敗。
    EngineError,
    /// 生成物が方針違反として拒否された。
    PolicyRejected,
    /// 繰り返しごとの観測が一致しなかった。
    InconsistentRepetitions,
}

/// 方式ごとの計測値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metrics {
    /// 準備時間。素朴は `None`。
    pub prepare: Option<Duration>,
    /// 実行時間の中央値。
    pub run: Duration,
    /// 実際に実行した回数。
    pub repetitions: u32,
    /// 方式固有の単位。
    pub units: Units,
    /// WASM モジュールのバイト数(WASM のみ)。
    pub artifact_bytes: Option<usize>,
    /// 再生された素朴換算の命令数(WASM のみ)。
    pub replayed_steps: Option<u64>,
}

/// 基準との判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 基準そのもの。
    Oracle,
    /// 基準と完全一致。
    Match,
    /// 基準と不一致。差分の項目を含む。
    Mismatch(Vec<Field>),
}

/// 方式の結果。
///
/// `Completed` は観測値(テープ 30,000 バイトを含む)を直接持つ。方式数は高々 3 件のため箱詰めしない。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 完走し、観測と判定が得られた。
    Completed {
        /// 観測値。
        observation: Observation,
        /// 基準との判定。
        verdict: Verdict,
        /// 計測値。
        metrics: Metrics,
    },
    /// 対象外。
    Unsupported(UnsupportedReason),
    /// 失敗。
    Failed(FailureKind),
}

/// 方式ごとの報告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodReport {
    /// 方式。
    pub method: Method,
    /// 結果。
    pub outcome: Outcome,
}

/// 総合結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overall {
    /// 完走した全方式が基準と一致した。
    AllMatch,
    /// 1 つでも基準と不一致。
    Mismatch,
    /// 不一致はないが、未対応または失敗の方式がある。
    Incomplete,
}

/// 比較の結果一式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// 使用した命令数の上限。
    pub limit: u64,
    /// 方式ごとの報告。`reports[0]` は素朴(`Verdict::Oracle`)。
    pub reports: Vec<MethodReport>,
    /// 総合結果。
    pub overall: Overall,
}
