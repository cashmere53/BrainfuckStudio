//! 命令の遷移テスト。
//!
//! 本ファイルでは加算・減算・ポインタ移動・入出力を扱う。ループ命令を
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

#[test]
fn move_right_increments_dp_and_advances_pc_without_changing_cells() {
    let mut machine = machine_from(">", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.data_pointer, 1);
    assert_eq!(snapshot.current_cell, 0);
    assert_eq!(snapshot.pc, 1);
    assert_eq!(snapshot.steps_executed, 1);
    assert!(machine.tape().iter().all(|&cell| cell == 0));
}

#[test]
fn move_left_decrements_dp_and_advances_pc_and_keeps_other_cells() {
    let mut machine = machine_from(">+<", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().data_pointer, 1);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.data_pointer, 0);
    assert_eq!(snapshot.current_cell, 0);
    assert_eq!(snapshot.pc, 3);
    assert_eq!(snapshot.steps_executed, 3);
    assert_eq!(machine.tape()[1], 1, "移動先のセルの値は保持される");
}

#[test]
fn output_appends_current_cell_byte_in_execution_order() {
    let mut machine = machine_from("++.+.", b"", MachineConfig::unlimited());
    for _ in 0..5 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert_eq!(machine.output(), &[2, 3]);
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.current_cell, 3);
    assert_eq!(snapshot.pc, 5);
    assert_eq!(snapshot.steps_executed, 5);
}

#[test]
fn output_does_not_change_cell_pointer_or_tape() {
    let mut machine = machine_from("+++.", b"", MachineConfig::unlimited());
    for _ in 0..3 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    let before = machine.clone();
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.output(), &[3]);
    assert_eq!(machine.snapshot().current_cell, 3);
    assert_eq!(machine.snapshot().data_pointer, 0);
    assert_eq!(machine.snapshot().pc, 4);
    assert_eq!(machine.snapshot().steps_executed, 4);
    assert_eq!(machine.tape(), before.tape(), "出力はテープを変更しない");
    assert_eq!(machine.input_position(), 0);
}

#[test]
fn output_keeps_non_utf8_bytes_unchanged() {
    // 0x80 は 128 回の加算、続けて 127 回の加算で 0xFF を作る。
    let source = format!("{}.{}.", "+".repeat(128), "+".repeat(127));
    let mut machine = machine_from(&source, b"", MachineConfig::unlimited());
    for _ in 0..(128 + 1 + 127 + 1) {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert_eq!(machine.output(), &[0x80, 0xFF]);
    assert!(std::str::from_utf8(machine.output()).is_err());
}

#[test]
fn output_preserves_every_byte_value_in_order() {
    // 0x00 から 0xFF までの全値を順に出力する。
    let source = format!(".{}", "+.".repeat(255));
    let mut machine = machine_from(&source, b"", MachineConfig::unlimited());
    let mut steps = 0u64;
    while !machine.is_halted() {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
        steps += 1;
    }
    let expected: Vec<u8> = (0..=255u8).collect();
    assert_eq!(machine.output(), expected.as_slice());
    assert_eq!(machine.snapshot().steps_executed, steps);
}

#[test]
fn input_stores_next_byte_in_current_cell_and_advances_position() {
    let mut machine = machine_from(",", b"A", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.current_cell, b'A');
    assert_eq!(snapshot.pc, 1);
    assert_eq!(snapshot.data_pointer, 0);
    assert_eq!(snapshot.steps_executed, 1);
    assert_eq!(machine.input_position(), 1);
    assert!(machine.output().is_empty());
}

#[test]
fn input_reads_bytes_in_order_including_non_utf8_values() {
    let mut machine = machine_from(",>,>,", &[0xFF, 0x00, 0x7F], MachineConfig::unlimited());
    for _ in 0..5 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert_eq!(machine.tape()[0], 0xFF);
    assert_eq!(machine.tape()[1], 0x00);
    assert_eq!(machine.tape()[2], 0x7F);
    assert_eq!(machine.input_position(), 3);
    assert_eq!(machine.snapshot().steps_executed, 5);
}

#[test]
fn input_at_eof_stores_zero_without_error_and_keeps_position() {
    let mut machine = machine_from(",", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.current_cell, 0);
    assert_eq!(snapshot.pc, 1);
    assert_eq!(snapshot.steps_executed, 1);
    assert_eq!(machine.input_position(), 0);
}

#[test]
fn input_at_eof_overwrites_nonzero_cell_with_zero() {
    let mut machine = machine_from("+++,", b"", MachineConfig::unlimited());
    for _ in 0..3 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert_eq!(machine.snapshot().current_cell, 3);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().current_cell, 0);
    assert_eq!(machine.input_position(), 0);
}

#[test]
fn repeated_input_after_exhaustion_yields_zero_and_does_not_move_position() {
    let mut machine = machine_from(",,", b"A", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().current_cell, b'A');
    assert_eq!(machine.input_position(), 1);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(
        machine.snapshot().current_cell,
        0,
        "EOF では非ゼロのセルも 0 になる"
    );
    assert_eq!(machine.input_position(), 1, "EOF では読取位置を進めない");
    assert_eq!(machine.snapshot().steps_executed, 2);
}

#[test]
fn input_changes_only_the_current_cell() {
    let mut machine = machine_from(">+<,", b"Z", MachineConfig::unlimited());
    for _ in 0..4 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    let tape = machine.tape();
    assert_eq!(tape[0], b'Z');
    assert_eq!(tape[1], 1, "移動先のセルは変更されない");
    assert!(tape[2..].iter().all(|&cell| cell == 0));
    assert!(machine.output().is_empty());
}
