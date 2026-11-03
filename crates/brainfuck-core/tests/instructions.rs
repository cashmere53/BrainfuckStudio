//! 命令の遷移テスト。
//!
//! 本ファイルでは加算・減算のみを扱う。ポインタ移動・入出力・ループ命令を
//! 実行するプログラムは使わない(後続タスクで扱う)。

mod common;

use brainfuck_core::{MachineConfig, StepResult};
use common::machine_from;

#[test]
fn increment_adds_one_to_the_current_cell_and_advances_pc() {
    let mut machine = machine_from("+", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.current_cell, 1);
    assert_eq!(snapshot.pc, 1);
    assert_eq!(snapshot.data_pointer, 0);
    assert_eq!(snapshot.steps_executed, 1);
}

#[test]
fn decrement_subtracts_one_from_the_current_cell_and_advances_pc() {
    let mut machine = machine_from("++-", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.current_cell, 1);
    assert_eq!(snapshot.pc, 3);
    assert_eq!(snapshot.steps_executed, 3);
}

#[test]
fn increment_wraps_from_255_to_0_without_error() {
    let source = "+".repeat(256);
    let mut machine = machine_from(&source, b"", MachineConfig::unlimited());
    for _ in 0..255 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert_eq!(machine.snapshot().current_cell, 255);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.current_cell, 0);
    assert_eq!(snapshot.steps_executed, 256);
}

#[test]
fn decrement_wraps_from_0_to_255_without_error() {
    let mut machine = machine_from("-", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.current_cell, 255);
    assert_eq!(snapshot.pc, 1);
    assert_eq!(snapshot.steps_executed, 1);
}

#[test]
fn decrement_from_1_reaches_0_then_wraps_to_255() {
    let mut machine = machine_from("+--", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().current_cell, 0);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().current_cell, 255);
}

#[test]
fn arithmetic_changes_only_the_current_cell() {
    let mut machine = machine_from("++", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let tape = machine.tape();
    assert_eq!(tape[0], 2);
    assert!(
        tape[1..].iter().all(|&cell| cell == 0),
        "dp 以外のセルは変更されない"
    );
    assert_eq!(machine.snapshot().data_pointer, 0);
}
