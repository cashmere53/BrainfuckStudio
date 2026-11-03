//! 機械の生成直後の状態と観測 API のテスト。
//!
//! 命令の実行(`step` / `run`)は呼び出さない。

mod common;

use brainfuck_core::{MachineConfig, TAPE_SIZE};
use common::{assert_same_state, machine_from};

#[test]
fn tape_length_is_30000_cells() {
    let machine = machine_from("+", b"", MachineConfig::unlimited());
    assert_eq!(TAPE_SIZE, 30_000);
    assert_eq!(machine.tape().len(), 30_000);
}

#[test]
fn initial_tape_cells_are_all_zero() {
    let machine = machine_from("+", b"", MachineConfig::unlimited());
    assert!(machine.tape().iter().all(|&cell| cell == 0));
}

#[test]
fn initial_snapshot_has_zero_pointers_counts_and_cell() {
    let machine = machine_from("+", b"", MachineConfig::unlimited());
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 0);
    assert_eq!(snapshot.data_pointer, 0);
    assert_eq!(snapshot.current_cell, 0);
    assert_eq!(snapshot.steps_executed, 0);
    assert!(!snapshot.halted);
}

#[test]
fn initial_output_is_empty() {
    let machine = machine_from("+.", b"abc", MachineConfig::unlimited());
    assert!(machine.output().is_empty());
}

#[test]
fn initial_input_position_is_zero() {
    let machine = machine_from(",", b"abc", MachineConfig::unlimited());
    assert_eq!(machine.input_position(), 0);
}

#[test]
fn empty_program_is_halted_from_the_start() {
    let machine = machine_from("", b"", MachineConfig::unlimited());
    assert!(machine.is_halted());
    let snapshot = machine.snapshot();
    assert!(snapshot.halted);
    assert_eq!(snapshot.pc, 0);
    assert_eq!(snapshot.steps_executed, 0);
}

#[test]
fn comment_only_program_is_halted_from_the_start() {
    let machine = machine_from("hello world", b"", MachineConfig::unlimited());
    assert!(machine.program().is_empty());
    assert!(machine.snapshot().halted);
}

#[test]
fn non_empty_program_is_not_halted_from_the_start() {
    let machine = machine_from("+", b"", MachineConfig::unlimited());
    assert!(!machine.is_halted());
    assert!(!machine.snapshot().halted);
}

#[test]
fn program_accessor_returns_parsed_program() {
    let machine = machine_from("+-[]", b"", MachineConfig::with_max_steps(10));
    assert_eq!(machine.program().len(), 4);
}

#[test]
fn clone_is_equal_but_owns_separate_state() {
    let original = machine_from("+[-]", b"xy", MachineConfig::with_max_steps(5));
    let copy = original.clone();
    assert_same_state(&original, &copy);
    assert_ne!(original.tape().as_ptr(), copy.tape().as_ptr());
}

#[test]
fn machines_from_same_inputs_are_independent() {
    let first = machine_from(">+<-", b"input", MachineConfig::unlimited());
    let second = machine_from(">+<-", b"input", MachineConfig::unlimited());
    assert_same_state(&first, &second);
    assert_ne!(first.tape().as_ptr(), second.tape().as_ptr());
}
