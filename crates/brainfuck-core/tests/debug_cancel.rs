//! キャンセル指示(`CancelSignal`)の単体テスト。
//!
//! 常に偽の実装、共有される真偽値、参照・共有ポインタ・動的ディスパッチ経由の応答、
//! クロージャの呼び出し回数に応じた応答を確認する。別スレッドは使わず決定的に検証する。
//!
//! 後半は `DebugSession::run_batch` の中断・再開・停止優先・ブレークポイントとの同時該当を、
//! 呼び出し回数で決定的に再現して検証する(タスク 4.1)。

mod debug_common;

use brainfuck_core::{
    BatchSize, CancelFn, CancelSignal, DebugSession, MachineConfig, NeverCancel, StopReason,
};
use debug_common::{assert_same_final_state, cancel_after, machine_from, session_from};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// 任意の `CancelSignal` 実装の現在の応答を返す(検証用の局所ヘルパ)。
fn answer<C: CancelSignal + ?Sized>(signal: &C) -> bool {
    signal.is_cancelled()
}

#[test]
fn never_cancel_always_answers_false() {
    let signal = NeverCancel;
    for _ in 0..100 {
        assert!(!answer(&signal));
    }
    assert!(!NeverCancel.is_cancelled());
}

#[test]
fn atomic_bool_follows_the_shared_flag_without_becoming_sticky() {
    let flag = AtomicBool::new(false);
    assert!(!answer(&flag));

    flag.store(true, Ordering::Release);
    assert!(answer(&flag));
    assert!(answer(&flag), "指示が有効な間は繰り返し真を返す");

    // 指示の取り下げ(偽への書き戻し)は次の確認で反映される。
    flag.store(false, Ordering::SeqCst);
    assert!(!answer(&flag));
}

#[test]
fn reference_to_atomic_bool_delegates() {
    let flag = AtomicBool::new(false);
    let by_ref: &AtomicBool = &flag;
    let by_double_ref: &&AtomicBool = &by_ref;

    assert!(!answer(by_ref));
    assert!(!answer(by_double_ref));

    flag.store(true, Ordering::Release);
    assert!(answer(by_ref));
    assert!(answer(by_double_ref));
}

#[test]
fn arc_shares_the_flag_between_clones() {
    let flag = Arc::new(AtomicBool::new(false));
    let controller = Arc::clone(&flag);
    let observer = Arc::clone(&flag);

    assert!(!answer(&observer));

    controller.store(true, Ordering::Release);
    assert!(answer(&observer), "設定側の書き込みが観測側に見える");

    controller.store(false, Ordering::SeqCst);
    assert!(!answer(&observer));
}

#[test]
fn arc_of_trait_object_delegates() {
    let flag = Arc::new(AtomicBool::new(false));
    let shared: Arc<dyn CancelSignal> = Arc::new(Arc::clone(&flag));

    assert!(!answer(shared.as_ref()));
    assert!(!answer(&shared));

    flag.store(true, Ordering::Release);
    assert!(answer(shared.as_ref()));
    assert!(answer(&shared));
}

#[test]
fn trait_object_reference_delegates() {
    let flag = AtomicBool::new(true);
    let dynamic: &dyn CancelSignal = &flag;
    assert!(answer(dynamic));

    let never: &dyn CancelSignal = &NeverCancel;
    assert!(!answer(never));
}

#[test]
fn closure_wrapper_answers_by_call_count() {
    // 3 回目の確認から有効になる指示を、呼び出し回数だけで決定的に再現する。
    let calls = Cell::new(0_usize);
    let signal = CancelFn(|| {
        let n = calls.get() + 1;
        calls.set(n);
        n >= 3
    });

    assert!(!answer(&signal));
    assert!(!answer(&signal));
    assert!(answer(&signal));
    assert!(answer(&signal));
    assert_eq!(calls.get(), 4, "確認ごとにちょうど 1 回呼ばれる");
}

#[test]
fn closure_wrapper_can_be_shared_behind_arc_and_reference() {
    let calls = AtomicUsize::new(0);
    let signal = Arc::new(CancelFn(|| {
        calls.fetch_add(1, Ordering::SeqCst);
        false
    }));

    assert!(!answer(&signal));
    assert!(!answer(&*signal));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn closure_wrapper_over_shared_flag_reads_the_flag_each_time() {
    let flag = AtomicBool::new(false);
    let signal = CancelFn(|| flag.load(Ordering::Acquire));

    assert!(!answer(&signal));
    flag.store(true, Ordering::Release);
    assert!(answer(&signal));
}

// ---- バッチ実行中のキャンセル(タスク 4.1) ----

/// バッチ命令数を作る(テストは常に 1 以上を指定する)。
fn batch(n: u64) -> BatchSize {
    BatchSize::new(n).expect("テストのバッチ命令数は 1 以上")
}

/// 有限に停止するプログラム。出力と入力読取を含む。
const FINITE: &str = "++[>+<-]>.,.";

/// 停止しないプログラム(現在セルが非 0 のまま無限ループする)。途中のキャンセルの検証に用いる。
const ENDLESS: &str = "+[]";

#[test]
fn cancel_after_n_confirmations_interrupts_after_exactly_n_instructions() {
    for n in [0_usize, 1, 2, 5, 100] {
        let mut session = session_from(ENDLESS, b"", MachineConfig::unlimited());

        let report = session.run_batch(batch(1_000), &cancel_after(n));

        assert_eq!(report.stop, Ok(StopReason::Cancelled), "n = {n}");
        assert_eq!(report.executed, n as u64, "n = {n}");
        assert_eq!(report.snapshot.steps_executed, report.executed, "n = {n}");
        assert_eq!(session.last_stop(), Some(StopReason::Cancelled), "n = {n}");
    }
}

#[test]
fn cancel_is_queried_once_before_each_instruction_and_not_executed_after_observed() {
    let mut session = session_from(ENDLESS, b"", MachineConfig::unlimited());
    let calls = Cell::new(0_usize);
    let signal = CancelFn(|| {
        let seen = calls.get() + 1;
        calls.set(seen);
        seen > 4
    });

    let report = session.run_batch(batch(1_000), &signal);

    assert_eq!(report.stop, Ok(StopReason::Cancelled));
    assert_eq!(report.executed, 4);
    assert_eq!(
        calls.get(),
        5,
        "命令ごとに 1 回ずつ、観測した確認も含めて 5 回"
    );
}

#[test]
fn interrupted_state_equals_the_reference_after_exactly_the_observed_instructions() {
    for n in [1_usize, 4, 9] {
        let mut session = session_from(ENDLESS, b"", MachineConfig::unlimited());

        let report = session.run_batch(batch(1_000), &cancel_after(n));
        assert_eq!(report.stop, Ok(StopReason::Cancelled), "n = {n}");

        // 参照機械を同じ命令数だけ core の step() で進めた状態と一致する。
        let mut reference = machine_from(ENDLESS, b"", MachineConfig::unlimited());
        for _ in 0..n {
            reference
                .step()
                .expect("参照機械は無限ループのため実行エラーにならない");
        }
        assert_same_final_state(session.machine(), &reference);
        assert_eq!(session.machine(), &reference, "n = {n}");
        assert_eq!(report.snapshot, reference.snapshot(), "n = {n}");
        assert!(
            report.snapshot.pc < session.machine().program().len(),
            "pc は命令境界(プログラム長未満)に留まる"
        );
    }
}

#[test]
fn resuming_after_cancel_reaches_the_same_final_result_as_an_uninterrupted_run() {
    let mut uninterrupted = session_from(FINITE, b"x", MachineConfig::unlimited());
    let full = uninterrupted.run_batch(batch(1_000), &NeverCancel);
    assert_eq!(full.stop, Ok(StopReason::Halted));

    for n in [0_usize, 1, 3, 6] {
        let mut session = session_from(FINITE, b"x", MachineConfig::unlimited());

        let first = session.run_batch(batch(1_000), &cancel_after(n));
        assert_eq!(first.stop, Ok(StopReason::Cancelled), "n = {n}");
        assert_eq!(first.executed, n as u64, "n = {n}");

        // 指示が無効になった状態で再開する。
        let rest = session.run_batch(batch(1_000), &NeverCancel);
        assert_eq!(rest.stop, Ok(StopReason::Halted), "n = {n}");
        assert_eq!(first.executed + rest.executed, full.executed, "n = {n}");
        assert_eq!(rest.snapshot, full.snapshot, "n = {n}");
        assert_same_final_state(session.machine(), uninterrupted.machine());
    }
}

#[test]
fn cancel_then_resume_in_small_batches_matches_the_uninterrupted_final_state() {
    let mut uninterrupted = session_from(FINITE, b"x", MachineConfig::unlimited());
    let full = uninterrupted.run_batch(batch(1_000), &NeverCancel);

    let mut session = session_from(FINITE, b"x", MachineConfig::unlimited());
    let interrupted = session.run_batch(batch(1_000), &cancel_after(3));
    assert_eq!(interrupted.stop, Ok(StopReason::Cancelled));

    let mut total = interrupted.executed;
    let mut last = None;
    for _ in 0..1_000 {
        let report = session.run_batch(batch(2), &NeverCancel);
        total += report.executed;
        last = Some(report.stop);
        if report.stop == Ok(StopReason::Halted) {
            break;
        }
    }

    assert_eq!(last, Some(Ok(StopReason::Halted)));
    assert_eq!(total, full.executed);
    assert_same_final_state(session.machine(), uninterrupted.machine());
}

#[test]
fn shared_flag_switches_cancel_between_batches_without_becoming_sticky() {
    let mut session = session_from(ENDLESS, b"", MachineConfig::unlimited());
    let flag = AtomicBool::new(true);

    let cancelled = session.run_batch(batch(10), &flag);
    assert_eq!(cancelled.stop, Ok(StopReason::Cancelled));
    assert_eq!(cancelled.executed, 0);

    flag.store(false, Ordering::SeqCst);
    let running = session.run_batch(batch(10), &flag);
    assert_eq!(running.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(running.executed, 10);

    flag.store(true, Ordering::Release);
    let cancelled_again = session.run_batch(batch(10), &flag);
    assert_eq!(cancelled_again.stop, Ok(StopReason::Cancelled));
    assert_eq!(cancelled_again.executed, 0);
    assert_eq!(
        cancelled_again.snapshot, running.snapshot,
        "中断中は状態を変えない"
    );

    flag.store(false, Ordering::SeqCst);
    let resumed = session.run_batch(batch(10), &flag);
    assert_eq!(resumed.stop, Ok(StopReason::StepLimitReached));
    assert_eq!(resumed.executed, 10);
    assert_eq!(resumed.snapshot.steps_executed, 20);
}

#[test]
fn halted_session_reports_halted_whatever_the_cancel_signal_says() {
    let mut session = session_from(FINITE, b"x", MachineConfig::unlimited());
    assert_eq!(
        session.run_batch(batch(1_000), &NeverCancel).stop,
        Ok(StopReason::Halted)
    );
    let before = session.machine().clone();
    let active = AtomicBool::new(true);

    let always = session.run_batch(batch(1_000), &CancelFn(|| true));
    assert_eq!(always.stop, Ok(StopReason::Halted));
    assert_eq!(always.executed, 0);

    let shared = session.run_batch(batch(1), &active);
    assert_eq!(shared.stop, Ok(StopReason::Halted));
    assert_eq!(shared.executed, 0);

    let counted = session.run_batch(batch(1_000), &cancel_after(0));
    assert_eq!(counted.stop, Ok(StopReason::Halted));
    assert_eq!(counted.executed, 0);

    assert_eq!(session.machine(), &before, "停止済みでは状態を変えない");
    assert_eq!(session.last_stop(), Some(StopReason::Halted));
}

#[test]
fn cancel_and_breakpoint_at_the_same_position_report_cancelled_then_breakpoint() {
    let mut session = session_from(ENDLESS, b"", MachineConfig::unlimited());
    session
        .breakpoints_mut()
        .insert(0)
        .expect("添字 0 はプログラム内の命令");
    let before = session.machine().clone();

    let cancelled = session.run_batch(batch(10), &CancelFn(|| true));
    assert_eq!(cancelled.stop, Ok(StopReason::Cancelled));
    assert_eq!(cancelled.executed, 0);
    assert_eq!(session.machine(), &before);

    let breakpoint = session.run_batch(batch(10), &NeverCancel);
    assert_eq!(breakpoint.stop, Ok(StopReason::Breakpoint));
    assert_eq!(breakpoint.executed, 0);
    assert_eq!(session.machine(), &before);
}

#[test]
fn batch_without_a_cancel_signal_is_never_cancelled() {
    let mut session = session_from(FINITE, b"x", MachineConfig::unlimited());

    let report = session.run_batch(batch(1_000), &NeverCancel);

    assert_eq!(report.stop, Ok(StopReason::Halted));
    assert_eq!(session.last_stop(), Some(StopReason::Halted));
}

/// `Breakpoint` で止まりながら `Halted` になるまで、キャンセル無しのバッチ実行を繰り返す。
/// 戻り値は停止までに実行した命令数の合計。
fn drain_to_halt(session: &mut DebugSession) -> u64 {
    let mut total = 0;
    for _ in 0..10_000 {
        let report = session.run_batch(batch(1_000), &NeverCancel);
        total += report.executed;
        match report.stop {
            Ok(StopReason::Halted) => return total,
            Ok(StopReason::Breakpoint) | Ok(StopReason::StepLimitReached) => {}
            other => panic!("想定外の停止: {other:?}"),
        }
    }
    panic!("停止に到達しない");
}

#[test]
fn cancel_while_stopped_at_breakpoint_keeps_state_and_resume_moves_forward() {
    // 添字 4 はループ本体の `+`。反復ごとに停止し、再開すれば前進できる位置。
    const LOOP: &str = "++[>+<-]";
    const LOOP_BREAKPOINT: usize = 4;

    // 参照: キャンセルを挟まない場合。
    let mut uninterrupted = session_from(LOOP, b"", MachineConfig::unlimited());
    uninterrupted
        .breakpoints_mut()
        .insert(LOOP_BREAKPOINT)
        .expect("添字 4 は命令内");
    let first_reference = uninterrupted.run_batch(batch(1_000), &NeverCancel);
    assert_eq!(first_reference.stop, Ok(StopReason::Breakpoint));
    let rest_reference = drain_to_halt(&mut uninterrupted);

    // 検証対象: 同じ位置で停止した直後にキャンセルし、そのあと再開する。
    let mut session = session_from(LOOP, b"", MachineConfig::unlimited());
    session
        .breakpoints_mut()
        .insert(LOOP_BREAKPOINT)
        .expect("添字 4 は命令内");
    let stopped = session.run_batch(batch(1_000), &NeverCancel);
    assert_eq!(stopped.stop, Ok(StopReason::Breakpoint));
    assert_eq!(stopped.snapshot.pc, LOOP_BREAKPOINT);
    let before = session.machine().clone();

    let cancelled = session.run_batch(batch(1_000), &CancelFn(|| true));
    assert_eq!(cancelled.stop, Ok(StopReason::Cancelled));
    assert_eq!(cancelled.executed, 0);
    assert_eq!(
        cancelled.snapshot, stopped.snapshot,
        "キャンセル中は状態を変えない"
    );
    assert_eq!(session.machine(), &before);
    assert_eq!(session.last_stop(), Some(StopReason::Cancelled));

    // 指示が無効になったあとの再開は、停止位置の命令を実行して前進する。
    let resumed = session.run_batch(batch(1_000), &NeverCancel);
    assert!(
        resumed.executed >= 1,
        "キャンセル後の再開は停止位置で同じ Breakpoint にならず前進する (executed = {})",
        resumed.executed
    );
    let rest = drain_to_halt(&mut session);

    assert_eq!(
        stopped.executed + cancelled.executed + resumed.executed + rest,
        first_reference.executed + rest_reference,
        "総実行数はキャンセルを挟まない場合と一致する"
    );
    assert_eq!(session.machine(), uninterrupted.machine());
    assert_same_final_state(session.machine(), uninterrupted.machine());
}
