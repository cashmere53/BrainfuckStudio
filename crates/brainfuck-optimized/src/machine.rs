//! 公開の実行機械(`OptimizedMachine`)。
//!
//! 最適化済みプログラムを、一括適用([`crate::fast`])と縮退再生([`crate::replay`])で完走まで実行する。
//! 縮退経路は公開の再生関数の呼び出しのみで実現し、この crate に素朴意味論の別実装を持たない。
//!
//! 観測 API(スナップショット・テープ・出力・入力位置・停止状態)は読み取り専用で、状態を変更しない。

use crate::fast::{execute, FastOutcome};
use crate::ir::OptimizedProgram;
use crate::replay::{replay, Replayed};
use crate::state::ExecState;
use brainfuck_core::{MachineConfig, RuntimeError, StopReason};

/// 実行状態の観測結果(読み取り専用の値)。
///
/// 実行数は 2 種類あり、名称どおりに別の定義である。
/// - `steps_executed` は素朴実装換算(実行した命令の総数)。素朴実装が同一時点で数える値と常に一致する。
/// - `ir_ops_executed` は IR 操作の実行回数(方式固有)。一括適用した操作のみを数え、縮退再生の命令は数えない。
///   縮退後は素朴再生で進むため更新されず、フォールバックが起きるまでに一括適用した IR 操作数を表す。
///
/// 常に `ir_ops_executed <= steps_executed` が成り立つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct OptimizedSnapshot {
    /// 命令添字。実行前は先頭操作の先頭の命令添字(操作が無ければ命令数)、停止時は命令数、
    /// エラー確定後はエラーが報告する添字。
    pub pc: usize,
    /// データポインタ(`0..TAPE_SIZE`)。
    pub data_pointer: usize,
    /// 現在のセルの値。
    pub current_cell: u8,
    /// 素朴実装換算の実行数(実行した命令の総数)。
    pub steps_executed: u64,
    /// IR 操作の実行回数。一括適用のみを数え、縮退再生の命令は数えない(フォールバック前までの操作数)。
    pub ir_ops_executed: u64,
    /// `Ok(StopReason::Halted)` が確定しているかどうか。
    pub halted: bool,
    /// 読み取り済みの入力バイト数。
    pub input_position: usize,
}

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

    /// 現在の観測結果を返す(読み取り専用。状態は変更しない)。
    pub fn snapshot(&self) -> OptimizedSnapshot {
        let replay = &self.state.replay;
        OptimizedSnapshot {
            pc: self.pc(),
            data_pointer: replay.data_pointer(),
            current_cell: replay.tape()[replay.data_pointer()],
            steps_executed: replay.steps_executed(),
            ir_ops_executed: self.state.ir_ops_executed,
            halted: self.is_halted(),
            input_position: replay.input_position(),
        }
    }

    /// テープ全体(長さ `TAPE_SIZE`)を読み取り専用で返す。
    pub fn tape(&self) -> &[u8] {
        self.state.replay.tape()
    }

    /// これまでに出力されたバイト列全体を読み取り専用で返す。
    pub fn output(&self) -> &[u8] {
        self.state.replay.output()
    }

    /// 読み取り済みの入力バイト数を返す。
    pub fn input_position(&self) -> usize {
        self.state.replay.input_position()
    }

    /// `Ok(StopReason::Halted)` が確定しているかを返す。エラー確定後と実行前は偽。
    pub fn is_halted(&self) -> bool {
        matches!(
            self.outcome,
            Some(Replayed {
                result: Ok(StopReason::Halted),
                ..
            })
        )
    }

    /// 借用している最適化済みプログラムを返す。
    pub fn program(&self) -> &'p OptimizedProgram {
        self.program
    }

    /// `pc` の現在値。確定後は確定時の `pc`、確定前は次に実行する操作の先頭の命令添字。
    fn pc(&self) -> usize {
        match self.outcome {
            Some(done) => done.pc,
            None => match self.program.ops().get(self.state.ir_pc) {
                Some(op) => op.source_range().start,
                None => self.program.source().len(),
            },
        }
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

    /// 観測 API(スナップショット・テープ・出力・入力位置・停止状態)を core の同名項目と比較する。
    fn assert_observations_match_core(source: &str, input: &[u8], config: MachineConfig) {
        let mut core = Machine::new(
            Program::parse(source).expect("valid program"),
            input.to_vec(),
            config,
        );
        let _ = core.run();
        let core_snapshot = core.snapshot();

        let program = compiled(source);
        let mut machine = OptimizedMachine::new(&program, input.to_vec(), config);
        let _ = machine.run();
        let snapshot = machine.snapshot();
        let label = format!("source {source:?}, input {input:?}, config {config:?}");
        assert_eq!(snapshot.pc, core_snapshot.pc, "pc: {label}");
        assert_eq!(
            snapshot.data_pointer, core_snapshot.data_pointer,
            "data pointer: {label}"
        );
        assert_eq!(
            snapshot.current_cell, core_snapshot.current_cell,
            "current cell: {label}"
        );
        assert_eq!(
            snapshot.steps_executed, core_snapshot.steps_executed,
            "steps: {label}"
        );
        assert_eq!(snapshot.halted, core_snapshot.halted, "halted: {label}");
        assert_eq!(machine.tape(), core.tape(), "tape: {label}");
        assert_eq!(machine.output(), core.output(), "output: {label}");
        assert_eq!(
            machine.input_position(),
            core.input_position(),
            "input position: {label}"
        );
        assert_eq!(
            snapshot.input_position,
            core.input_position(),
            "snapshot input position: {label}"
        );
        assert_eq!(machine.is_halted(), core.is_halted(), "halted: {label}");
        assert!(
            snapshot.ir_ops_executed <= snapshot.steps_executed,
            "ir ops exceed steps: {label}"
        );
    }

    #[test]
    fn snapshot_before_run_reports_the_initial_position() {
        let program = compiled("++[-]");
        let machine = OptimizedMachine::new(&program, vec![4], MachineConfig::unlimited());
        let snapshot = machine.snapshot();
        assert_eq!(snapshot.pc, 0);
        assert_eq!(snapshot.data_pointer, 0);
        assert_eq!(snapshot.current_cell, 0);
        assert_eq!(snapshot.steps_executed, 0);
        assert_eq!(snapshot.ir_ops_executed, 0);
        assert!(!snapshot.halted);
        assert_eq!(snapshot.input_position, 0);
        assert!(!machine.is_halted());
    }

    #[test]
    fn empty_program_is_not_halted_before_run() {
        let program = compiled("");
        let machine = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        assert_eq!(machine.snapshot().pc, 0);
        assert!(!machine.snapshot().halted);
        assert!(!machine.is_halted());
    }

    #[test]
    fn snapshot_at_halt_reports_both_execution_counts() {
        // 素朴換算 16 に対し、IR 操作は `+++++` と `[-]` の 2 回。
        let program = compiled("+++++[-]");
        let mut machine = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        assert_eq!(machine.run(), Ok(StopReason::Halted));
        let snapshot = machine.snapshot();
        assert_eq!(snapshot.pc, 8);
        assert_eq!(snapshot.data_pointer, 0);
        assert_eq!(snapshot.current_cell, 0);
        assert_eq!(snapshot.steps_executed, 16);
        assert_eq!(snapshot.ir_ops_executed, 2);
        assert!(snapshot.halted);
        assert_eq!(snapshot.input_position, 0);
        assert!(machine.is_halted());
        assert_observations_match_core("+++++[-]", &[], MachineConfig::unlimited());
    }

    #[test]
    fn snapshot_at_out_of_bounds_reports_the_error_position() {
        // `+++` は 1 回の一括適用(IR 操作 1)、`<` は範囲外の縮退で素朴再生が報告する。
        let program = compiled("+++<");
        let mut machine = OptimizedMachine::new(&program, Vec::new(), MachineConfig::unlimited());
        assert!(machine.run().is_err());
        let snapshot = machine.snapshot();
        assert_eq!(snapshot.pc, 3);
        assert_eq!(snapshot.data_pointer, 0);
        assert_eq!(snapshot.current_cell, 3);
        assert_eq!(snapshot.steps_executed, 3);
        assert_eq!(snapshot.ir_ops_executed, 1);
        assert!(!snapshot.halted);
        assert!(!machine.is_halted());
        assert_observations_match_core("+++<", &[], MachineConfig::unlimited());
    }

    #[test]
    fn snapshot_at_step_limit_matches_the_core_position() {
        let program = compiled("+++++[-]");
        let mut machine =
            OptimizedMachine::new(&program, Vec::new(), MachineConfig::with_max_steps(10));
        assert!(machine.run().is_err());
        let snapshot = machine.snapshot();
        assert_eq!(snapshot.steps_executed, 10);
        assert_eq!(snapshot.ir_ops_executed, 1);
        assert!(!snapshot.halted);
        assert_observations_match_core("+++++[-]", &[], MachineConfig::with_max_steps(10));
    }

    #[test]
    fn observation_reads_do_not_change_the_state() {
        let program = compiled(",[.,]");
        let mut machine = OptimizedMachine::new(&program, vec![1, 2], MachineConfig::unlimited());
        assert_eq!(machine.run(), Ok(StopReason::Halted));
        let before = machine.state.clone();
        let first = machine.snapshot();
        let _ = machine.snapshot();
        let _ = machine.tape();
        let _ = machine.output();
        let _ = machine.input_position();
        let _ = machine.is_halted();
        let _ = machine.program();
        assert_eq!(machine.snapshot(), first);
        assert_eq!(machine.state, before);
    }

    #[test]
    fn observations_match_the_core_machine_for_representative_programs() {
        assert_observations_match_core("++++++++[>++++++++<-]>+.", &[], MachineConfig::unlimited());
        assert_observations_match_core("+>+>+<<[>]", &[], MachineConfig::unlimited());
        assert_observations_match_core("+++[->++<]", &[], MachineConfig::unlimited());
        assert_observations_match_core("-.", &[], MachineConfig::unlimited());
        assert_observations_match_core(",[.,]", &[0x00, 0xFF, 7], MachineConfig::unlimited());
        assert_observations_match_core("<", &[], MachineConfig::unlimited());
        assert_observations_match_core("+[>+.<-]", &[], MachineConfig::unlimited());
    }

    #[test]
    fn observations_match_the_core_machine_at_every_step_limit() {
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
                assert_observations_match_core(
                    source,
                    &[1, 2, 3],
                    MachineConfig::with_max_steps(limit),
                );
            }
        }
    }
}
