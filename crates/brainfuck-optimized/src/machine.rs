//! 公開の実行機械(`OptimizedMachine`)。
//!
//! 最適化済みプログラムを、一括適用([`crate::fast`])と縮退再生([`crate::replay`])で完走まで実行する。
//! 縮退経路は公開の再生関数の呼び出しのみで実現し、この crate に素朴意味論の別実装を持たない。
//!
//! 観測 API(スナップショット・テープ参照など)は 3.6 で追加する。本タスクのテストは内部状態を直接読む。

#![allow(dead_code)]

use crate::fast::{execute, FastOutcome};
use crate::ir::OptimizedProgram;
use crate::replay::{replay, Replayed};
use crate::state::ExecState;
use brainfuck_core::{MachineConfig, RuntimeError, StopReason};

/// 最適化済みプログラムの実行機械。
///
/// `OptimizedProgram` を借用し、IR は変更しない。機械ごとに実行状態を所有するため、
/// 同じプログラムから作った複数の機械は互いに独立である。
///
/// 上限を持たない設定(`MachineConfig::unlimited`)で終了しないプログラムを実行すると、
/// [`OptimizedMachine::run`] は戻らない(core の `Machine::run` と同じ)。
#[derive(Debug)]
pub struct OptimizedMachine<'p> {
    /// 借用した最適化済みプログラム。
    program: &'p OptimizedProgram,
    /// 実行設定(命令数の上限の有無)。
    config: MachineConfig,
    /// 実行状態(素朴再生の状態と IR 上の位置)。
    state: ExecState,
    /// 確定した結果。停止またはエラーが確定した後は、以降の `run` がこれを返す。
    outcome: Option<Replayed>,
}

impl<'p> OptimizedMachine<'p> {
    /// 最適化済みプログラム・入力バイト列・実行設定から機械を生成する。
    ///
    /// 初期状態はテープ全 0、データポインタ・入力位置・実行数は 0、出力は空。失敗しない。
    pub fn new(program: &'p OptimizedProgram, input: Vec<u8>, config: MachineConfig) -> Self {
        OptimizedMachine {
            program,
            config,
            state: ExecState::new(input),
            outcome: None,
        }
    }

    /// 停止またはエラーまで完走実行する。
    ///
    /// 操作が尽きていれば正常停止を確定する。そうでなければ操作のコストと条件を評価して一括適用し、
    /// 縮退が返ればその操作の先頭の命令添字から公開の再生関数で停止またはエラーまで進め、結果を確定する。
    /// 確定した結果は保持し、以降の呼び出しは状態を変えず同じ結果を返す。
    ///
    /// 上限 0 の空プログラムは停止、上限 0 の非空プログラムは最初の操作でステップ制限超過になる。
    /// 上限を持たない設定で終了しないプログラムに対しては戻らない(呼び出し側が上限を設定する責務を負う)。
    pub fn run(&mut self) -> Result<StopReason, RuntimeError> {
        if let Some(done) = self.outcome {
            return done.result;
        }
        let program = self.program;
        let limit = self.config.max_steps;
        let ops = program.ops();
        while self.state.ir_pc < ops.len() {
            let op = &ops[self.state.ir_pc];
            let remaining = match limit {
                Some(max) => max.saturating_sub(self.state.replay.steps_executed),
                None => u64::MAX,
            };
            if execute(op, &mut self.state, remaining) == FastOutcome::Fallback {
                let start = op.source_range().start;
                let replayed = replay(program.source(), limit, start, &mut self.state.replay);
                self.outcome = Some(replayed);
                return replayed.result;
            }
        }
        let done = Replayed {
            pc: program.source().len(),
            result: Ok(StopReason::Halted),
        };
        self.outcome = Some(done);
        done.result
    }
}

#[cfg(test)]
mod tests {
    use super::OptimizedMachine;
    use crate::ir::OptimizedProgram;
    use brainfuck_core::{
        Machine, MachineConfig, PointerDirection, Program, RuntimeError, StopReason, TAPE_SIZE,
    };

    fn compiled(source: &str) -> OptimizedProgram {
        OptimizedProgram::compile(&Program::parse(source).expect("valid program"))
    }

    /// 同一条件で core の完走実行と比較する(core の `Machine` がオラクル)。
    fn assert_matches_core(source: &str, input: &[u8], config: MachineConfig) {
        let mut core = Machine::new(
            Program::parse(source).expect("valid program"),
            input.to_vec(),
            config,
        );
        let core_result = core.run();
        let core_snapshot = core.snapshot();

        let program = compiled(source);
        let mut machine = OptimizedMachine::new(&program, input.to_vec(), config);
        let result = machine.run();
        let label = format!("source {source:?}, input {input:?}, config {config:?}");
        assert_eq!(result, core_result, "result mismatch: {label}");
        assert_eq!(machine.state.replay.tape(), core.tape(), "tape: {label}");
        assert_eq!(
            machine.state.replay.output(),
            core.output(),
            "output: {label}"
        );
        assert_eq!(
            machine.state.replay.input_position(),
            core.input_position(),
            "input position: {label}"
        );
        assert_eq!(
            machine.state.replay.data_pointer(),
            core_snapshot.data_pointer,
            "data pointer: {label}"
        );
        assert_eq!(
            machine.state.replay.steps_executed(),
            core_snapshot.steps_executed,
            "steps: {label}"
        );
        assert_eq!(
            machine.outcome.map(|done| done.pc),
            Some(core_snapshot.pc),
            "pc: {label}"
        );
    }

    #[test]
    fn new_starts_from_the_initial_state() {
        let program = compiled("+[-]");
        let machine = OptimizedMachine::new(&program, vec![9], MachineConfig::unlimited());
        assert_eq!(machine.state.ir_pc, 0);
        assert_eq!(machine.state.ir_ops_executed, 0);
        assert_eq!(machine.state.replay.tape().len(), TAPE_SIZE);
        assert!(machine.state.replay.tape().iter().all(|&cell| cell == 0));
        assert_eq!(machine.state.replay.data_pointer(), 0);
        assert_eq!(machine.state.replay.steps_executed(), 0);
        assert_eq!(machine.state.replay.input(), &[9]);
        assert_eq!(machine.state.replay.input_position(), 0);
        assert!(machine.state.replay.output().is_empty());
        assert_eq!(machine.outcome, None);
    }

    #[test]
    fn empty_program_with_zero_limit_halts() {
        let program = compiled("");
        let mut machine =
            OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(0));
        assert_eq!(machine.run(), Ok(StopReason::Halted));
        assert_eq!(machine.state.replay.steps_executed(), 0);
    }

    #[test]
    fn non_empty_program_with_zero_limit_fails_at_the_first_operation() {
        let program = compiled("+");
        let mut machine =
            OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(0));
        assert_eq!(
            machine.run(),
            Err(RuntimeError::StepLimitExceeded { limit: 0, pc: 0 })
        );
        assert_eq!(machine.state.replay.tape()[0], 0);
        assert_eq!(machine.state.replay.steps_executed(), 0);
        assert_eq!(machine.state.ir_ops_executed, 0);
    }

    #[test]
    fn add_and_clear_run_to_halt_with_the_naive_step_count() {
        // `+++++` は素朴換算 5、`[-]` は v = 5 のクリアで 1 + 2v = 11。合計 16。
        let program = compiled("+++++[-]");
        let mut machine = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        assert_eq!(machine.run(), Ok(StopReason::Halted));
        assert_eq!(machine.state.replay.tape()[0], 0);
        assert_eq!(machine.state.replay.steps_executed(), 16);
        assert_eq!(machine.state.ir_ops_executed, 2);
        assert_eq!(machine.outcome.map(|done| done.pc), Some(8));
        assert_matches_core("+++++[-]", &[], MachineConfig::unlimited());
    }

    #[test]
    fn out_of_bounds_pointer_leaves_the_state_unchanged() {
        let program = compiled("<");
        let mut machine = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        assert_eq!(
            machine.run(),
            Err(RuntimeError::PointerOutOfBounds {
                pc: 0,
                dp: 0,
                direction: PointerDirection::Left,
            })
        );
        assert_eq!(machine.state.replay.data_pointer(), 0);
        assert_eq!(machine.state.replay.steps_executed(), 0);
        assert_eq!(machine.state.replay.tape()[0], 0);
    }

    #[test]
    fn error_is_fixed_and_repeated_runs_do_not_change_the_state() {
        let program = compiled("+++<");
        let mut machine = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        let first = machine.run();
        assert!(matches!(
            first,
            Err(RuntimeError::PointerOutOfBounds { pc: 3, dp: 0, .. })
        ));
        let before = machine.state.clone();
        assert_eq!(machine.run(), first);
        assert_eq!(machine.state, before);
    }

    #[test]
    fn halted_machine_reruns_return_the_same_result_without_changes() {
        let program = compiled("+.");
        let mut machine = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        assert_eq!(machine.run(), Ok(StopReason::Halted));
        let before = machine.state.clone();
        assert_eq!(machine.run(), Ok(StopReason::Halted));
        assert_eq!(machine.state, before);
        assert_eq!(machine.state.replay.output(), &[1]);
    }

    #[test]
    fn machines_from_one_program_are_independent() {
        let program = compiled("++[-]>+");
        let mut first = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        let second = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        let untouched = second.state.clone();
        assert_eq!(first.run(), Ok(StopReason::Halted));
        assert_eq!(second.state, untouched);
        let mut second = second;
        assert_eq!(second.run(), Ok(StopReason::Halted));
        assert_eq!(first.state, second.state);
    }

    #[test]
    fn representative_programs_match_the_core_machine() {
        assert_matches_core("++++++++[>++++++++<-]>+.", &[], MachineConfig::unlimited());
        assert_matches_core("+>+>+<<[>]", &[], MachineConfig::unlimited());
        assert_matches_core("+++[->++<]", &[], MachineConfig::unlimited());
        assert_matches_core("-[+]", &[], MachineConfig::unlimited());
        assert_matches_core("-.", &[], MachineConfig::unlimited());
        assert_matches_core(",[.,]", &[0x00, 0xFF, 7], MachineConfig::unlimited());
        assert_matches_core(",[.,]", &[], MachineConfig::unlimited());
        assert_matches_core("<", &[], MachineConfig::unlimited());
        assert_matches_core("+[>+.<-]", &[], MachineConfig::unlimited());
    }

    #[test]
    fn step_limits_match_the_core_machine_at_every_point() {
        let sources = [
            "++++++++[>++++++++<-]>+.",
            "+++++[-]",
            "+>+>+<<[>]",
            "+++[->++<]",
            "-[+]",
            ",[.,]",
            "+[]",
            "+[>+]",
        ];
        for source in sources {
            for limit in 0..=40 {
                assert_matches_core(source, &[1, 2, 3], MachineConfig::with_max_steps(limit));
            }
        }
    }

    #[test]
    fn limit_exceeded_inside_a_clear_reports_the_core_pc_and_state() {
        // `[-]` の一括適用(コスト 11)は残り 10 で縮退し、素朴再生が上限超過まで進む。
        let program = compiled("+++++[-]");
        let mut machine =
            OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(10));
        assert_eq!(
            machine.run(),
            Err(RuntimeError::StepLimitExceeded { limit: 10, pc: 6 })
        );
        assert_eq!(machine.state.replay.steps_executed(), 10);
        assert_matches_core("+++++[-]", &[], MachineConfig::with_max_steps(10));
    }

    #[test]
    fn infinite_loop_stops_at_the_limit_with_the_core_result() {
        let program = compiled("+[]");
        let mut machine =
            OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(3));
        assert_eq!(
            machine.run(),
            Err(RuntimeError::StepLimitExceeded { limit: 3, pc: 2 })
        );
        assert_eq!(machine.state.replay.steps_executed(), 3);
        assert_matches_core("+[]", &[], MachineConfig::with_max_steps(3));
    }
}
