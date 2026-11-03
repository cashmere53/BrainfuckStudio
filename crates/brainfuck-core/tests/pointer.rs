//! ポインタ移動の境界とポインタ範囲外エラーのテスト。
//!
//! 本ファイルでは `>` `<` と `+` `-` のみを使う。入出力・ループ命令を実行するプログラムは使わない。

mod common;

use brainfuck_core::{
    Machine, MachineConfig, PointerDirection, RuntimeError, StepResult, TAPE_SIZE,
};
use common::{assert_same_state, machine_from};

/// データポインタの最大値(`TAPE_SIZE - 1`)。
const LAST_CELL: usize = TAPE_SIZE - 1;

/// 指定回数 `step` を呼び、すべて `Executed` になることを確認する。
fn step_n_times(machine: &mut Machine, count: usize) {
    for _ in 0..count {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
}

#[test]
fn right_move_at_last_cell_returns_pointer_out_of_bounds_with_pc_dp_and_direction() {
    let mut machine = machine_from(&">".repeat(LAST_CELL + 1), b"", MachineConfig::unlimited());
    step_n_times(&mut machine, LAST_CELL);
    assert_eq!(machine.snapshot().data_pointer, LAST_CELL);
    assert_eq!(machine.snapshot().pc, LAST_CELL);

    assert_eq!(
        machine.step(),
        Err(RuntimeError::PointerOutOfBounds {
            pc: LAST_CELL,
            dp: LAST_CELL,
            direction: PointerDirection::Right,
        })
    );
}

#[test]
fn left_move_at_zero_returns_pointer_out_of_bounds_with_pc_dp_and_direction() {
    let mut machine = machine_from("<", b"", MachineConfig::unlimited());
    assert_eq!(
        machine.step(),
        Err(RuntimeError::PointerOutOfBounds {
            pc: 0,
            dp: 0,
            direction: PointerDirection::Left,
        })
    );
}

#[test]
fn left_move_at_zero_after_moves_reports_the_later_pc_and_dp() {
    // `>>+<<<` の 6 個目(添字 5)の `<` は、dp = 0 の状態で範囲外になる。
    let mut machine = machine_from(">>+<<<", b"", MachineConfig::unlimited());
    step_n_times(&mut machine, 5);
    assert_eq!(machine.snapshot().data_pointer, 0);
    assert_eq!(machine.snapshot().pc, 5);

    assert_eq!(
        machine.step(),
        Err(RuntimeError::PointerOutOfBounds {
            pc: 5,
            dp: 0,
            direction: PointerDirection::Left,
        })
    );
}

#[test]
fn right_move_error_leaves_all_state_unchanged() {
    let mut machine = machine_from(&">".repeat(LAST_CELL + 1), b"", MachineConfig::unlimited());
    step_n_times(&mut machine, LAST_CELL);
    let before = machine.clone();

    assert!(machine.step().is_err());
    assert_same_state(&machine, &before);
}

#[test]
fn left_move_error_leaves_all_state_unchanged_including_tape_and_steps() {
    // 途中で `+` によりテープと実行数を非自明な値にしてから範囲外エラーを起こす。
    let mut machine = machine_from(">>+<<<", b"", MachineConfig::unlimited());
    step_n_times(&mut machine, 5);
    let before = machine.clone();

    assert!(machine.step().is_err());
    assert_same_state(&machine, &before);
    assert_eq!(machine.snapshot().steps_executed, 5);
}

#[test]
fn repeated_step_after_out_of_bounds_error_returns_the_same_error_each_time() {
    let mut machine = machine_from("<", b"", MachineConfig::unlimited());
    let before = machine.clone();
    let expected = Err(RuntimeError::PointerOutOfBounds {
        pc: 0,
        dp: 0,
        direction: PointerDirection::Left,
    });

    assert_eq!(machine.step(), expected);
    assert_eq!(machine.step(), expected);
    assert_same_state(&machine, &before);
}

#[test]
fn moving_from_zero_to_last_cell_and_back_succeeds_at_every_step() {
    let source = format!("{}{}", ">".repeat(LAST_CELL), "<".repeat(LAST_CELL));
    let mut machine = machine_from(&source, b"", MachineConfig::unlimited());

    step_n_times(&mut machine, LAST_CELL);
    assert_eq!(machine.snapshot().data_pointer, LAST_CELL);
    assert_eq!(machine.snapshot().pc, LAST_CELL);
    assert_eq!(machine.snapshot().steps_executed, LAST_CELL as u64);

    step_n_times(&mut machine, LAST_CELL);
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.data_pointer, 0);
    assert_eq!(snapshot.steps_executed, 2 * LAST_CELL as u64);
    assert!(snapshot.halted);
}
