//! 制限値。
//!
//! 生成・監査・実行・比較のすべてがこの値を参照する。変更は設計の Revalidation Trigger に該当する。

/// 生成モジュールのバイト数の上限(8 MiB)。超えれば `CompileError::ModuleTooLarge`。
pub const MAX_MODULE_BYTES: usize = 8 * 1024 * 1024;

/// ループの入れ子の上限。超えれば `CompileError::NestingTooDeep`。
pub const MAX_LOOP_DEPTH: usize = 256;

/// 素朴換算の 1 実行数あたりに割り当てる燃料(実行量監視の倍率)。
pub const FUEL_PER_STEP: u64 = 64;

/// 燃料の下駄(定数オーバーヘッド分)。燃料は `上限 × FUEL_PER_STEP + FUEL_BASE + 生成物のバイト数`
/// (生成物のバイト数は `runtime.rs` が加える。直線的な本体の入口での一括請求を上限 0 でも賄うため)。
pub const FUEL_BASE: u64 = 1_024;

/// 出力量の上限(64 MiB)。比較クレートのテストからも参照できるよう公開する。
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
