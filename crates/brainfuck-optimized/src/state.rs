//! 実行状態(crate 内部)。
//!
//! 一括適用(`fast`)と縮退再生(`replay`)が共有する状態。素朴再生の状態([`ReplayState`])に、
//! IR 上の位置と IR 操作の実行回数を足したものである。テープ等を二重に持たない。
//! `ir_ops_executed` は [`ReplayState`] の外にあるため、縮退経路は構造的にそれを更新できない。
//!
//! 呼び出し元は 3.6 で揃うため、それまで未使用警告を許可する(3.6 で除去する)。

#![allow(dead_code)]

use crate::replay::ReplayState;

/// 一括適用と縮退再生が共有する実行状態。
///
/// 不変条件: `ir_ops_executed <= replay.steps_executed`(IR 操作の各実行は、少なくとも
/// 1 個の素朴命令の換算を加えるため)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecState {
    /// 素朴再生の状態(テープ・データポインタ・素朴換算の実行数・入出力)。
    pub(crate) replay: ReplayState,
    /// IR 操作の実行回数(一括適用と縮退再生の区別に関わらず、操作を 1 回適用するごとに増える)。
    pub(crate) ir_ops_executed: u64,
    /// IR 上の操作添字。
    pub(crate) ir_pc: usize,
}

impl ExecState {
    /// 入力バイト列から初期状態を生成する。IR 位置と操作の実行回数は 0 とする。
    pub(crate) fn new(input: Vec<u8>) -> ExecState {
        ExecState {
            replay: ReplayState::new(input),
            ir_ops_executed: 0,
            ir_pc: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExecState;
    use brainfuck_core::TAPE_SIZE;

    #[test]
    fn new_starts_at_the_initial_replay_state_with_zero_ir_counters() {
        let state = ExecState::new(vec![5]);
        assert_eq!(state.ir_pc, 0);
        assert_eq!(state.ir_ops_executed, 0);
        assert_eq!(state.replay.tape().len(), TAPE_SIZE);
        assert_eq!(state.replay.steps_executed(), 0);
        assert_eq!(state.replay.input(), &[5]);
        assert_eq!(state.replay.input_position(), 0);
        assert!(state.replay.output().is_empty());
    }
}
