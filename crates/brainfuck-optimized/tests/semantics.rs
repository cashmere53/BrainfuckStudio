//! 公開の実行機械(`OptimizedMachine`)の契約を、公開 API のみを使って検証する。
//!
//! 対象: 初期状態(10.1)、完走実行の正常終了(10.2)、停止後の再実行の冪等(7.8, 10.3)、
//! エラー後の再実行の固定(13.x)、同一 IR から作った機械の独立(10.6, 10.7)、
//! 素朴換算の実行数と IR 操作の実行回数の区別(6.2, 6.3, 6.7)、
//! 標準プログラム(Hello World とエコー)の出力と入力の扱い(9.x)。

use brainfuck_core::{
    MachineConfig, PointerDirection, Program, RuntimeError, StopReason, TAPE_SIZE,
};
use brainfuck_optimized::{Op, OptimizedMachine, OptimizedProgram};

const HELLO: &str = include_str!("fixtures/hello.bf");
const ECHO: &str = include_str!("fixtures/echo.bf");

fn optimized(source: &str) -> OptimizedProgram {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    OptimizedProgram::compile(&program)
}

#[test]
fn new_machine_starts_in_the_initial_state() {
    let program = optimized("+.,");
    let machine = OptimizedMachine::new(&program, vec![1, 2], MachineConfig::with_max_steps(10));

    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 0, "pc");
    assert_eq!(snapshot.data_pointer, 0, "data pointer");
    assert_eq!(snapshot.current_cell, 0, "current cell");
    assert_eq!(snapshot.steps_executed, 0, "steps");
    assert_eq!(snapshot.ir_ops_executed, 0, "ir ops");
    assert!(!snapshot.halted, "halted");
    assert_eq!(snapshot.input_position, 0, "input position");
    assert_eq!(machine.tape().len(), TAPE_SIZE);
    assert!(machine.tape().iter().all(|&cell| cell == 0), "tape");
    assert!(machine.output().is_empty(), "output");
    assert_eq!(machine.input_position(), 0);
    assert!(!machine.is_halted());
}

#[test]
fn normal_completion_returns_halted_and_observes_the_final_state() {
    let program = optimized("+++.");
    let mut machine =
        OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(100));

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert!(machine.is_halted());
    assert_eq!(machine.output(), &[3u8][..]);
    assert_eq!(machine.snapshot().pc, program.source().len());
}

#[test]
fn rerunning_a_halted_machine_leaves_its_state_unchanged() {
    let program = optimized("+++++[-].");
    let mut machine =
        OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(1_000));
    assert_eq!(machine.run(), Ok(StopReason::Halted));

    let before = machine.snapshot();
    let tape = machine.tape().to_vec();
    let output = machine.output().to_vec();

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.snapshot(), before);
    assert_eq!(machine.tape(), tape.as_slice());
    assert_eq!(machine.output(), output.as_slice());
    assert!(machine.is_halted());
}

#[test]
fn rerunning_after_an_error_returns_the_same_error_and_keeps_the_state() {
    let program = optimized("+.<<");
    let mut machine =
        OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(100));
    let expected = Err(RuntimeError::PointerOutOfBounds {
        pc: 2,
        dp: 0,
        direction: PointerDirection::Left,
    });

    assert_eq!(machine.run(), expected);
    let before = machine.snapshot();
    let tape = machine.tape().to_vec();
    let output = machine.output().to_vec();

    assert_eq!(machine.run(), expected);
    assert_eq!(machine.snapshot(), before);
    assert_eq!(machine.tape(), tape.as_slice());
    assert_eq!(machine.output(), output.as_slice());
    assert_eq!(before.pc, 2);
    assert_eq!(machine.output(), &[1u8][..]);
    assert!(!machine.is_halted());
}

#[test]
fn machines_built_from_one_program_have_independent_states() {
    let program = optimized("+.,>+.");
    let ir_before: Vec<Op> = program.ops().to_vec();
    let config = || MachineConfig::with_max_steps(1_000);

    let mut first = OptimizedMachine::new(&program, vec![9], config());
    let mut second = OptimizedMachine::new(&program, vec![9], config());

    assert_eq!(first.run(), Ok(StopReason::Halted));

    // 一方の実行は他方の状態に影響しない。
    let fresh = OptimizedMachine::new(&program, vec![9], config());
    assert_eq!(second.snapshot(), fresh.snapshot());
    assert!(second.tape().iter().all(|&cell| cell == 0));
    assert!(second.output().is_empty());
    assert_eq!(second.input_position(), 0);
    assert!(!second.is_halted());

    assert_eq!(second.run(), Ok(StopReason::Halted));
    assert_eq!(second.snapshot(), first.snapshot());
    assert_eq!(second.output(), first.output());
    assert_eq!(second.tape(), first.tape());

    // IR は実行によって変更されない。
    assert_eq!(program.ops(), ir_before.as_slice());
}

#[test]
fn optimized_operations_count_naive_steps_and_fewer_ir_operations() {
    // 素朴実装では `+++++` の 5 命令、`[` の判定 1 回、5 反復それぞれの `-` と `]` の計 10 命令で
    // 合計 16 命令。IR では加算 1 操作とクリア 1 操作の計 2 操作で済む。
    let program = optimized("+++++[-]");
    let mut machine =
        OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(1_000));
    assert_eq!(machine.run(), Ok(StopReason::Halted));

    let snapshot = machine.snapshot();
    assert_eq!(snapshot.steps_executed, 16, "素朴実装換算の実行数");
    assert_eq!(snapshot.ir_ops_executed, 2, "IR 操作の実行回数");
    assert!(snapshot.ir_ops_executed < snapshot.steps_executed);
    assert_eq!(snapshot.current_cell, 0);
}

#[test]
fn zero_cell_transfer_loop_is_skipped_without_error() {
    // セル 0 では `[<+>-]` の本体は実行されず、素朴実装では `[` の判定の 1 命令だけで停止する。
    // 本体の `<` は範囲外に触れうるが、エラーにはならない(8.6)。
    let program = optimized("[<+>-]");
    let mut machine =
        OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(1_000));

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.steps_executed, 1, "素朴実装換算の実行数");
    assert_eq!(snapshot.data_pointer, 0);
    assert!(snapshot.halted);
}

#[test]
fn hello_world_prints_the_expected_bytes() {
    let program = optimized(HELLO);
    let mut machine =
        OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(100_000));

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), b"Hello World!\n");
    assert!(machine.is_halted());
}

#[test]
fn echo_copies_bytes_including_ff_and_stops_at_a_zero_cell() {
    // 0xFF は非ゼロなのでエコーされる。0x00 を読んだ時点で `[` を抜けて停止し、以降の入力は読まない。
    let program = optimized(ECHO);
    let input = vec![0x01, 0xFF, 0x7F, 0x00, b'A'];
    let mut machine =
        OptimizedMachine::new(&program, input, MachineConfig::with_max_steps(100_000));

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), &[0x01, 0xFF, 0x7F][..]);
    assert_eq!(machine.input_position(), 4);
    assert!(machine.is_halted());
}

#[test]
fn echo_stops_at_end_of_input_where_eof_reads_as_zero() {
    let program = optimized(ECHO);

    let mut machine = OptimizedMachine::new(
        &program,
        vec![0xFF, 0x80],
        MachineConfig::with_max_steps(100_000),
    );
    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), &[0xFF, 0x80][..]);
    assert_eq!(machine.input_position(), 2);

    let mut empty =
        OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(100_000));
    assert_eq!(empty.run(), Ok(StopReason::Halted));
    assert!(empty.output().is_empty());
    assert_eq!(empty.input_position(), 0);
}
