//! 機械の生成直後の状態と観測 API のテスト。
//!
//! 命令の実行(`step` / `run`)は呼び出さない。

mod common;

use brainfuck_core::{
    Machine, MachineConfig, MachineSnapshot, ParseError, PointerDirection, Program, RuntimeError,
    StepResult, StopReason, TAPE_SIZE,
};
use common::{assert_same_state, machine_from};
use std::any::TypeId;
use std::error::Error;

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

/// 1 命令だけ実行し、実行前後のスナップショットを返す。
fn one_step(machine: &mut Machine) -> (MachineSnapshot, MachineSnapshot) {
    let before = machine.snapshot();
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    (before, machine.snapshot())
}

#[test]
fn increment_effect_is_observable_from_snapshots() {
    let mut machine = machine_from("+", b"", MachineConfig::unlimited());
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.current_cell, 0);
    assert_eq!(after.current_cell, 1);
    assert_eq!(after.steps_executed, before.steps_executed + 1);
    assert_eq!(after.pc, before.pc + 1);
    assert_eq!(after.data_pointer, before.data_pointer);
}

#[test]
fn decrement_effect_wraps_cell_and_is_observable_from_snapshots() {
    let mut machine = machine_from("-", b"", MachineConfig::unlimited());
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.current_cell, 0);
    assert_eq!(after.current_cell, 255);
    assert_eq!(after.steps_executed, 1);
    assert_eq!(after.pc, 1);
    assert_eq!(after.data_pointer, 0);
}

#[test]
fn move_right_effect_changes_pointer_and_current_cell() {
    let mut machine = machine_from(">", b"", MachineConfig::unlimited());
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.data_pointer, 0);
    assert_eq!(after.data_pointer, 1);
    assert_eq!(after.current_cell, 0);
    assert_eq!(after.steps_executed, 1);
    assert_eq!(after.pc, 1);
}

#[test]
fn move_left_effect_changes_pointer_and_current_cell() {
    let mut machine = machine_from("><", b"", MachineConfig::unlimited());
    let _ = one_step(&mut machine);
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.data_pointer, 1);
    assert_eq!(after.data_pointer, 0);
    assert_eq!(after.current_cell, 0);
    assert_eq!(after.steps_executed, 2);
    assert_eq!(after.pc, 2);
}

#[test]
fn output_effect_appends_current_cell_without_changing_cell() {
    let mut machine = machine_from("+.", b"", MachineConfig::unlimited());
    let _ = one_step(&mut machine);
    let output_before = machine.output().to_vec();
    let (before, after) = one_step(&mut machine);
    assert!(output_before.is_empty());
    assert_eq!(machine.output(), &[1u8][..]);
    assert_eq!(before.current_cell, after.current_cell);
    assert_eq!(after.steps_executed, 2);
    assert_eq!(after.pc, 2);
}

#[test]
fn input_effect_stores_byte_and_advances_input_position() {
    let mut machine = machine_from(",", b"A", MachineConfig::unlimited());
    let position_before = machine.input_position();
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.current_cell, 0);
    assert_eq!(after.current_cell, b'A');
    assert_eq!(position_before, 0);
    assert_eq!(machine.input_position(), 1);
    assert_eq!(after.steps_executed, 1);
    assert_eq!(after.pc, 1);
}

#[test]
fn loop_start_skip_effect_jumps_past_matching_end_without_counting_skipped_body() {
    // セルが 0 のため `[` は対応する `]` の次へ飛ぶ。スキップされた `+` は実行数に加算されない。
    let mut machine = machine_from("[+]", b"", MachineConfig::unlimited());
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.pc, 0);
    assert_eq!(after.pc, 3);
    assert_eq!(after.steps_executed, 1);
    assert_eq!(after.current_cell, 0);
    assert!(after.halted);
}

#[test]
fn loop_start_fallthrough_effect_moves_to_next_instruction() {
    // `+` でセルを 1 にし、非 0 の `[` は次の命令へ進む。
    let mut machine = machine_from("+[-]", b"", MachineConfig::unlimited());
    let _ = one_step(&mut machine);
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.current_cell, 1);
    assert_eq!(after.pc, 2);
    assert_eq!(after.current_cell, 1);
    assert_eq!(after.steps_executed, 2);
    assert!(!after.halted);
}

#[test]
fn loop_end_jump_effect_returns_to_instruction_after_matching_start() {
    // `++[>+<-]` の 1 周目: 8 命令目の `]` はセル 1 (非 0) なので `[` の次(添字 3)へ戻る。
    let mut machine = machine_from("++[>+<-]", b"", MachineConfig::unlimited());
    for _ in 0..7 {
        let _ = one_step(&mut machine);
    }
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.pc, 7);
    assert_eq!(before.current_cell, 1);
    assert_eq!(after.pc, 3);
    assert_eq!(after.data_pointer, 0);
    assert_eq!(after.current_cell, 1);
    assert_eq!(after.steps_executed, 8);
    assert!(!after.halted);
}

#[test]
fn loop_end_fallthrough_effect_moves_past_end_when_cell_is_zero() {
    // `+[-]` の `]` はセル 0 のため戻らず、末尾の次へ進んで停止する。
    let mut machine = machine_from("+[-]", b"", MachineConfig::unlimited());
    for _ in 0..3 {
        let _ = one_step(&mut machine);
    }
    let (before, after) = one_step(&mut machine);
    assert_eq!(before.pc, 3);
    assert_eq!(before.current_cell, 0);
    assert_eq!(after.pc, 4);
    assert_eq!(after.steps_executed, 4);
    assert!(after.halted);
}

#[test]
fn snapshot_does_not_change_machine_state() {
    let mut machine = machine_from("+>,.[-]", b"x", MachineConfig::with_max_steps(10));
    let _ = one_step(&mut machine);
    let _ = one_step(&mut machine);

    let before = machine.clone();
    let first = machine.snapshot();
    let second = machine.snapshot();
    assert_eq!(first, second);
    assert_same_state(&machine, &before);
    assert_eq!(machine, before);
}

#[test]
fn observation_accessors_do_not_change_machine_state() {
    let mut machine = machine_from("+.", b"ab", MachineConfig::unlimited());
    let _ = one_step(&mut machine);
    let before = machine.clone();
    let _ = machine.snapshot();
    let _ = machine.tape();
    let _ = machine.output();
    let _ = machine.is_halted();
    let _ = machine.program();
    let _ = machine.input_position();
    assert_same_state(&machine, &before);
}

#[test]
fn halted_flag_is_false_while_running_and_true_after_last_instruction() {
    // `+[-]` は 4 命令。途中の 3 回はすべて停止していない。
    let mut machine = machine_from("+[-]", b"", MachineConfig::unlimited());
    for _ in 0..4 {
        assert!(!machine.snapshot().halted);
        assert!(!machine.is_halted());
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert!(machine.snapshot().halted);
    assert!(machine.is_halted());
}

#[test]
fn halted_flag_stays_true_after_further_steps() {
    let mut machine = machine_from("+", b"", MachineConfig::unlimited());
    assert!(!machine.snapshot().halted);
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert!(machine.snapshot().halted);
    assert_eq!(machine.step(), Ok(StepResult::Halted));
    assert!(machine.snapshot().halted);
}

#[test]
fn unmatched_closing_bracket_display_names_cause_index_and_offset() {
    // コメント文字 `a` `b` を挟み、命令添字 1 とソースオフセット 3 を区別する。
    let error = Program::parse("a+b]").unwrap_err();
    assert_eq!(
        error,
        ParseError::UnmatchedClosingBracket {
            instruction_index: 1,
            source_offset: 3,
        }
    );
    let text = error.to_string();
    assert!(text.contains("unmatched closing bracket"), "{text}");
    assert!(text.contains("instruction #1"), "{text}");
    assert!(text.contains("source offset 3"), "{text}");
}

#[test]
fn unmatched_opening_bracket_display_names_cause_index_and_offset() {
    // 最初に積まれた未解決の `[`(添字 1、オフセット 3)が報告される。
    // コメント文字 `a` `b` を挟み、添字とオフセットを区別する。
    let error = Program::parse("a+b[+[").unwrap_err();
    assert_eq!(
        error,
        ParseError::UnmatchedOpeningBracket {
            instruction_index: 1,
            source_offset: 3,
        }
    );
    let text = error.to_string();
    assert!(text.contains("unmatched opening bracket"), "{text}");
    assert!(text.contains("instruction #1"), "{text}");
    assert!(text.contains("source offset 3"), "{text}");
}

#[test]
fn parse_error_offset_is_a_byte_position_after_multibyte_text() {
    // 「あ」は 3 バイトのため、`]` のソースオフセットは文字数の 1 ではなく 3 になる。
    let error = Program::parse("あ]").unwrap_err();
    assert_eq!(
        error,
        ParseError::UnmatchedClosingBracket {
            instruction_index: 0,
            source_offset: 3,
        }
    );
    assert!(error.to_string().contains("source offset 3"));
}

#[test]
fn parse_error_display_does_not_contain_source_text() {
    let error = Program::parse("秘密+]SECRET").unwrap_err();
    let text = error.to_string();
    assert!(text.contains("instruction #1"), "{text}");
    assert!(text.contains("source offset 7"), "{text}");
    assert!(!text.contains("秘密"), "{text}");
    assert!(!text.contains("SECRET"), "{text}");

    let bytes_error = Program::parse_bytes(b"\xFFSECRET[").unwrap_err();
    let bytes_text = bytes_error.to_string();
    assert!(bytes_text.contains("source offset 7"), "{bytes_text}");
    assert!(!bytes_text.contains("SECRET"), "{bytes_text}");
}

#[test]
fn pointer_left_out_of_bounds_display_names_cause_pc_dp_and_direction() {
    let mut machine = machine_from("+<", b"", MachineConfig::unlimited());
    let _ = one_step(&mut machine);
    let error = machine.run().unwrap_err();
    assert_eq!(
        error,
        RuntimeError::PointerOutOfBounds {
            pc: 1,
            dp: 0,
            direction: PointerDirection::Left,
        }
    );
    let text = error.to_string();
    assert!(text.contains("data pointer out of bounds"), "{text}");
    assert!(text.contains("pc=1"), "{text}");
    assert!(text.contains("dp=0"), "{text}");
    assert!(text.contains("direction=left"), "{text}");
}

#[test]
fn pointer_right_out_of_bounds_display_names_cause_pc_dp_and_direction() {
    // 先頭の `+` の後、29,999 回の右移動で dp は末尾セル(29,999)に至る。
    // 添字 30,000 の `>` が範囲外になるため pc=30,000、dp=29,999 となり、両者は異なる。
    let source = format!("+{}", ">".repeat(TAPE_SIZE));
    let mut machine = machine_from(&source, b"", MachineConfig::unlimited());
    let error = machine.run().unwrap_err();
    assert_eq!(
        error,
        RuntimeError::PointerOutOfBounds {
            pc: TAPE_SIZE,
            dp: TAPE_SIZE - 1,
            direction: PointerDirection::Right,
        }
    );
    let text = error.to_string();
    assert!(text.contains(&format!("pc={TAPE_SIZE},")), "{text}");
    assert!(text.contains(&format!("dp={}", TAPE_SIZE - 1)), "{text}");
    assert!(text.contains("direction=right"), "{text}");
    // 範囲外エラーでは実行数が加算されない。`+` と 29,999 回の `>` の計 30,000 回のみ。
    assert_eq!(machine.snapshot().steps_executed, TAPE_SIZE as u64);
}

#[test]
fn step_limit_display_names_cause_limit_and_pc() {
    // `+[]` は上限 5 で未停止のまま止まる。実行数 5、上限 5、pc 2 は互いに異なる値にする。
    let mut machine = machine_from("+[]", b"", MachineConfig::with_max_steps(5));
    let error = machine.run().unwrap_err();
    assert_eq!(error, RuntimeError::StepLimitExceeded { limit: 5, pc: 2 });
    assert_eq!(machine.snapshot().steps_executed, 5);
    let text = error.to_string();
    assert!(text.contains("step limit exceeded"), "{text}");
    assert!(text.contains("limit=5"), "{text}");
    assert!(text.contains("after 5 steps"), "{text}");
    assert!(text.contains("pc=2"), "{text}");
}

#[test]
fn runtime_error_display_does_not_contain_source_or_input_bytes() {
    let mut machine = machine_from("秘密<", b"SECRET\xFF", MachineConfig::unlimited());
    let error = machine.run().unwrap_err();
    let text = error.to_string();
    assert!(text.contains("pc=0"), "{text}");
    assert!(text.contains("dp=0"), "{text}");
    assert!(text.contains("direction=left"), "{text}");
    assert!(!text.contains("秘密"), "{text}");
    assert!(!text.contains("SECRET"), "{text}");
    assert!(!text.contains("secret"), "{text}");
}

/// 解析と完走実行を 1 つの関数で行い、どちらのエラーも `?` で標準のエラー表現へ変換する。
fn parse_and_run(
    source: &str,
    input: &[u8],
    config: MachineConfig,
) -> Result<StopReason, Box<dyn Error>> {
    let program = Program::parse(source)?;
    let mut machine = Machine::new(program, input.to_vec(), config);
    Ok(machine.run()?)
}

#[test]
fn errors_convert_into_boxed_standard_error() {
    let parse_error = parse_and_run("]", b"", MachineConfig::unlimited()).unwrap_err();
    assert!(parse_error
        .to_string()
        .contains("unmatched closing bracket"));
    assert!(parse_error.downcast_ref::<ParseError>().is_some());

    let pointer_error = parse_and_run("<", b"", MachineConfig::unlimited()).unwrap_err();
    assert!(pointer_error
        .to_string()
        .contains("data pointer out of bounds"));
    assert!(pointer_error.downcast_ref::<RuntimeError>().is_some());

    let limit_error = parse_and_run("+[]", b"", MachineConfig::with_max_steps(3)).unwrap_err();
    assert!(limit_error.to_string().contains("step limit exceeded"));
    assert!(limit_error.downcast_ref::<RuntimeError>().is_some());

    let ok = parse_and_run("+", b"", MachineConfig::unlimited());
    assert_eq!(ok.unwrap(), StopReason::Halted);
}

#[test]
fn parse_error_and_runtime_error_are_distinct_types() {
    assert_ne!(
        TypeId::of::<ParseError>(),
        TypeId::of::<RuntimeError>(),
        "解析エラーと実行エラーは別の型である"
    );

    let parse_boxed: Box<dyn Error> = Box::new(Program::parse("]").unwrap_err());
    assert!(parse_boxed.downcast_ref::<RuntimeError>().is_none());

    let runtime_boxed: Box<dyn Error> =
        Box::new(RuntimeError::StepLimitExceeded { limit: 0, pc: 0 });
    assert!(runtime_boxed.downcast_ref::<ParseError>().is_none());
}
