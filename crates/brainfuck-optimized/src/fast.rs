//! 一括適用(FastPath)。1 つの IR 操作を、素朴換算コスト計算つきで実行状態へ一括適用する。
//!
//! 判定順序は「コスト `c` を [`Op::naive_cost`] で求める → `c > remaining` なら縮退 → 移動を伴う
//! 操作は [`Op::static_reach`] の範囲条件を評価して満たさなければ縮退 → 適用」。縮退を返すときは
//! 実行状態を一切変更しない。コストと到達範囲の式は `ir` の公開ヘルパにのみ置き、ここには持たない。
//!
//! クリアと移動/加算/乗算(`Clear` / `Transfer`)は 3.3 で一括適用を実装した。走査(`Scan`)は
//! 3.2 の時点の仮の縮退のまま、3.4 がその分岐を置き換える。
//!
//! 呼び出し元は 3.6 で揃うため、それまで未使用警告を許可する(3.6 で除去する)。

#![allow(dead_code)]

use crate::ir::{Op, OpKind, Term};
use crate::state::ExecState;
use brainfuck_core::TAPE_SIZE;

/// 一括適用の結果。`Fallback` のとき実行状態は変更されていない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FastOutcome {
    /// 操作を一括適用した。
    Applied,
    /// 一括適用しない(予算不足・範囲外・仮の縮退)。実行状態は変更されていない。
    Fallback,
}

/// 操作 `op`(`st.ir_pc` の位置にあるもの)を、残り予算 `remaining` の範囲で一括適用する。
///
/// `remaining` は上限の残り(制限なしは `u64::MAX`)。`Applied` のとき、`steps_executed` を
/// 素朴換算のコストだけ加算し、`ir_ops_executed` を 1 増やし、`ir_pc` を次の操作へ進める
/// (ループは解析時に確定した飛び先へ移る)。
pub(crate) fn execute(op: &Op, st: &mut ExecState, remaining: u64) -> FastOutcome {
    // 不変条件 `dp < TAPE_SIZE`(`ReplayState`)により添字は常に有効。
    let dp = st.replay.dp;
    let cell = st.replay.tape[dp];
    let cost = op.naive_cost(cell, 0);
    if cost > remaining {
        return FastOutcome::Fallback;
    }

    let next_pc = match op.kind() {
        OpKind::Add(n) => {
            let slot = &mut st.replay.tape[dp];
            *slot = slot.wrapping_add(*n);
            st.ir_pc + 1
        }
        OpKind::Move(n) => {
            let Some(target) = dp.checked_add_signed(*n) else {
                return FastOutcome::Fallback;
            };
            let fits = op
                .static_reach()
                .is_some_and(|reach| reach_within_tape(dp, reach));
            if !fits {
                return FastOutcome::Fallback;
            }
            st.replay.dp = target;
            st.ir_pc + 1
        }
        OpKind::Output => {
            st.replay.output.push(cell);
            st.ir_pc + 1
        }
        OpKind::Input => {
            match st.replay.input.get(st.replay.input_pos).copied() {
                Some(byte) => {
                    st.replay.tape[dp] = byte;
                    st.replay.input_pos += 1;
                }
                None => st.replay.tape[dp] = 0,
            }
            st.ir_pc + 1
        }
        OpKind::LoopStart { end } => {
            if cell == 0 {
                end.saturating_add(1)
            } else {
                st.ir_pc + 1
            }
        }
        OpKind::LoopEnd { start } => {
            if cell != 0 {
                start.saturating_add(1)
            } else {
                st.ir_pc + 1
            }
        }
        OpKind::Clear { .. } => {
            st.replay.tape[dp] = 0;
            st.ir_pc + 1
        }
        OpKind::Transfer { terms, .. } => {
            // 開始セル 0 では本体は実行されないため、到達範囲は評価せず何もしない。
            if cell != 0 {
                let fits = op
                    .static_reach()
                    .is_some_and(|reach| reach_within_tape(dp, reach));
                let all_targets_in_tape = terms.iter().all(|t| term_target(dp, t).is_some());
                if !fits || !all_targets_in_tape {
                    return FastOutcome::Fallback;
                }
                for term in terms.iter() {
                    if let Some(target) = term_target(dp, term) {
                        let slot = &mut st.replay.tape[target];
                        *slot = slot.wrapping_add(term.factor.wrapping_mul(cell));
                    }
                }
                st.replay.tape[dp] = 0;
            }
            st.ir_pc + 1
        }
        // 3.4 で一括適用を実装するまでは仮に縮退する。
        OpKind::Scan { .. } => {
            return FastOutcome::Fallback;
        }
    };

    st.replay.steps_executed = st.replay.steps_executed.saturating_add(cost);
    st.ir_ops_executed = st.ir_ops_executed.saturating_add(1);
    st.ir_pc = next_pc;
    FastOutcome::Applied
}

/// 移動/加算/乗算の項 `term` の書き込み先(`dp` からの相対位置)。テープ外なら `None`。
fn term_target(dp: usize, term: &Term) -> Option<usize> {
    dp.checked_add_signed(term.offset)
        .filter(|&target| target < TAPE_SIZE)
}

/// 現在位置 `dp` からの相対到達範囲 `reach`(`Op::static_reach` の値)がすべてテープ内に収まるか。
///
/// 範囲外に出る場合と加算がオーバーフローする場合は `false`。
fn reach_within_tape(dp: usize, (min, max): (isize, isize)) -> bool {
    matches!(
        (dp.checked_add_signed(min), dp.checked_add_signed(max)),
        (Some(_), Some(hi)) if hi < TAPE_SIZE
    )
}

#[cfg(test)]
mod tests {
    use super::{execute, FastOutcome};
    use crate::ir::{Op, OpKind, Term};
    use crate::state::ExecState;
    use brainfuck_core::TAPE_SIZE;

    const VALUES: [u8; 6] = [0, 1, 2, 127, 128, 255];

    fn op(kind: OpKind, source_len: usize) -> Op {
        Op::new(kind, 0, source_len)
    }

    fn state_at(dp: usize, cell: u8) -> ExecState {
        let mut st = ExecState::new(Vec::new());
        st.replay.dp = dp;
        st.replay.tape[dp] = cell;
        st
    }

    /// `(offset, factor)` の組から `Transfer` を組み立てる。
    fn transfer(
        terms: &[(isize, u8)],
        body_len: usize,
        reach_min: isize,
        reach_max: isize,
    ) -> OpKind {
        OpKind::Transfer {
            terms: terms
                .iter()
                .map(|&(offset, factor)| Term { offset, factor })
                .collect(),
            body_len,
            reach_min,
            reach_max,
        }
    }

    #[test]
    fn add_wraps_the_cell_and_costs_its_source_length() {
        let mut st = state_at(0, 255);
        st.ir_pc = 4;
        let outcome = execute(&op(OpKind::Add(2), 3), &mut st, u64::MAX);
        assert_eq!(outcome, FastOutcome::Applied);
        assert_eq!(st.replay.tape[0], 1);
        assert_eq!(st.replay.steps_executed, 3);
        assert_eq!(st.ir_ops_executed, 1);
        assert_eq!(st.ir_pc, 5);
    }

    #[test]
    fn add_with_zero_net_still_costs_its_source_length() {
        let mut st = state_at(0, 9);
        let outcome = execute(&op(OpKind::Add(0), 2), &mut st, u64::MAX);
        assert_eq!(outcome, FastOutcome::Applied);
        assert_eq!(st.replay.tape[0], 9);
        assert_eq!(st.replay.steps_executed, 2);
        assert_eq!(st.ir_ops_executed, 1);
    }

    #[test]
    fn move_right_and_left_shift_the_pointer_and_cost_the_magnitude() {
        let mut st = state_at(0, 0);
        assert_eq!(
            execute(&op(OpKind::Move(3), 3), &mut st, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.dp, 3);
        assert_eq!(st.replay.steps_executed, 3);
        assert_eq!(
            execute(&op(OpKind::Move(-2), 2), &mut st, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.dp, 1);
        assert_eq!(st.replay.steps_executed, 5);
        assert_eq!(st.ir_ops_executed, 2);
        assert_eq!(st.ir_pc, 2);
    }

    #[test]
    fn move_reaches_the_tape_boundaries_exactly() {
        let last = TAPE_SIZE - 1;
        let mut st = state_at(0, 0);
        let len = last;
        assert_eq!(
            execute(&op(OpKind::Move(last as isize), len), &mut st, u64::MAX),
            FastOutcome::Applied,
            "0 から 29,999 への移動は成功する"
        );
        assert_eq!(st.replay.dp, last);
    }

    #[test]
    fn move_out_of_the_tape_falls_back_without_changing_state() {
        let last = TAPE_SIZE - 1;
        let mut left = state_at(0, 7);
        let before = left.clone();
        assert_eq!(
            execute(&op(OpKind::Move(-1), 1), &mut left, u64::MAX),
            FastOutcome::Fallback
        );
        assert_eq!(left, before);

        let mut right = state_at(last, 7);
        let before = right.clone();
        assert_eq!(
            execute(&op(OpKind::Move(1), 1), &mut right, u64::MAX),
            FastOutcome::Fallback
        );
        assert_eq!(right, before);
    }

    #[test]
    fn move_with_a_huge_offset_falls_back_without_overflowing() {
        let mut st = state_at(5, 0);
        let before = st.clone();
        assert_eq!(
            execute(&op(OpKind::Move(isize::MAX), 1), &mut st, u64::MAX),
            FastOutcome::Fallback
        );
        assert_eq!(st, before);
    }

    #[test]
    fn cost_equal_to_the_remaining_budget_is_applied() {
        let mut st = state_at(0, 1);
        assert_eq!(
            execute(&op(OpKind::Add(1), 3), &mut st, 3),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.steps_executed, 3);
    }

    #[test]
    fn cost_above_the_remaining_budget_falls_back_without_changing_state() {
        let mut st = state_at(0, 1);
        let before = st.clone();
        assert_eq!(
            execute(&op(OpKind::Add(1), 3), &mut st, 2),
            FastOutcome::Fallback
        );
        assert_eq!(st, before);

        let mut mv = state_at(0, 1);
        let before = mv.clone();
        assert_eq!(
            execute(&op(OpKind::Move(5), 5), &mut mv, 4),
            FastOutcome::Fallback
        );
        assert_eq!(mv, before);
    }

    #[test]
    fn zero_remaining_budget_falls_back_for_every_operation_with_a_cost() {
        let kinds = [
            OpKind::Add(1),
            OpKind::Move(1),
            OpKind::Output,
            OpKind::Input,
            OpKind::LoopStart { end: 1 },
            OpKind::LoopEnd { start: 0 },
        ];
        for kind in kinds {
            let mut st = state_at(0, 1);
            let before = st.clone();
            assert_eq!(
                execute(&op(kind.clone(), 1), &mut st, 0),
                FastOutcome::Fallback,
                "{kind:?}"
            );
            assert_eq!(st, before, "{kind:?}");
        }
    }

    #[test]
    fn output_appends_the_current_cell_and_costs_one() {
        let mut st = state_at(0, 65);
        let outcome = execute(&op(OpKind::Output, 1), &mut st, u64::MAX);
        assert_eq!(outcome, FastOutcome::Applied);
        assert_eq!(st.replay.output, vec![65]);
        assert_eq!(st.replay.steps_executed, 1);
        assert_eq!(st.ir_ops_executed, 1);
    }

    #[test]
    fn input_stores_bytes_in_order_and_stores_zero_at_end_of_input() {
        let mut st = ExecState::new(vec![7, 8]);
        assert_eq!(
            execute(&op(OpKind::Input, 1), &mut st, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.tape[0], 7);
        assert_eq!(st.replay.input_pos, 1);

        assert_eq!(
            execute(&op(OpKind::Input, 1), &mut st, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.tape[0], 8);
        assert_eq!(st.replay.input_pos, 2);

        assert_eq!(
            execute(&op(OpKind::Input, 1), &mut st, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.tape[0], 0, "EOF では 0 を格納する");
        assert_eq!(st.replay.input_pos, 2, "EOF では入力位置が進まない");
        assert_eq!(st.replay.steps_executed, 3);
        assert_eq!(st.ir_ops_executed, 3);
    }

    #[test]
    fn loop_start_jumps_past_the_matching_end_only_when_the_cell_is_zero() {
        let kind = OpKind::LoopStart { end: 5 };

        let mut zero = state_at(0, 0);
        zero.ir_pc = 2;
        assert_eq!(
            execute(&op(kind.clone(), 1), &mut zero, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(zero.ir_pc, 6, "セル 0 なら対応する ] の次へ進む");
        assert_eq!(zero.replay.steps_executed, 1);

        let mut nonzero = state_at(0, 3);
        nonzero.ir_pc = 2;
        assert_eq!(
            execute(&op(kind, 1), &mut nonzero, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(nonzero.ir_pc, 3, "非 0 なら本体の先頭へ進む");
        assert_eq!(nonzero.replay.steps_executed, 1);
    }

    #[test]
    fn loop_end_jumps_back_past_the_matching_start_only_when_the_cell_is_nonzero() {
        let kind = OpKind::LoopEnd { start: 2 };

        let mut nonzero = state_at(0, 1);
        nonzero.ir_pc = 5;
        assert_eq!(
            execute(&op(kind.clone(), 1), &mut nonzero, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(nonzero.ir_pc, 3, "非 0 なら対応する [ の次へ戻る");
        assert_eq!(nonzero.replay.steps_executed, 1);

        let mut zero = state_at(0, 0);
        zero.ir_pc = 5;
        assert_eq!(
            execute(&op(kind, 1), &mut zero, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(zero.ir_pc, 6, "セル 0 なら抜けて次へ進む");
    }

    #[test]
    fn clear_zeroes_the_cell_and_costs_its_closed_form() {
        for v in VALUES {
            let v64 = u64::from(v);
            let mut up = state_at(4, v);
            up.ir_pc = 1;
            assert_eq!(
                execute(
                    &op(OpKind::Clear { increments: false }, 3),
                    &mut up,
                    u64::MAX
                ),
                FastOutcome::Applied,
                "[-] v={v}"
            );
            assert_eq!(up.replay.tape[4], 0, "[-] v={v}");
            let expected_up = if v == 0 { 1 } else { 1 + 2 * v64 };
            assert_eq!(up.replay.steps_executed, expected_up, "[-] v={v}");
            assert_eq!(up.ir_ops_executed, 1);
            assert_eq!(up.ir_pc, 2);

            let mut down = state_at(4, v);
            assert_eq!(
                execute(
                    &op(OpKind::Clear { increments: true }, 3),
                    &mut down,
                    u64::MAX
                ),
                FastOutcome::Applied,
                "[+] v={v}"
            );
            assert_eq!(down.replay.tape[4], 0, "[+] v={v}");
            let expected_down = if v == 0 { 1 } else { 1 + 2 * (256 - v64) };
            assert_eq!(down.replay.steps_executed, expected_down, "[+] v={v}");
        }
    }

    #[test]
    fn clear_above_the_remaining_budget_falls_back_without_changing_state() {
        // [-] の換算は 1 + 2 * 100 = 201
        let mut st = state_at(0, 100);
        let before = st.clone();
        assert_eq!(
            execute(&op(OpKind::Clear { increments: false }, 3), &mut st, 200),
            FastOutcome::Fallback
        );
        assert_eq!(st, before);
        assert_eq!(
            execute(&op(OpKind::Clear { increments: false }, 3), &mut st, 201),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.tape[0], 0);
    }

    #[test]
    fn transfer_with_a_zero_start_cell_is_a_no_op_costing_one_even_when_out_of_reach() {
        // 本体は -5 まで到達しうるが、開始セル 0 では本体を実行しないので範囲を評価しない。
        let mut st = state_at(0, 0);
        st.ir_pc = 3;
        assert_eq!(
            execute(&op(transfer(&[(-5, 1)], 8, -5, 1), 10), &mut st, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.tape[0], 0);
        assert_eq!(st.replay.steps_executed, 1);
        assert_eq!(st.ir_ops_executed, 1);
        assert_eq!(st.ir_pc, 4);
    }

    #[test]
    fn transfer_adds_scaled_values_and_clears_the_start_cell() {
        // [->>+++<<] 相当: v = 5, 項 (+2, 係数 3), 本体 8 命令。換算 1 + 5 * (8 + 1) = 46
        let mut st = state_at(10, 5);
        assert_eq!(
            execute(&op(transfer(&[(2, 3)], 8, 0, 2), 8), &mut st, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.tape[10], 0);
        assert_eq!(st.replay.tape[12], 15);
        assert_eq!(st.replay.steps_executed, 46);
        assert_eq!(st.ir_ops_executed, 1);
        assert_eq!(st.ir_pc, 1);
    }

    #[test]
    fn transfer_multiplies_with_wrapping_mod_256() {
        // v = 255: 係数 3 → 765 mod 256 = 253、係数 2 → 510 mod 256 = 254。
        // 開始セル 10 に 254 を足すと 264 mod 256 = 8 になる。
        let mut st = state_at(5, 255);
        st.replay.tape[4] = 10;
        assert_eq!(
            execute(
                &op(transfer(&[(-1, 2), (1, 3)], 4, -1, 1), 4),
                &mut st,
                u64::MAX
            ),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.tape[4], 8);
        assert_eq!(st.replay.tape[6], 253);
        assert_eq!(st.replay.tape[5], 0);
    }

    #[test]
    fn transfer_reaching_the_last_cell_exactly_is_applied() {
        let last = TAPE_SIZE - 1;
        let mut st = state_at(last - 2, 1);
        assert_eq!(
            execute(&op(transfer(&[(2, 1)], 3, 0, 2), 3), &mut st, u64::MAX),
            FastOutcome::Applied
        );
        assert_eq!(st.replay.tape[last], 1);
    }

    #[test]
    fn transfer_whose_reach_leaves_the_tape_falls_back_for_a_nonzero_start_cell() {
        let mut left = state_at(0, 3);
        let before = left.clone();
        assert_eq!(
            execute(&op(transfer(&[(-1, 1)], 4, -1, 0), 4), &mut left, u64::MAX),
            FastOutcome::Fallback
        );
        assert_eq!(left, before);

        let last = TAPE_SIZE - 1;
        let mut right = state_at(last, 3);
        let before = right.clone();
        assert_eq!(
            execute(&op(transfer(&[(1, 1)], 4, 0, 1), 4), &mut right, u64::MAX),
            FastOutcome::Fallback
        );
        assert_eq!(right, before);
    }

    #[test]
    fn transfer_cost_above_the_remaining_budget_falls_back_without_changing_state() {
        // 換算は 1 + 5 * 9 = 46
        let mut st = state_at(10, 5);
        let before = st.clone();
        assert_eq!(
            execute(&op(transfer(&[(2, 3)], 8, 0, 2), 8), &mut st, 45),
            FastOutcome::Fallback
        );
        assert_eq!(st, before);
        assert_eq!(
            execute(&op(transfer(&[(2, 3)], 8, 0, 2), 8), &mut st, 46),
            FastOutcome::Applied
        );
    }

    #[test]
    fn scan_falls_back_until_task_3_4_lands() {
        let mut st = state_at(0, 4);
        let before = st.clone();
        assert_eq!(
            execute(&op(OpKind::Scan { stride: 1 }, 1), &mut st, u64::MAX),
            FastOutcome::Fallback
        );
        assert_eq!(st, before);
    }

    #[test]
    fn applied_step_increment_equals_the_naive_cost_helper() {
        let operations = [
            op(OpKind::Add(5), 4),
            op(OpKind::Move(-2), 2),
            op(OpKind::Output, 1),
            op(OpKind::Input, 1),
            op(OpKind::LoopStart { end: 1 }, 1),
            op(OpKind::LoopEnd { start: 0 }, 1),
            op(OpKind::Clear { increments: false }, 3),
            op(OpKind::Clear { increments: true }, 3),
            op(transfer(&[(1, 1)], 3, 0, 1), 3),
        ];
        for operation in &operations {
            for v in VALUES {
                let mut st = state_at(10, v);
                let before = st.replay.steps_executed;
                let outcome = execute(operation, &mut st, u64::MAX);
                assert_eq!(
                    outcome,
                    FastOutcome::Applied,
                    "{:?} v={v}",
                    operation.kind()
                );
                assert_eq!(
                    st.replay.steps_executed - before,
                    operation.naive_cost(v, 0),
                    "{:?} v={v}",
                    operation.kind()
                );
            }
        }
    }
}
