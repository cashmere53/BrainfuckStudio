//! ループ命令(`[` と `]`)のジャンプのテスト。
//!
//! 本ファイルでは `[` `]` `+` `-` `>` `<` `.` `,` のみを使う。各プログラムは有限回で停止する。
//! 無限ループを作る入力は使わず、停止しない場合に備えて `step` の回数を上限で制限する。

mod common;

use brainfuck_core::{Machine, MachineConfig, StepResult};
use common::{assert_same_state, machine_from};

/// 停止するまで `step` を呼ぶ。`max_steps` 回以内に停止しなければテストを失敗させる。
fn step_until_halted(machine: &mut Machine, max_steps: usize) {
    for _ in 0..max_steps {
        if machine.is_halted() {
            return;
        }
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert!(machine.is_halted(), "上限内に停止しなかった");
}

#[test]
fn left_bracket_on_zero_cell_jumps_past_matching_right_without_counting_body() {
    // `[>>+++<<]+` の本体(添字 1..=7)は実行も計数もされず、`pc` は `]` の次(添字 9)へ進む。
    let mut machine = machine_from("[>>+++<<]+", b"", MachineConfig::unlimited());

    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 9);
    assert_eq!(snapshot.steps_executed, 1);
    assert_eq!(snapshot.data_pointer, 0);
    assert_eq!(snapshot.current_cell, 0);
    assert!(!snapshot.halted);

    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().steps_executed, 2);
    assert_eq!(machine.snapshot().current_cell, 1);
    assert!(machine.is_halted());
}

#[test]
fn left_bracket_on_zero_cell_with_matching_right_last_halts() {
    // 対応する `]` が最後の命令のとき、スキップ後の `pc` は命令数と等しく停止状態になる。
    let mut machine = machine_from("[-]", b"", MachineConfig::unlimited());

    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 3);
    assert_eq!(snapshot.steps_executed, 1);
    assert!(snapshot.halted);
    assert_eq!(machine.step(), Ok(StepResult::Halted));
}

#[test]
fn left_bracket_on_nonzero_cell_advances_to_next_instruction() {
    // `+[-]` の `[` はセル 1 で非 0 のため、次の命令(添字 2)へ進む。
    let mut machine = machine_from("+[-]", b"", MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));

    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 2);
    assert_eq!(snapshot.steps_executed, 2);
    assert_eq!(snapshot.current_cell, 1);
}

#[test]
fn right_bracket_on_zero_cell_advances_to_next_instruction() {
    // `+[-]+` の `]` は 0 で抜けるため、次の命令(添字 4)へ進む。実行数は `+` `[` `-` `]` の 4。
    let mut machine = machine_from("+[-]+", b"", MachineConfig::unlimited());
    for _ in 0..4 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }

    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 4);
    assert_eq!(snapshot.steps_executed, 4);
    assert_eq!(snapshot.current_cell, 0);
    assert!(!snapshot.halted);
}

#[test]
fn loop_iteration_count_and_step_count_are_exact() {
    // `+++[-]`: `+++` で 3 回、`[` で 1 回、本体 `-` と `]` が 3 反復する。
    // 反復内の `]` は非 0 のとき `[` の次(添字 4)へ戻り、`[` は再評価されない。
    let mut machine = machine_from("+++[-]", b"", MachineConfig::unlimited());
    for _ in 0..3 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert_eq!(machine.step(), Ok(StepResult::Executed)); // `[`
    assert_eq!(machine.snapshot().pc, 4);

    assert_eq!(machine.step(), Ok(StepResult::Executed)); // `-` (cell 2)
    assert_eq!(machine.step(), Ok(StepResult::Executed)); // `]` (cell 2 != 0)
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 4);
    assert_eq!(snapshot.current_cell, 2);
    assert_eq!(snapshot.steps_executed, 6);

    step_until_halted(&mut machine, 20);
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.current_cell, 0);
    assert_eq!(snapshot.pc, 6);
    assert_eq!(snapshot.steps_executed, 10);
    assert!(snapshot.halted);
}

#[test]
fn loop_ending_at_last_instruction_halts_after_exit() {
    // `+[-]` は末尾の `]` で抜けた直後に停止する。実行数は `+` `[` `-` `]` の 4。
    let mut machine = machine_from("+[-]", b"", MachineConfig::unlimited());
    step_until_halted(&mut machine, 10);

    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 4);
    assert_eq!(snapshot.steps_executed, 4);
    assert_eq!(snapshot.current_cell, 0);
    assert!(snapshot.halted);
}

#[test]
fn parallel_loops_each_jump_to_their_own_matching_right_plus_one() {
    // `[-][+]`: 最初の `[` は添字 2 の `]` の次(添字 3)へ、次の `[` は添字 5 の `]` の次(添字 6)へ進む。
    let mut machine = machine_from("[-][+]", b"", MachineConfig::unlimited());

    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().pc, 3);
    assert_eq!(machine.snapshot().steps_executed, 1);

    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.snapshot().pc, 6);
    assert_eq!(machine.snapshot().steps_executed, 2);
    assert!(machine.is_halted());
}

#[test]
fn nested_loops_run_independently_and_jump_to_matching_pairs() {
    // `++[>[-]<-]`: 外側は 2 回反復し、内側の `[` はセル 1 が 0 のため毎回スキップされる。
    // 内側の `[` (添字 4) の対応は添字 6 の `]` なので、スキップ先は添字 7。
    // 外側の `]` (添字 9) の対応は添字 2 の `[` なので、非 0 のときの戻り先は添字 3。
    let mut machine = machine_from("++[>[-]<-]", b"", MachineConfig::unlimited());

    for _ in 0..4 {
        assert_eq!(machine.step(), Ok(StepResult::Executed));
    }
    assert_eq!(machine.snapshot().pc, 4); // `++[>` の後、内側 `[` を指す
    assert_eq!(machine.step(), Ok(StepResult::Executed)); // 内側 `[` (cell 1 = 0)
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 7);
    assert_eq!(snapshot.data_pointer, 1);
    assert_eq!(snapshot.steps_executed, 5);

    assert_eq!(machine.step(), Ok(StepResult::Executed)); // `<`
    assert_eq!(machine.step(), Ok(StepResult::Executed)); // `-` (cell 0 = 1)
    assert_eq!(machine.step(), Ok(StepResult::Executed)); // 外側 `]` (cell 0 != 0)
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 3);
    assert_eq!(snapshot.current_cell, 1);
    assert_eq!(snapshot.steps_executed, 8);

    step_until_halted(&mut machine, 10);
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.steps_executed, 13);
    assert_eq!(snapshot.data_pointer, 0);
    assert_eq!(snapshot.current_cell, 0);
    assert_eq!(machine.tape()[0], 0);
    assert_eq!(machine.tape()[1], 0);
    assert!(snapshot.halted);
}

#[test]
fn nested_loop_with_inner_iterations_produces_exact_counts_and_output() {
    // `++[>++[>+<-]<-]>>.`: 外側 2 回、各反復で内側が 2 回反復し、セル 2 に計 4 を加える。
    // 実行数は前置 `++` (2) + 外側 `[` (1) + 外側 1 反復目 (17) + 2 反復目 (17) + `>>.` (3) = 40。
    let mut machine = machine_from("++[>++[>+<-]<-]>>.", b"", MachineConfig::unlimited());
    step_until_halted(&mut machine, 100);

    let snapshot = machine.snapshot();
    assert_eq!(snapshot.steps_executed, 40);
    assert_eq!(snapshot.data_pointer, 2);
    assert_eq!(snapshot.current_cell, 4);
    assert_eq!(machine.tape()[0], 0);
    assert_eq!(machine.tape()[1], 0);
    assert_eq!(machine.output(), &[4]);
    assert!(snapshot.halted);
}

#[test]
fn loop_instructions_do_not_change_cell_pointer_input_or_output() {
    // `,[.-]` の `[` を 1 回だけ実行し、その前後で `[` が変更するものが無いことを確認する。
    let mut machine = machine_from(",[.-]", &[2], MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed)); // `,` で cell 0 = 2
    let before = machine.clone();

    assert_eq!(machine.step(), Ok(StepResult::Executed)); // `[` (cell 0 != 0)
    assert_eq!(machine.snapshot().pc, 2);
    assert_eq!(machine.snapshot().steps_executed, 2);
    assert_eq!(
        machine.snapshot().data_pointer,
        before.snapshot().data_pointer
    );
    assert_eq!(machine.snapshot().current_cell, 2);
    assert_eq!(machine.tape(), before.tape());
    assert_eq!(machine.output(), before.output());
    assert_eq!(machine.input_position(), before.input_position());
}

#[test]
fn echo_loop_outputs_input_and_halts_after_exit() {
    // `,[.-]` は入力 `[2, 1]` を 2 反復で処理する。出力は `[2, 1]`。
    // 実行数は `,` (1) + `[` (1) + 反復ごとの `.` `-` `]` (3) × 2 = 8。
    let mut machine = machine_from(",[.-]", &[2, 1], MachineConfig::unlimited());
    step_until_halted(&mut machine, 20);

    assert_eq!(machine.output(), &[2, 1]);
    assert_eq!(machine.snapshot().steps_executed, 8);
    assert_eq!(machine.snapshot().current_cell, 0);
    assert!(machine.is_halted());
}

#[test]
fn copies_of_machine_after_loop_steps_remain_independent() {
    // ループを進めた後の複製は、元の機械と完全一致し、その後は互いに独立に動く。
    let mut original = machine_from("+[-]", b"", MachineConfig::unlimited());
    assert_eq!(original.step(), Ok(StepResult::Executed));
    assert_eq!(original.step(), Ok(StepResult::Executed));
    let copy = original.clone();
    assert_same_state(&original, &copy);

    step_until_halted(&mut original, 10);
    assert!(original.is_halted());
    assert!(!copy.is_halted());
}
