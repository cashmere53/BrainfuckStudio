//! 外部から受け取る値の上限(ソース/入力のサイズ、バッチ幅、窓半径、出力保持量、命令数上限)。
//!
//! 値はすべてここに 1 か所で定義する。フロントの定数モジュールは本ファイルと手で一致させる
//! (ts-rs は定数を出力しないため。一致は契約テストで確認する)。

/// ソースの上限(バイト)。1 MiB。
pub const MAX_SOURCE_BYTES: usize = 1_048_576;

/// 入力の上限(バイト)。1 MiB。
pub const MAX_INPUT_BYTES: usize = 1_048_576;

/// バッチ幅の上限。
pub const MAX_BATCH_SIZE: u64 = 1_000_000;

/// バッチ幅の既定値(上流 `brainfuck-core` の定数を再利用する)。
pub use brainfuck_core::DEFAULT_BATCH_SIZE;

/// テープ窓の半径の上限。
pub const MAX_TAPE_RADIUS: usize = 64;

/// テープ窓の半径の既定値。
pub const DEFAULT_TAPE_RADIUS: usize = 16;

/// 出力応答に含める末尾の上限(バイト)。
pub const OUTPUT_TAIL_LIMIT: usize = 65_536;

/// 命令数上限の上限。JSON の `number` で正確に表せる最大の整数(2^53 − 1)。
pub const MAX_STEP_LIMIT: u64 = 9_007_199_254_740_991;

/// 命令数上限の既定値。
pub const DEFAULT_MAX_STEPS: u64 = 10_000_000;

/// ブレークポイント配列の上限(要素数)。
pub const MAX_BREAKPOINT_OFFSETS: usize = 100_000;

// 既定値が上限の内側にあることをコンパイル時に検査する(違反はビルド失敗になる)。
const _: () = {
    assert!(DEFAULT_BATCH_SIZE >= 1 && DEFAULT_BATCH_SIZE <= MAX_BATCH_SIZE);
    assert!(DEFAULT_TAPE_RADIUS <= MAX_TAPE_RADIUS);
    assert!(DEFAULT_MAX_STEPS >= 1 && DEFAULT_MAX_STEPS <= MAX_STEP_LIMIT);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_match_the_specified_values() {
        assert_eq!(MAX_SOURCE_BYTES, 1_048_576);
        assert_eq!(MAX_INPUT_BYTES, 1_048_576);
        assert_eq!(MAX_BATCH_SIZE, 1_000_000);
        assert_eq!(DEFAULT_BATCH_SIZE, 1_000);
        assert_eq!(MAX_TAPE_RADIUS, 64);
        assert_eq!(DEFAULT_TAPE_RADIUS, 16);
        assert_eq!(OUTPUT_TAIL_LIMIT, 65_536);
        assert_eq!(MAX_STEP_LIMIT, (1u64 << 53) - 1);
        assert_eq!(DEFAULT_MAX_STEPS, 10_000_000);
        assert_eq!(MAX_BREAKPOINT_OFFSETS, 100_000);
    }
}
