//! 比較の上限値。

use std::time::Duration;

/// 1 回の比較で各方式に許す最大命令数(素朴換算)。
pub const MAX_COMPARISON_STEPS: u64 = 50_000_000;

/// 繰り返し実行の最大回数。
pub const MAX_REPETITIONS: u32 = 5;

/// 繰り返し実行の既定回数。
pub const DEFAULT_REPETITIONS: u32 = 3;

/// 基準の繰り返し実行に許す時間予算。超えたら繰り返しを打ち切る(判定には使わない)。
pub const REPEAT_TIME_BUDGET: Duration = Duration::from_secs(2);
