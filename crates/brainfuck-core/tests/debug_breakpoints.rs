//! ブレークポイント集合(`BreakpointSet`)の単体テスト。
//!
//! 命令添字のみを扱い、範囲外の追加・トグルはエラーで集合を変更しない。
//! 問い合わせと削除は範囲外でも異常終了せず偽を返す。

mod debug_common;

use brainfuck_core::{
    BatchSize, BreakpointError, BreakpointSet, CancelFn, DebugSession, Machine, MachineConfig,
    NeverCancel, RuntimeError, StopReason,
};
use debug_common::{machine_from, session_from};
use std::error::Error;

/// 集合の内容を昇順の添字列として取り出す(検証用の局所ヘルパ)。
fn indices(set: &BreakpointSet) -> Vec<usize> {
    set.iter().collect()
}

#[test]
fn new_set_is_empty_and_contains_nothing() {
    let set = BreakpointSet::new(5);
    assert_eq!(set.instruction_count(), 5);
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    assert!(indices(&set).is_empty());
    for index in 0..5 {
        assert!(!set.contains(index));
    }
}

#[test]
fn insert_new_index_reports_true_and_adds_it() {
    let mut set = BreakpointSet::new(5);
    assert_eq!(set.insert(2), Ok(true));
    assert!(set.contains(2));
    assert_eq!(set.len(), 1);
    assert!(!set.is_empty());
}

#[test]
fn insert_existing_index_reports_false_and_keeps_count() {
    let mut set = BreakpointSet::new(5);
    assert_eq!(set.insert(3), Ok(true));
    assert_eq!(set.insert(3), Ok(false));
    assert!(set.contains(3));
    assert_eq!(set.len(), 1);
    assert_eq!(indices(&set), vec![3]);
}

#[test]
fn insert_out_of_range_is_error_and_leaves_set_unchanged() {
    let mut set = BreakpointSet::new(4);
    assert_eq!(set.insert(1), Ok(true));
    let before = set.clone();

    assert_eq!(
        set.insert(4),
        Err(BreakpointError::OutOfRange {
            instruction_index: 4,
            instruction_count: 4,
        })
    );
    assert_eq!(
        set.insert(100),
        Err(BreakpointError::OutOfRange {
            instruction_index: 100,
            instruction_count: 4,
        })
    );
    assert_eq!(set, before);
    assert_eq!(set.len(), 1);
    assert_eq!(indices(&set), vec![1]);
    assert!(!set.contains(4));
}

#[test]
fn out_of_range_error_message_contains_only_numbers_and_fixed_text() {
    let error = BreakpointError::OutOfRange {
        instruction_index: 7,
        instruction_count: 3,
    };
    let message = error.to_string();
    assert!(message.contains('7'));
    assert!(message.contains('3'));
    // `std::error::Error` を実装していること。
    let _: &dyn Error = &error;
}

#[test]
fn remove_existing_reports_true_and_missing_reports_false() {
    let mut set = BreakpointSet::new(5);
    set.insert(0).unwrap();
    set.insert(4).unwrap();

    assert!(set.remove(4));
    assert!(!set.contains(4));
    assert_eq!(set.len(), 1);

    assert!(!set.remove(4));
    assert!(!set.remove(2));
    assert_eq!(indices(&set), vec![0]);
}

#[test]
fn remove_out_of_range_returns_false_without_panic() {
    let mut set = BreakpointSet::new(2);
    set.insert(1).unwrap();
    let before = set.clone();

    assert!(!set.remove(2));
    assert!(!set.remove(usize::MAX));
    assert_eq!(set, before);
}

#[test]
fn toggle_round_trip_restores_original_state() {
    let mut set = BreakpointSet::new(3);

    assert_eq!(set.toggle(1), Ok(true));
    assert!(set.contains(1));
    assert_eq!(set.len(), 1);

    assert_eq!(set.toggle(1), Ok(false));
    assert!(!set.contains(1));
    assert_eq!(set.len(), 0);
    assert!(set.is_empty());
}

#[test]
fn toggle_out_of_range_is_error_and_leaves_set_unchanged() {
    let mut set = BreakpointSet::new(3);
    set.insert(0).unwrap();
    let before = set.clone();

    assert_eq!(
        set.toggle(3),
        Err(BreakpointError::OutOfRange {
            instruction_index: 3,
            instruction_count: 3,
        })
    );
    assert_eq!(set, before);
    assert_eq!(indices(&set), vec![0]);
}

#[test]
fn clear_removes_all_indices() {
    let mut set = BreakpointSet::new(10);
    for index in [0, 3, 9] {
        set.insert(index).unwrap();
    }
    assert_eq!(set.len(), 3);

    set.clear();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    assert!(indices(&set).is_empty());
    assert!(!set.contains(0));
    assert!(!set.contains(3));
    assert!(!set.contains(9));
}

#[test]
fn iter_is_ascending_and_without_duplicates() {
    let mut set = BreakpointSet::new(10);
    for index in [5, 1, 3, 1, 0, 9, 5] {
        let _ = set.insert(index);
    }
    assert_eq!(indices(&set), vec![0, 1, 3, 5, 9]);
    assert_eq!(set.len(), 5);
}

#[test]
fn zero_instruction_set_rejects_every_index() {
    let mut set = BreakpointSet::new(0);
    assert_eq!(set.instruction_count(), 0);
    assert!(set.is_empty());
    assert!(indices(&set).is_empty());

    assert_eq!(
        set.insert(0),
        Err(BreakpointError::OutOfRange {
            instruction_index: 0,
            instruction_count: 0,
        })
    );
    assert_eq!(
        set.toggle(0),
        Err(BreakpointError::OutOfRange {
            instruction_index: 0,
            instruction_count: 0,
        })
    );
    assert!(!set.contains(0));
    assert!(!set.remove(0));
    assert!(set.is_empty());
}

#[test]
fn contains_and_remove_out_of_range_do_not_panic() {
    let set = BreakpointSet::new(2);
    assert!(!set.contains(2));
    assert!(!set.contains(usize::MAX));
}

#[test]
fn set_is_comparable_and_cloneable_for_value_semantics() {
    let mut left = BreakpointSet::new(4);
    left.insert(2).unwrap();
    let right = left.clone();
    assert_eq!(left, right);

    left.insert(0).unwrap();
    assert_ne!(left, right);
    assert_eq!(indices(&right), vec![2]);
}

// ---------------------------------------------------------------------------
// タスク 3.4: ブレークポイント停止と再開規則のバッチ実行への組み込み
// ---------------------------------------------------------------------------

/// 命令数 `n` のバッチ命令数を作る(検証用の局所ヘルパ)。
fn batch(n: u64) -> BatchSize {
    BatchSize::new(n).expect("テスト用バッチ命令数は 1 以上")
}

/// 指定した命令添字にブレークポイントを設定したセッションを作る(検証用の局所ヘルパ)。
fn session_with_breakpoints(source: &str, indices: &[usize]) -> DebugSession {
    let mut session = session_from(source, b"", MachineConfig::unlimited());
    for &index in indices {
        session.breakpoints_mut().insert(index).unwrap();
    }
    session
}

#[test]
fn breakpoint_at_initial_pc_stops_immediately_without_executing() {
    let mut session = session_with_breakpoints("+.", &[0]);
    let report = session.run_batch(batch(10), &NeverCancel);

    assert_eq!(report.stop, Ok(StopReason::Breakpoint));
    assert_eq!(report.executed, 0);
    assert_eq!(report.snapshot.pc, 0);
    assert_eq!(report.snapshot.steps_executed, 0);
    assert_eq!(session.last_stop(), Some(StopReason::Breakpoint));
}

#[test]
fn resume_after_breakpoint_executes_the_stopped_instruction_and_advances() {
    let mut session = session_with_breakpoints("+.", &[0]);
    let first = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(first.stop, Ok(StopReason::Breakpoint));

    // 同じ位置で即座に停止し続けず、停止位置の命令を実行して前進する。
    let second = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(second.stop, Ok(StopReason::Halted));
    assert_eq!(second.executed, 2);
    assert_eq!(second.snapshot.pc, 2);
    assert_eq!(second.snapshot.current_cell, 1);
    assert_eq!(session.machine().output(), &[1u8][..]);
}

#[test]
fn breakpoint_stop_leaves_machine_state_unchanged_after_exactly_the_prior_instructions() {
    // "+>+" の添字 1 に設定: 命令 0 を 1 回実行した直後で停止する。
    let source = "+>+";
    let mut session = session_with_breakpoints(source, &[1]);
    let report = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(report.stop, Ok(StopReason::Breakpoint));
    assert_eq!(report.executed, 1);

    let mut expected: Machine = machine_from(source, b"", MachineConfig::unlimited());
    expected.step().unwrap();
    assert_eq!(session.machine(), &expected);
    assert_eq!(session.snapshot().pc, 1);
    assert_eq!(session.snapshot().data_pointer, 0);
    assert_eq!(session.snapshot().steps_executed, 1);
}

#[test]
fn breakpoint_inside_loop_stops_at_every_iteration() {
    // "++[-]": 添字 3 ('-') に設定。セル値 2 のループは 2 回まわる。
    let mut session = session_with_breakpoints("++[-]", &[3]);
    let mut stops = Vec::new();
    loop {
        let report = session.run_batch(batch(1_000), &NeverCancel);
        let stop = report.stop.expect("エラーは発生しない");
        stops.push(stop);
        if stop == StopReason::Halted {
            break;
        }
        assert!(stops.len() < 10, "無限に停止し続けてはならない");
    }
    assert_eq!(
        stops,
        vec![
            StopReason::Breakpoint,
            StopReason::Breakpoint,
            StopReason::Halted
        ]
    );
    assert_eq!(session.snapshot().current_cell, 0);
}

#[test]
fn step_into_landing_on_a_breakpoint_lets_run_advance() {
    let mut session = session_with_breakpoints("+++.", &[1]);
    session.step_into().unwrap();
    assert_eq!(session.snapshot().pc, 1);

    // Step Into 直後の着地位置はブレークポイントでも前進する。
    let report = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(report.stop, Ok(StopReason::Halted));
    assert_eq!(report.executed, 3);
    assert_eq!(report.snapshot.current_cell, 3);
}

#[test]
fn breakpoint_on_the_batch_boundary_is_not_missed() {
    // "+++.": 幅 2 で pc=2 にブレークポイント。最初のバッチ末尾の pc に一致する。
    let mut session = session_with_breakpoints("+++.", &[2]);

    let first = session.run_batch(batch(2), &NeverCancel);
    assert_eq!(first.stop, Ok(StopReason::Breakpoint));
    assert_eq!(first.executed, 2);
    assert_eq!(first.snapshot.pc, 2);

    let second = session.run_batch(batch(2), &NeverCancel);
    assert_eq!(second.stop, Ok(StopReason::Halted));
    assert_eq!(second.executed, 2);
}

#[test]
fn breakpoint_on_the_batch_boundary_wins_over_step_limit_reached() {
    // 幅ちょうど 1 で pc=1 に設定。StepLimitReached ではなく Breakpoint になる。
    let mut session = session_with_breakpoints("++", &[1]);
    let report = session.run_batch(batch(1), &NeverCancel);
    assert_eq!(report.stop, Ok(StopReason::Breakpoint));
    assert_eq!(report.executed, 1);
}

#[test]
fn halted_session_reports_halted_even_with_a_breakpoint_on_the_last_pc() {
    let mut session = session_with_breakpoints("+.", &[1]);
    assert_eq!(
        session.run_batch(batch(10), &NeverCancel).stop,
        Ok(StopReason::Breakpoint)
    );
    assert_eq!(
        session.run_batch(batch(10), &NeverCancel).stop,
        Ok(StopReason::Halted)
    );

    let again = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(again.stop, Ok(StopReason::Halted));
    assert_eq!(again.executed, 0);
}

#[test]
fn breakpoints_added_and_removed_between_batches_apply_from_the_next_batch() {
    let mut session = session_from("+++.", b"", MachineConfig::unlimited());
    let first = session.run_batch(batch(1), &NeverCancel);
    assert_eq!(first.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(first.snapshot.pc, 1);

    // 実行間に追加した添字は次のバッチから有効になる。
    session.breakpoints_mut().insert(3).unwrap();
    let second = session.run_batch(batch(100), &NeverCancel);
    assert_eq!(second.stop, Ok(StopReason::Breakpoint));
    assert_eq!(second.executed, 2);
    assert_eq!(second.snapshot.pc, 3);

    // 削除も次のバッチから有効になる。
    assert!(session.breakpoints_mut().remove(3));
    let third = session.run_batch(batch(100), &NeverCancel);
    assert_eq!(third.stop, Ok(StopReason::Halted));
}

#[test]
fn reset_restores_immediate_breakpoint_stop_at_pc_zero() {
    let mut session = session_with_breakpoints("+.", &[0]);
    assert_eq!(
        session.run_batch(batch(10), &NeverCancel).stop,
        Ok(StopReason::Breakpoint)
    );
    assert_eq!(
        session.run_batch(batch(10), &NeverCancel).stop,
        Ok(StopReason::Halted)
    );

    session.reset();
    let report = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(report.stop, Ok(StopReason::Breakpoint));
    assert_eq!(report.executed, 0);
    assert_eq!(report.snapshot.pc, 0);
}

#[test]
fn breakpoint_index_is_the_instruction_index_and_comment_characters_are_not_instructions() {
    // コメント文字 'x' 'y' は命令ではないため、添字 1 は 2 個目の命令('+')を指す。
    let mut session = session_with_breakpoints("+x+y.", &[1]);
    let report = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(report.stop, Ok(StopReason::Breakpoint));
    assert_eq!(report.snapshot.pc, 1);
    assert_eq!(report.executed, 1);
}

#[test]
fn cancel_and_breakpoint_on_the_same_position_report_cancelled_then_breakpoint() {
    let mut session = session_with_breakpoints("+.", &[0]);

    let cancelled = session.run_batch(batch(10), &CancelFn(|| true));
    assert_eq!(cancelled.stop, Ok(StopReason::Cancelled));
    assert_eq!(cancelled.executed, 0);
    assert_eq!(cancelled.snapshot.pc, 0);

    // キャンセルでは承認されない。次のバッチでブレークポイントが通知される。
    let next = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(next.stop, Ok(StopReason::Breakpoint));
    assert_eq!(next.executed, 0);
}

#[test]
fn breakpoint_stop_sets_last_stop_and_keeps_the_breakpoint_set() {
    let mut session = session_with_breakpoints("+.", &[0]);
    session.run_batch(batch(10), &NeverCancel);
    assert_eq!(session.last_stop(), Some(StopReason::Breakpoint));
    assert!(session.breakpoints().contains(0));
}

// ---------------------------------------------------------------------------
// タスク 3.4 レビュー差し戻し(ラウンド 1): 再開フラグの規則を検出する追加テスト
// ---------------------------------------------------------------------------

#[test]
fn reset_after_breakpoint_stop_clears_resume_flag() {
    // Breakpoint 停止直後(再開フラグは真)にリセットしても、初期 pc のブレークポイントで即時停止する。
    let mut session = session_with_breakpoints("+.", &[0]);
    assert_eq!(
        session.run_batch(batch(10), &NeverCancel).stop,
        Ok(StopReason::Breakpoint)
    );

    session.reset();
    let report = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(report.stop, Ok(StopReason::Breakpoint));
    assert_eq!(report.executed, 0);
    assert_eq!(report.snapshot.pc, 0);
    assert_eq!(report.snapshot.steps_executed, 0);
}

#[test]
fn reset_after_step_into_clears_resume_flag() {
    // Step Into 直後(再開フラグは真)にリセットしても、初期 pc のブレークポイントで即時停止する。
    let mut session = session_with_breakpoints("+.", &[0]);
    session.step_into().unwrap();
    assert_eq!(session.snapshot().pc, 1);

    session.reset();
    let report = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(report.stop, Ok(StopReason::Breakpoint));
    assert_eq!(report.executed, 0);
    assert_eq!(report.snapshot.pc, 0);
}

#[test]
fn cancel_after_breakpoint_stop_keeps_resume_flag_so_next_batch_advances() {
    // Breakpoint で停止した後にキャンセルされても、再開フラグは変わらない。
    // 次のバッチは停止位置の命令を実行して前進し、同じ位置で Breakpoint にならない。
    let mut session = session_with_breakpoints("+.", &[0]);
    assert_eq!(
        session.run_batch(batch(10), &NeverCancel).stop,
        Ok(StopReason::Breakpoint)
    );

    let cancelled = session.run_batch(batch(10), &CancelFn(|| true));
    assert_eq!(cancelled.stop, Ok(StopReason::Cancelled));
    assert_eq!(cancelled.executed, 0);
    assert_eq!(cancelled.snapshot.pc, 0);

    let next = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(next.stop, Ok(StopReason::Halted));
    assert_eq!(next.executed, 2);
    assert_eq!(next.snapshot.pc, 2);
}

#[test]
fn error_at_breakpoint_pc_keeps_resume_flag_and_repeats_the_same_error() {
    // "<" は添字 0 で必ずポインタ範囲外になる。停止位置の命令がエラーでも再開フラグは変えない。
    let mut session = session_with_breakpoints("<", &[0]);
    let before = session.machine().clone();

    assert_eq!(
        session.run_batch(batch(10), &NeverCancel).stop,
        Ok(StopReason::Breakpoint)
    );

    let first = session.run_batch(batch(10), &NeverCancel);
    assert!(matches!(
        first.stop,
        Err(RuntimeError::PointerOutOfBounds { .. })
    ));
    assert_eq!(first.executed, 0);

    // フラグが真のままなので、3 回目もブレークポイントではなく同じエラーになる。
    let second = session.run_batch(batch(10), &NeverCancel);
    assert!(matches!(
        second.stop,
        Err(RuntimeError::PointerOutOfBounds { .. })
    ));
    assert_eq!(second.executed, 0);
    assert_eq!(session.machine(), &before);
}
