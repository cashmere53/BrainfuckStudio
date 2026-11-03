//! 1 ステップ実行の骨格(停止・上限・計数)のテスト。
//!
//! 本ファイルでは加算・減算のみで構成されるプログラムを使う。
//! ポインタ移動・入出力・ループ命令を実行するプログラムは使わない。

mod common;

use brainfuck_core::{Machine, MachineConfig, RuntimeError, StepResult};
use common::{assert_same_state, machine_from};

/// 停止または直前のエラーまで `step` を繰り返し、最後の結果を返す。
///
/// 停止時は `Ok(())`、エラー時はそのエラーを返す。
fn step_until_halted_or_error(machine: &mut Machine) -> Result<(), RuntimeError> {
    loop {
        match machine.step() {
            Ok(StepResult::Executed) => continue,
            Ok(StepResult::Halted) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

#[test]
fn one_step_increases_steps_executed_by_exactly_one() {
    let mut machine = machine_from("++", b"", MachineConfig::unlimited());
    assert_eq!(machine.snapshot().steps_executed, 0);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().steps_executed, 1);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().steps_executed, 2);
}

#[test]
fn comment_characters_are_not_counted_as_steps() {
    // 有効命令は `+`・`-`・`+` の 3 個のみ。
    let mut machine = machine_from("a+ b-\n+ c", b"", MachineConfig::unlimited());
    assert_eq!(machine.program().len(), 3);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.steps_executed, 3);
    assert_eq!(snapshot.pc, 3);
    assert!(snapshot.halted);
    assert_eq!(snapshot.current_cell, 1);
}

#[test]
fn step_after_halt_returns_halted_and_leaves_state_unchanged() {
    let mut machine = machine_from("+", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let before = machine.clone();
    for _ in 0..3 {
        assert_eq!(machine.step(), Ok(StepResult::Halted));
        assert_same_state(&machine, &before);
    }
}

#[test]
fn empty_program_is_halted_and_step_returns_halted() {
    let mut machine = machine_from("", b"", MachineConfig::unlimited());
    let before = machine.clone();
    assert_eq!(machine.step(), Ok(StepResult::Halted));
    assert_same_state(&machine, &before);
    assert_eq!(machine.snapshot().steps_executed, 0);
}

#[test]
fn limit_equal_to_program_length_allows_normal_halt() {
    // N = 3 の上限で N 個の命令は成功し、4 回目のステップは停止を返す。
    let mut machine = machine_from("+-+", b"", MachineConfig::with_max_steps(3));
    assert_eq!(step_until_halted_or_error(&mut machine), Ok(()));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.steps_executed, 3);
    assert_eq!(snapshot.pc, 3);
    assert!(snapshot.halted);
    assert_eq!(machine.step(), Ok(StepResult::Halted));
}

#[test]
fn limit_one_less_than_program_length_fails_at_limit_with_pc() {
    // N = 3 の命令に対して上限 N-1 = 2 では、3 個目の実行前にエラーとなる。
    let mut machine = machine_from("+-+", b"", MachineConfig::with_max_steps(2));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let before = machine.clone();
    let error = machine
        .step()
        .expect_err("上限 N-1 では追加の命令でエラーになる");
    assert_eq!(error, RuntimeError::StepLimitExceeded { limit: 2, pc: 2 });
    assert_same_state(&machine, &before);
    assert_eq!(machine.snapshot().steps_executed, 2);
}

#[test]
fn limit_one_more_than_program_length_allows_normal_halt() {
    // N = 3 の命令に対して上限 N+1 = 4 では正常終了する。
    let mut machine = machine_from("+-+", b"", MachineConfig::with_max_steps(4));
    assert_eq!(step_until_halted_or_error(&mut machine), Ok(()));
    assert_eq!(machine.snapshot().steps_executed, 3);
    assert!(machine.snapshot().halted);
}

#[test]
fn limit_zero_with_empty_program_is_halted() {
    let mut machine = machine_from("", b"", MachineConfig::with_max_steps(0));
    assert_eq!(machine.step(), Ok(StepResult::Halted));
    assert!(machine.snapshot().halted);
    assert_eq!(machine.snapshot().steps_executed, 0);
}

#[test]
fn limit_zero_with_nonempty_program_fails_on_first_step() {
    let mut machine = machine_from("+", b"", MachineConfig::with_max_steps(0));
    let before = machine.clone();
    assert_eq!(
        machine.step(),
        Err(RuntimeError::StepLimitExceeded { limit: 0, pc: 0 })
    );
    assert_same_state(&machine, &before);
    assert_eq!(machine.snapshot().current_cell, 0);
}

#[test]
fn step_after_limit_error_returns_same_error_and_keeps_state() {
    let mut machine = machine_from("++", b"", MachineConfig::with_max_steps(1));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let expected = RuntimeError::StepLimitExceeded { limit: 1, pc: 1 };
    let before = machine.clone();
    assert_eq!(machine.step(), Err(expected));
    assert_same_state(&machine, &before);
    assert_eq!(machine.step(), Err(expected));
    assert_eq!(machine.step(), Err(expected));
    assert_same_state(&machine, &before);
}

#[test]
fn unlimited_config_does_not_stop_by_step_count() {
    let source = "+".repeat(1000);
    let mut machine = machine_from(&source, b"", MachineConfig::unlimited());
    assert_eq!(step_until_halted_or_error(&mut machine), Ok(()));
    assert_eq!(machine.snapshot().steps_executed, 1000);
    assert_eq!(machine.snapshot().current_cell, (1000 % 256) as u8);
}
