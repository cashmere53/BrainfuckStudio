//! デバッグセッション(生成・観測・リセット・バッチ幅)の単体テスト(タスク 3.1)。
//!
//! 生成直後の状態が core の機械と一致すること、読み取りが状態を変えないこと、
//! リセットが初期状態へ戻しブレークポイントを保持すること、セッションが独立していることを確認する。
//! 機械を進めた後のリセットは Step Into 以降のタスクで検証する。

mod debug_common;

use brainfuck_core::{
    BatchSize, DebugSession, Machine, MachineConfig, Program, DEFAULT_BATCH_SIZE,
    DEFAULT_WINDOW_RADIUS,
};
use debug_common::session_from;

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
