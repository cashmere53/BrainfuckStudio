//! デバッグセッション(生成・観測・リセット・バッチ幅)の単体テスト(タスク 3.1)。
//!
//! 生成直後の状態が core の機械と一致すること、読み取りが状態を変えないこと、
//! リセットが初期状態へ戻しブレークポイントを保持すること、セッションが独立していることを確認する。
//! 機械を進めた後のリセットは Step Into 以降のタスクで検証する。

mod debug_common;

use brainfuck_core::{
    BatchSize, CancelFn, DebugSession, Machine, MachineConfig, NeverCancel, Program, RuntimeError,
    StepResult, StopReason, DEFAULT_BATCH_SIZE, DEFAULT_WINDOW_RADIUS,
};
use debug_common::{cancel_after, session_from};
use std::cell::Cell;
use std::sync::atomic::AtomicBool;

/// 値が `Send` であることをコンパイル時に確認する。
fn assert_send<T: Send>() {}

#[test]
fn session_can_move_to_another_thread() {
    assert_send::<DebugSession>();
}

#[test]
fn new_session_is_identical_to_a_fresh_machine() {
    let source = "+[>,.]";
    let input = b"xy";
    let config = MachineConfig::with_max_steps(500);
    let session = session_from(source, input, config);
    let fresh = Machine::new(Program::parse(source).unwrap(), input.to_vec(), config);

    assert_eq!(session.machine(), &fresh);
    assert_eq!(session.snapshot(), fresh.snapshot());
    assert_eq!(session.machine().tape(), fresh.tape());
    assert_eq!(session.machine().output(), fresh.output());
    assert_eq!(session.machine().input_position(), fresh.input_position());
}

#[test]
fn new_session_has_no_breakpoints_and_no_last_stop() {
    let session = session_from("+[>,.]", b"", MachineConfig::unlimited());
    assert!(session.breakpoints().is_empty());
    assert_eq!(session.breakpoints().len(), 0);
    assert_eq!(session.breakpoints().instruction_count(), 6);
    assert_eq!(session.last_stop(), None);
}

#[test]
fn default_batch_size_is_one_thousand() {
    assert_eq!(DEFAULT_BATCH_SIZE, 1_000);
    assert_eq!(BatchSize::DEFAULT.get(), 1_000);
    assert_eq!(BatchSize::default(), BatchSize::DEFAULT);
}

#[test]
fn batch_size_rejects_zero() {
    assert_eq!(BatchSize::new(0), None);
}

#[test]
fn batch_size_accepts_any_positive_count() {
    assert_eq!(BatchSize::new(1).map(BatchSize::get), Some(1));
    assert_eq!(BatchSize::new(7).map(BatchSize::get), Some(7));
    assert_eq!(BatchSize::new(u64::MAX).map(BatchSize::get), Some(u64::MAX));
}

#[test]
fn sessions_from_the_same_input_are_independent() {
    let config = MachineConfig::unlimited();
    let mut first = session_from("+.", b"", config);
    let second = session_from("+.", b"", config);
    let untouched = second.machine().clone();

    first.breakpoints_mut().insert(1).expect("1 は有効な添字");
    assert!(first.breakpoints().contains(1));
    assert!(!second.breakpoints().contains(1));

    first.reset();
    assert_eq!(second.machine(), &untouched);
    assert_eq!(second.last_stop(), None);
    assert!(second.breakpoints().is_empty());
}

#[test]
fn reads_do_not_change_the_session() {
    let session = session_from("+[-]", b"ab", MachineConfig::with_max_steps(10));
    let before = session.machine().clone();

    let _ = session.snapshot();
    let _ = session.machine().tape();
    let _ = session.machine().output();
    let _ = session.last_stop();
    let _ = session.breakpoints();
    let _ = session.tape_window(DEFAULT_WINDOW_RADIUS);
    let _ = session.state_view(DEFAULT_WINDOW_RADIUS);

    assert_eq!(session.machine(), &before);
    assert_eq!(session.last_stop(), None);
}

#[test]
fn reset_restores_the_creation_state_and_keeps_breakpoints() {
    let source = "+[-]";
    let input = b"ab";
    let config = MachineConfig::with_max_steps(10);
    let mut session = session_from(source, input, config);
    session.breakpoints_mut().insert(2).expect("2 は有効な添字");
    let created = session_from(source, input, config).machine().clone();

    session.reset();

    assert_eq!(session.machine(), &created);
    assert_eq!(session.last_stop(), None);
    assert!(session.breakpoints().contains(2));
    assert_eq!(session.breakpoints().len(), 1);
}

#[test]
fn breakpoints_can_be_edited_before_any_run() {
    let mut session = session_from("+.+.", b"", MachineConfig::unlimited());
    assert_eq!(session.breakpoints_mut().insert(3), Ok(true));
    assert_eq!(session.breakpoints_mut().insert(3), Ok(false));
    assert!(session.breakpoints_mut().insert(4).is_err());
    assert_eq!(session.breakpoints().iter().collect::<Vec<_>>(), vec![3]);
}

// ---- Step Into(タスク 3.2) ----

#[test]
fn step_into_executes_exactly_one_instruction_ignoring_breakpoints() {
    let mut session = session_from("+>+", b"", MachineConfig::unlimited());
    session.breakpoints_mut().insert(0).expect("0 は有効な添字");
    let before = session.snapshot();

    assert_eq!(session.step_into(), Ok(StepResult::Executed));

    let after = session.snapshot();
    assert_eq!(after.steps_executed, before.steps_executed + 1);
    assert_eq!(after.pc, before.pc + 1);
    assert_eq!(after.data_pointer, before.data_pointer);
    assert_eq!(before.current_cell, 0);
    assert_eq!(after.current_cell, 1);
    assert!(!after.halted);
    assert_eq!(session.last_stop(), None);
}

#[test]
fn step_into_effects_are_observable_per_instruction() {
    let mut session = session_from(">+<", b"", MachineConfig::unlimited());

    assert_eq!(session.step_into(), Ok(StepResult::Executed));
    let moved = session.snapshot();
    assert_eq!(
        (moved.pc, moved.data_pointer, moved.steps_executed),
        (1, 1, 1)
    );

    assert_eq!(session.step_into(), Ok(StepResult::Executed));
    let incremented = session.snapshot();
    assert_eq!((incremented.pc, incremented.current_cell), (2, 1));

    assert_eq!(session.step_into(), Ok(StepResult::Executed));
    let back = session.snapshot();
    assert_eq!((back.pc, back.data_pointer, back.current_cell), (3, 0, 0));
    assert_eq!(back.steps_executed, 3);
    assert!(back.halted);
}

#[test]
fn step_into_reaching_the_end_reports_halted() {
    let mut session = session_from("+", b"", MachineConfig::unlimited());

    assert_eq!(session.step_into(), Ok(StepResult::Executed));

    assert!(session.snapshot().halted);
    assert_eq!(session.last_stop(), Some(StopReason::Halted));
}

#[test]
fn step_into_without_reaching_the_end_clears_last_stop() {
    let mut session = session_from("+>", b"", MachineConfig::unlimited());

    assert_eq!(session.step_into(), Ok(StepResult::Executed));

    assert!(!session.snapshot().halted);
    assert_eq!(session.last_stop(), None);
}

#[test]
fn step_into_on_halted_session_leaves_state_and_reports_halted() {
    let mut session = session_from("+", b"", MachineConfig::unlimited());
    session.step_into().expect("1 命令目で停止する");
    let before = session.machine().clone();

    assert_eq!(session.step_into(), Ok(StepResult::Halted));

    assert_eq!(session.machine(), &before);
    assert_eq!(session.last_stop(), Some(StopReason::Halted));
}

#[test]
fn step_into_pointer_out_of_bounds_is_an_error_and_leaves_machine_unchanged() {
    let mut session = session_from("+<", b"", MachineConfig::unlimited());
    session.step_into().expect("+ は成功する");
    let before = session.machine().clone();

    let error = session.step_into().unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::PointerOutOfBounds { pc: 1, dp: 0, .. }
    ));
    assert_eq!(session.machine(), &before);
    assert_eq!(session.last_stop(), None);
}

#[test]
fn step_into_step_limit_exceeded_is_an_error_and_leaves_machine_unchanged() {
    let mut session = session_from("+>+", b"", MachineConfig::with_max_steps(1));
    session.step_into().expect("1 命令目は上限内");
    let before = session.machine().clone();

    let error = session.step_into().unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::StepLimitExceeded { limit: 1, .. }
    ));
    assert_eq!(session.machine(), &before);
    assert_eq!(session.last_stop(), None);
}

#[test]
fn reset_after_step_into_restores_creation_state_and_keeps_breakpoints() {
    let source = "+>-";
    let config = MachineConfig::unlimited();
    let mut session = session_from(source, b"", config);
    session.breakpoints_mut().insert(2).expect("2 は有効な添字");
    let created = session_from(source, b"", config).machine().clone();
    for _ in 0..3 {
        session.step_into().expect("3 命令で停止する");
    }
    assert_eq!(session.last_stop(), Some(StopReason::Halted));

    session.reset();

    assert_eq!(session.machine(), &created);
    assert_eq!(session.last_stop(), None);
    assert!(session.breakpoints().contains(2));
    assert_eq!(session.step_into(), Ok(StepResult::Executed));
    assert_eq!(session.snapshot().pc, 1);
}

// ---- バッチ実行(タスク 3.3) ----

fn batch(n: u64) -> BatchSize {
    BatchSize::new(n).expect("テストのバッチ命令数は 1 以上")
}

#[test]
fn batch_reports_halted_when_the_last_instruction_of_the_batch_halts() {
    let mut session = session_from("+", b"", MachineConfig::unlimited());

    let report = session.run_batch(batch(1), &NeverCancel);

    assert_eq!(report.stop, Ok(StopReason::Halted));
    assert_eq!(report.executed, 1);
    assert!(report.snapshot.halted);
    assert_eq!(session.last_stop(), Some(StopReason::Halted));
}

#[test]
fn batch_reports_halted_when_halting_coincides_with_the_batch_width() {
    let mut session = session_from("+>+", b"", MachineConfig::unlimited());

    let report = session.run_batch(batch(3), &NeverCancel);

    assert_eq!(report.stop, Ok(StopReason::Halted));
    assert_eq!(report.executed, 3);
    assert_eq!(report.snapshot.pc, 3);
}

#[test]
fn batch_of_exact_width_without_halting_reports_step_limit_reached() {
    let mut session = session_from("+>+>+>", b"", MachineConfig::unlimited());

    let report = session.run_batch(batch(3), &NeverCancel);

    assert_eq!(report.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(report.executed, 3);
    assert_eq!(report.snapshot.pc, 3);
    assert!(!report.snapshot.halted);
    assert_eq!(session.last_stop(), Some(StopReason::StepLimitReached));
}

#[test]
fn batch_one_instruction_short_of_halting_reports_step_limit_reached() {
    let mut session = session_from("+>+", b"", MachineConfig::unlimited());

    let first = session.run_batch(batch(2), &NeverCancel);
    assert_eq!(first.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(first.executed, 2);
    assert!(!first.snapshot.halted);

    let second = session.run_batch(batch(1), &NeverCancel);
    assert_eq!(second.stop, Ok(StopReason::Halted));
    assert_eq!(second.executed, 1);
    assert_eq!(second.snapshot.steps_executed, 3);
}

#[test]
fn halted_session_reports_halted_without_executing_and_is_idempotent() {
    let mut session = session_from("+", b"", MachineConfig::unlimited());
    session.run_batch(batch(1), &NeverCancel);
    let before = session.machine().clone();

    for _ in 0..2 {
        let report = session.run_batch(batch(5), &NeverCancel);
        assert_eq!(report.stop, Ok(StopReason::Halted));
        assert_eq!(report.executed, 0);
        assert_eq!(report.snapshot, before.snapshot());
        assert_eq!(session.machine(), &before);
        assert_eq!(session.last_stop(), Some(StopReason::Halted));
    }
}

#[test]
fn report_snapshot_is_the_machine_snapshot_at_notification() {
    let mut session = session_from("+>++", b"", MachineConfig::unlimited());

    let report = session.run_batch(batch(3), &NeverCancel);

    assert_eq!(report.snapshot, session.snapshot());
    assert_eq!(report.snapshot.steps_executed, 3);
    assert_eq!(report.snapshot.data_pointer, 1);
    assert_eq!(report.snapshot.current_cell, 1);
}

#[test]
fn executed_counts_one_per_instruction_and_not_per_batch_call() {
    // 命令は 5 個。幅 2 のバッチは 3 回呼ばれ、実行数の合計は命令数 5 と一致する。
    let mut session = session_from("++>+<", b"", MachineConfig::unlimited());
    let mut total_executed = 0u64;
    let mut calls = 0usize;

    loop {
        let report = session.run_batch(batch(2), &NeverCancel);
        calls += 1;
        total_executed += report.executed;
        assert_eq!(total_executed, report.snapshot.steps_executed);
        if report.stop != Ok(StopReason::StepLimitReached) {
            assert_eq!(report.stop, Ok(StopReason::Halted));
            break;
        }
        assert_eq!(report.executed, 2);
    }

    assert_eq!(calls, 3);
    assert_eq!(total_executed, 5);
    assert_eq!(session.snapshot().steps_executed, 5);
}

#[test]
fn step_limit_exceeded_is_an_error_and_repeats_the_same_error_without_change() {
    // `+[]` は無限ループ。上限 3 では 3 命令実行後の 4 命令目の要求がステップ制限超過になる。
    let mut session = session_from("+[]", b"", MachineConfig::with_max_steps(3));
    // 先に `StepLimitReached` で終わらせて `last_stop` を `Some` にし、エラーで消去されることを確認する。
    let warm = session.run_batch(batch(1), &NeverCancel);
    assert_eq!(warm.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(session.last_stop(), Some(StopReason::StepLimitReached));

    let first = session.run_batch(batch(10), &NeverCancel);
    assert!(matches!(
        first.stop,
        Err(RuntimeError::StepLimitExceeded { limit: 3, .. })
    ));
    assert_eq!(first.executed, 2);
    assert_eq!(first.snapshot.steps_executed, 3);
    assert!(!first.snapshot.halted);
    assert_eq!(session.last_stop(), None);
    let before = session.machine().clone();

    let second = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(second.stop, first.stop);
    assert_eq!(second.executed, 0);
    assert_eq!(second.snapshot, first.snapshot);
    assert_eq!(session.machine(), &before);
    assert_eq!(session.last_stop(), None);
}

#[test]
fn pointer_out_of_bounds_is_an_error_with_executed_count_and_clears_last_stop() {
    // `+` が 1 命令実行された後、`<` で dp が範囲外になる。
    let mut session = session_from("+<+", b"", MachineConfig::unlimited());
    let warm = session.run_batch(batch(1), &NeverCancel);
    assert_eq!(warm.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(session.last_stop(), Some(StopReason::StepLimitReached));

    let report = session.run_batch(batch(10), &NeverCancel);

    assert!(matches!(
        report.stop,
        Err(RuntimeError::PointerOutOfBounds { pc: 1, dp: 0, .. })
    ));
    assert_eq!(report.executed, 0);
    assert_eq!(report.snapshot.pc, 1);
    assert_eq!(report.snapshot.data_pointer, 0);
    assert_eq!(report.snapshot.current_cell, 1);
    assert_eq!(report.snapshot.steps_executed, 1);
    assert_eq!(session.last_stop(), None);
    let before = session.machine().clone();

    let again = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(again.stop, report.stop);
    assert_eq!(again.executed, 0);
    assert_eq!(session.machine(), &before);
}

#[test]
fn already_active_cancel_reports_cancelled_without_executing() {
    let mut session = session_from("+>+", b"", MachineConfig::unlimited());
    let before = session.machine().clone();
    let cancel = AtomicBool::new(true);

    let report = session.run_batch(batch(5), &cancel);

    assert_eq!(report.stop, Ok(StopReason::Cancelled));
    assert_eq!(report.executed, 0);
    assert_eq!(session.machine(), &before);
    assert_eq!(session.last_stop(), Some(StopReason::Cancelled));
}

#[test]
fn halted_session_reports_halted_even_when_cancel_is_active() {
    let mut session = session_from("+", b"", MachineConfig::unlimited());
    session.run_batch(batch(1), &NeverCancel);
    let before = session.machine().clone();
    let cancel = AtomicBool::new(true);

    let report = session.run_batch(batch(5), &cancel);

    assert_eq!(report.stop, Ok(StopReason::Halted));
    assert_eq!(report.executed, 0);
    assert_eq!(session.machine(), &before);
}

#[test]
fn cancel_observed_before_an_instruction_stops_before_executing_it() {
    // 2 回の確認まで偽、3 回目の確認(3 命令目の直前)から真。
    let mut session = session_from("+>+>+>", b"", MachineConfig::unlimited());

    let report = session.run_batch(batch(10), &cancel_after(2));

    assert_eq!(report.stop, Ok(StopReason::Cancelled));
    assert_eq!(report.executed, 2);
    assert_eq!(report.snapshot.pc, 2);
    assert_eq!(report.snapshot.steps_executed, 2);
    assert_eq!(session.last_stop(), Some(StopReason::Cancelled));
}

#[test]
fn cancel_at_the_batch_width_boundary_reports_cancelled_before_width() {
    // 2 回の確認まで偽、3 回目の確認は幅 2 に達したちょうどの位置で真になる。
    // キャンセルの判定は幅の判定より前なので `StepLimitReached` ではなく `Cancelled` になる。
    let mut session = session_from("+>+>+>", b"", MachineConfig::unlimited());

    let report = session.run_batch(batch(2), &cancel_after(2));

    assert_eq!(report.stop, Ok(StopReason::Cancelled));
    assert_eq!(report.executed, 2);
    assert_eq!(report.snapshot.pc, 2);
    assert_eq!(report.snapshot.steps_executed, 2);
    assert_eq!(session.last_stop(), Some(StopReason::Cancelled));
}

#[test]
fn cancel_is_checked_once_per_instruction_plus_once_at_the_width_boundary() {
    // 幅 3 で 3 命令を実行し、4 回目の反復で幅に達して止まる。確認は命令ごとに 1 回 + 幅到達時の 1 回。
    let mut session = session_from("+>+>+>", b"", MachineConfig::unlimited());
    let calls = Cell::new(0usize);
    let counting = CancelFn(|| {
        calls.set(calls.get() + 1);
        false
    });

    let report = session.run_batch(batch(3), &counting);

    assert_eq!(report.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(report.executed, 3);
    assert_eq!(calls.get(), report.executed as usize + 1);
}

#[test]
fn step_into_pointer_out_of_bounds_after_step_limit_batch_clears_last_stop() {
    let mut session = session_from("+<", b"", MachineConfig::unlimited());
    let warm = session.run_batch(batch(1), &NeverCancel);
    assert_eq!(warm.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(session.last_stop(), Some(StopReason::StepLimitReached));

    assert!(matches!(
        session.step_into(),
        Err(RuntimeError::PointerOutOfBounds { pc: 1, dp: 0, .. })
    ));
    assert_eq!(session.last_stop(), None);
}

#[test]
fn step_into_step_limit_exceeded_after_step_limit_batch_clears_last_stop() {
    // `+[]` は上限 3 で、3 命令実行後の要求は幅で止まる。続く step_into の 4 命令目がステップ制限超過になる。
    let mut session = session_from("+[]", b"", MachineConfig::with_max_steps(3));
    let warm = session.run_batch(batch(3), &NeverCancel);
    assert_eq!(warm.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(session.last_stop(), Some(StopReason::StepLimitReached));

    assert!(matches!(
        session.step_into(),
        Err(RuntimeError::StepLimitExceeded { limit: 3, .. })
    ));
    assert_eq!(session.last_stop(), None);
}

#[test]
fn step_into_pointer_out_of_bounds_after_cancelled_batch_clears_last_stop() {
    let mut session = session_from("+<", b"", MachineConfig::unlimited());
    let warm = session.run_batch(batch(5), &cancel_after(1));
    assert_eq!(warm.stop, Ok(StopReason::Cancelled));
    assert_eq!(session.last_stop(), Some(StopReason::Cancelled));

    assert!(matches!(
        session.step_into(),
        Err(RuntimeError::PointerOutOfBounds { pc: 1, dp: 0, .. })
    ));
    assert_eq!(session.last_stop(), None);
}
