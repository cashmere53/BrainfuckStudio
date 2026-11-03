//! 1 ステップ実行の骨格(停止・上限・計数)と完走実行 `run` のテスト。
//!
//! 前半の step テストでは加算・減算のみで構成されるプログラムを使う。
//! 後半の run テストでは停止理由・エラー中断・`step` 反復との一致を確認する。

mod common;

use brainfuck_core::{
    Machine, MachineConfig, PointerDirection, RuntimeError, StepResult, StopReason, TAPE_SIZE,
};
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

/// `run` を呼び、停止理由またはエラーをそのまま返す。
fn run_result(machine: &mut Machine) -> Result<StopReason, RuntimeError> {
    machine.run()
}

/// `step` を繰り返して `run` と同じ結果の形に揃える。
///
/// 停止で `Ok(StopReason::Halted)`、実行エラーでそのエラーを返す。
fn run_by_steps(machine: &mut Machine) -> Result<StopReason, RuntimeError> {
    loop {
        match machine.step() {
            Ok(StepResult::Executed) => continue,
            Ok(StepResult::Halted) => return Ok(StopReason::Halted),
            Err(error) => return Err(error),
        }
    }
}

#[test]
fn run_returns_halted_on_normal_completion() {
    let mut machine = machine_from("+++", b"", MachineConfig::unlimited());
    assert_eq!(run_result(&mut machine), Ok(StopReason::Halted));
    let snapshot = machine.snapshot();
    assert!(snapshot.halted);
    assert_eq!(snapshot.pc, 3);
    assert_eq!(snapshot.steps_executed, 3);
    assert_eq!(snapshot.current_cell, 3);
}

#[test]
fn run_on_empty_program_returns_halted_without_steps() {
    let mut machine = machine_from("", b"", MachineConfig::unlimited());
    let before = machine.clone();
    assert_eq!(run_result(&mut machine), Ok(StopReason::Halted));
    assert_same_state(&machine, &before);
    assert_eq!(machine.snapshot().steps_executed, 0);
}

#[test]
fn run_completes_loop_program_with_exact_step_count() {
    // `+[-]`: `+` 1 回、`[` 1 回(非 0 で進む)、`-` 1 回、`]` 1 回(0 で進む)。
    let mut machine = machine_from("+[-]", b"", MachineConfig::with_max_steps(100));
    assert_eq!(run_result(&mut machine), Ok(StopReason::Halted));
    let snapshot = machine.snapshot();
    assert!(snapshot.halted);
    assert_eq!(snapshot.current_cell, 0);
    assert_eq!(snapshot.steps_executed, 4);
}

#[test]
fn run_on_halted_machine_is_idempotent() {
    let mut machine = machine_from("++", b"", MachineConfig::unlimited());
    assert_eq!(run_result(&mut machine), Ok(StopReason::Halted));
    let before = machine.clone();
    for _ in 0..3 {
        assert_eq!(run_result(&mut machine), Ok(StopReason::Halted));
        assert_same_state(&machine, &before);
    }
}

#[test]
fn run_on_halted_machine_with_zero_limit_is_halted() {
    // 停止済みの判定は上限判定より先に行われるため、上限 0 でも正常終了する。
    let mut machine = machine_from("", b"", MachineConfig::with_max_steps(0));
    let before = machine.clone();
    assert_eq!(run_result(&mut machine), Ok(StopReason::Halted));
    assert_same_state(&machine, &before);
}

#[test]
fn run_stops_at_pointer_underflow_and_keeps_state() {
    // `++.<`: 4 命令目の `<` は dp 0 で左へ動けず、pc 3・dp 0 で中断する。
    let mut machine = machine_from("++.<", b"", MachineConfig::unlimited());
    assert_eq!(
        run_result(&mut machine),
        Err(RuntimeError::PointerOutOfBounds {
            pc: 3,
            dp: 0,
            direction: PointerDirection::Left,
        })
    );
    let snapshot = machine.snapshot();
    assert!(!snapshot.halted);
    assert_eq!(snapshot.pc, 3);
    assert_eq!(snapshot.data_pointer, 0);
    assert_eq!(snapshot.current_cell, 2);
    assert_eq!(snapshot.steps_executed, 3);
    assert_eq!(machine.output(), &[2]);
}

#[test]
fn run_stops_at_pointer_overflow_with_right_direction() {
    // `>` を 29,999 回進めて右端に達した後の `>` で範囲外となる。
    let source = format!("{}>.", ">".repeat(TAPE_SIZE - 1));
    let mut machine = machine_from(&source, b"", MachineConfig::unlimited());
    assert_eq!(
        run_result(&mut machine),
        Err(RuntimeError::PointerOutOfBounds {
            pc: TAPE_SIZE - 1,
            dp: TAPE_SIZE - 1,
            direction: PointerDirection::Right,
        })
    );
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.data_pointer, TAPE_SIZE - 1);
    assert_eq!(snapshot.pc, TAPE_SIZE - 1);
    assert_eq!(snapshot.steps_executed, (TAPE_SIZE - 1) as u64);
    assert!(machine.output().is_empty());
}

#[test]
fn run_pointer_error_keeps_output_and_tape_and_repeats_same_error() {
    let mut machine = machine_from("+.-<", b"", MachineConfig::unlimited());
    let expected = RuntimeError::PointerOutOfBounds {
        pc: 3,
        dp: 0,
        direction: PointerDirection::Left,
    };
    assert_eq!(run_result(&mut machine), Err(expected));
    let before = machine.clone();
    assert_eq!(machine.output(), &[1]);
    assert_eq!(machine.tape()[0], 0);
    assert_eq!(run_result(&mut machine), Err(expected));
    assert_same_state(&machine, &before);
}

#[test]
fn run_stops_at_step_limit_with_output_and_state_retained() {
    // `+.+.+.`: 上限 3 では 3 命令を実行し、4 命令目(`.`)の前で pc 3 の上限エラーとなる。
    let mut machine = machine_from("+.+.+.", b"", MachineConfig::with_max_steps(3));
    let expected = RuntimeError::StepLimitExceeded { limit: 3, pc: 3 };
    assert_eq!(run_result(&mut machine), Err(expected));
    let snapshot = machine.snapshot();
    assert!(!snapshot.halted);
    assert_eq!(snapshot.pc, 3);
    assert_eq!(snapshot.steps_executed, 3);
    assert_eq!(snapshot.current_cell, 2);
    assert_eq!(machine.output(), &[1]);
    assert_eq!(run_result(&mut machine), Err(expected));
}

#[test]
fn run_with_zero_limit_on_nonempty_program_fails_without_changes() {
    let mut machine = machine_from("+", b"", MachineConfig::with_max_steps(0));
    let before = machine.clone();
    assert_eq!(
        run_result(&mut machine),
        Err(RuntimeError::StepLimitExceeded { limit: 0, pc: 0 })
    );
    assert_same_state(&machine, &before);
}

#[test]
fn run_limit_equal_to_program_length_halts_normally() {
    let mut machine = machine_from("+-+", b"", MachineConfig::with_max_steps(3));
    assert_eq!(run_result(&mut machine), Ok(StopReason::Halted));
    assert_eq!(machine.snapshot().steps_executed, 3);
}

#[test]
fn run_matches_step_repetition_for_final_state_across_programs() {
    // 出力・ループ・入力・ポインタ移動を含む有限のプログラムを、各種設定で比較する。
    let hello_world = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.\
                       >>.<-.<.+++.------.--------.>>+.>++.";
    let cases: &[(&str, &[u8])] = &[
        (hello_world, b""),
        (",[.,]", b"\x00\xff\x10 abc"),
        (",[>,[.-]<-]", b"\x03\x01\x02\x03 xyz"),
        (",.,.,.+[-]", b"\x7f"),
        ("+>+<<", b""),
        ("+[-]>+.<", b""),
        ("+.+.+.-.[-]", b""),
    ];
    let configs = [
        MachineConfig::unlimited(),
        MachineConfig::with_max_steps(0),
        MachineConfig::with_max_steps(1),
        MachineConfig::with_max_steps(5),
        MachineConfig::with_max_steps(50),
        MachineConfig::with_max_steps(5_000),
    ];
    for &(source, input) in cases {
        for &config in &configs {
            let program =
                brainfuck_core::Program::parse(source).expect("テスト用ソースは解析に成功する");
            let mut by_run = Machine::new(program.clone(), input.to_vec(), config);
            let mut by_steps = Machine::new(program, input.to_vec(), config);
            let run_outcome = run_result(&mut by_run);
            let step_outcome = run_by_steps(&mut by_steps);
            assert_eq!(
                run_outcome, step_outcome,
                "source={source:?} config={config:?}"
            );
            assert_same_state(&by_run, &by_steps);
        }
    }
}
