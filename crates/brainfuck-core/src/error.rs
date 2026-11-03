//! 解析エラー・実行エラーと、ポインタ移動方向の型。
//!
//! 診断文は英語の固定文面とし、数値と列挙値のみを含める。
//! ソース文字列・入力バイト列・パスは保持も表示もしない。

use std::error::Error;
use std::fmt;

/// データポインタの移動方向。範囲外エラーの原因を示す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerDirection {
    /// 左(`<`)への移動。
    Left,
    /// 右(`>`)への移動。
    Right,
}

impl fmt::Display for PointerDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PointerDirection::Left => f.write_str("left"),
            PointerDirection::Right => f.write_str("right"),
        }
    }
}

/// 解析(`Program::parse` / `Program::parse_bytes`)で発生する括弧エラー。
///
/// 実行前に失敗が判明するため、このエラーを持つ `Program` は存在しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// 対応する `[` を持たない `]`。走査中に最初に見つかった位置。
    UnmatchedClosingBracket {
        /// 違反した `]` の命令添字。
        instruction_index: usize,
        /// 違反した `]` のソースバイトオフセット。
        source_offset: usize,
    },
    /// 解析終了時に残った未解決 `[` のうち、最初に積まれたもの。
    UnmatchedOpeningBracket {
        /// 違反した `[` の命令添字。
        instruction_index: usize,
        /// 違反した `[` のソースバイトオフセット。
        source_offset: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ParseError::UnmatchedClosingBracket {
                instruction_index,
                source_offset,
            } => write!(
                f,
                "unmatched closing bracket: instruction #{instruction_index}, source offset {source_offset}"
            ),
            ParseError::UnmatchedOpeningBracket {
                instruction_index,
                source_offset,
            } => write!(
                f,
                "unmatched opening bracket: instruction #{instruction_index}, source offset {source_offset}"
            ),
        }
    }
}

impl Error for ParseError {}

/// 実行(`Machine::step` / `Machine::run`)で発生する実行エラー。
///
/// エラーを返した時点で機械の状態は変更されていない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RuntimeError {
    /// データポインタが範囲外へ移動しようとした。状態は変更されない。
    PointerOutOfBounds {
        /// 発生時の命令ポインタ。
        pc: usize,
        /// 発生時のデータポインタ。
        dp: usize,
        /// 範囲外へ移動しようとした方向。
        direction: PointerDirection,
    },
    /// 実行済み命令数が上限に達し、機械が未停止のまま次の命令を実行できない。
    /// 実行済み数は常に `limit` に等しい。
    StepLimitExceeded {
        /// 設定された命令数の上限。
        limit: u64,
        /// 上限到達時の命令ポインタ。
        pc: usize,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RuntimeError::PointerOutOfBounds { pc, dp, direction } => write!(
                f,
                "data pointer out of bounds: pc={pc}, dp={dp}, direction={direction}"
            ),
            RuntimeError::StepLimitExceeded { limit, pc } => write!(
                f,
                "step limit exceeded: limit={limit} after {limit} steps, pc={pc}"
            ),
        }
    }
}

impl Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::{ParseError, PointerDirection, RuntimeError};
    use std::any::TypeId;
    use std::error::Error;

    #[test]
    fn unmatched_closing_bracket_display_contains_cause_index_and_offset() {
        let error = ParseError::UnmatchedClosingBracket {
            instruction_index: 3,
            source_offset: 5,
        };
        let text = error.to_string();
        assert!(text.contains("unmatched closing bracket"), "{text}");
        assert!(text.contains("instruction #3"), "{text}");
        assert!(text.contains("source offset 5"), "{text}");
    }

    #[test]
    fn unmatched_opening_bracket_display_contains_cause_index_and_offset() {
        let error = ParseError::UnmatchedOpeningBracket {
            instruction_index: 12,
            source_offset: 40,
        };
        let text = error.to_string();
        assert!(text.contains("unmatched opening bracket"), "{text}");
        assert!(text.contains("instruction #12"), "{text}");
        assert!(text.contains("source offset 40"), "{text}");
    }

    #[test]
    fn pointer_out_of_bounds_display_contains_cause_pc_dp_and_direction() {
        let left = RuntimeError::PointerOutOfBounds {
            pc: 7,
            dp: 0,
            direction: PointerDirection::Left,
        };
        let text = left.to_string();
        assert!(text.contains("data pointer out of bounds"), "{text}");
        assert!(text.contains("pc=7"), "{text}");
        assert!(text.contains("dp=0"), "{text}");
        assert!(text.contains("direction=left"), "{text}");

        let right = RuntimeError::PointerOutOfBounds {
            pc: 99,
            dp: 29_999,
            direction: PointerDirection::Right,
        };
        let text = right.to_string();
        assert!(text.contains("pc=99"), "{text}");
        assert!(text.contains("dp=29999"), "{text}");
        assert!(text.contains("direction=right"), "{text}");
    }

    #[test]
    fn step_limit_exceeded_display_contains_cause_limit_and_pc() {
        let error = RuntimeError::StepLimitExceeded { limit: 5, pc: 2 };
        let text = error.to_string();
        assert!(text.contains("step limit exceeded"), "{text}");
        assert!(text.contains("limit=5"), "{text}");
        assert!(text.contains("after 5 steps"), "{text}");
        assert!(text.contains("pc=2"), "{text}");
    }

    #[test]
    fn pointer_direction_displays_as_left_or_right() {
        assert_eq!(PointerDirection::Left.to_string(), "left");
        assert_eq!(PointerDirection::Right.to_string(), "right");
    }

    #[test]
    fn parse_error_implements_std_error_without_source() {
        let error = ParseError::UnmatchedClosingBracket {
            instruction_index: 0,
            source_offset: 0,
        };
        let dynamic: &dyn Error = &error;
        assert!(dynamic.source().is_none());
        let boxed: Box<dyn Error> = Box::new(error);
        assert!(boxed.to_string().contains("unmatched closing bracket"));
        let from_value: Box<dyn Error> = error.into();
        assert!(from_value.to_string().contains("instruction #0"));
    }

    #[test]
    fn runtime_error_implements_std_error_without_source() {
        let error = RuntimeError::StepLimitExceeded { limit: 0, pc: 0 };
        let dynamic: &dyn Error = &error;
        assert!(dynamic.source().is_none());
        let boxed: Box<dyn Error> = Box::new(error);
        assert!(boxed.to_string().contains("step limit exceeded"));
        let from_value: Box<dyn Error> = error.into();
        assert!(from_value.to_string().contains("limit=0"));
    }

    #[test]
    fn parse_error_and_runtime_error_are_distinct_types() {
        assert_ne!(
            TypeId::of::<ParseError>(),
            TypeId::of::<RuntimeError>(),
            "解析エラーと実行エラーは別の型である"
        );
    }

    #[test]
    fn errors_are_copy_and_comparable() {
        let a = RuntimeError::StepLimitExceeded { limit: 1, pc: 0 };
        let b = a;
        assert_eq!(a, b);
        let p = ParseError::UnmatchedOpeningBracket {
            instruction_index: 1,
            source_offset: 1,
        };
        let q = p;
        assert_eq!(p, q);
    }
}
