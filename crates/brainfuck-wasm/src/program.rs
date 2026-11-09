//! 公開窓口(`WasmProgram` / `WasmRun`)と離脱後の再生の接続。
//!
//! `compile` は 生成 → 監査 → ロード を行い、`run` は 実行 → 離脱時の上流再生 → 結果確定 を行う。
//! 素朴意味論の再生は持たず、離脱時の途中状態を上流 `brainfuck_optimized::replay` へ渡して読み戻すだけである。

use brainfuck_core::{Program, RuntimeError, StopReason, TAPE_SIZE};
use brainfuck_optimized::replay::{replay, ReplayParts, ReplayState};
use brainfuck_optimized::OptimizedProgram;

use crate::audit::audit;
use crate::emit::emit;
use crate::error::{CompileError, RunError};
use crate::runtime::{self, Loaded, RawRun, Status};

/// コンパイル済みのプログラム。生成物は自己生成のみで、外部供給のモジュールを受け取る入口は持たない。
/// `run` は `&self` で呼べ、実行ごとに新しいストアとインスタンスを作るため、呼び出し同士は互いに影響しない。
pub struct WasmProgram {
    source: Program,
    module: Vec<u8>,
    loaded: Loaded,
}

impl std::fmt::Debug for WasmProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmProgram")
            .field("module_bytes", &self.module.len())
            .finish_non_exhaustive()
    }
}

/// 1 回の実行の観測値。素朴実装の観測値と全項目が一致する(`bailed_out` と `replayed_steps` を除く)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmRun {
    /// 停止、またはエラー(上限到達・範囲外)。
    pub result: Result<StopReason, RuntimeError>,
    /// 終了時の命令添字。停止は命令数。
    pub pc: usize,
    /// 終了時のデータポインタ。
    pub data_pointer: usize,
    /// 素朴実装換算の実行数。
    pub steps_executed: u64,
    /// 終了時の入力読取位置。
    pub input_position: usize,
    /// 出力バイト列。
    pub output: Vec<u8>,
    /// テープ全体(長さ `TAPE_SIZE`)。
    pub tape: Box<[u8]>,
    /// 生成コードが離脱し、上流の再生で継続したか。
    pub bailed_out: bool,
    /// ホスト再生が実行した命令数(離脱がなければ 0)。
    pub replayed_steps: u64,
}

impl WasmProgram {
    /// IR からコンパイルする(生成 → 監査 → ロード)。
    ///
    /// # Errors
    ///
    /// 生成物が大きすぎる場合は `ModuleTooLarge`、方針に違反する場合は `PolicyViolation`、
    /// エンジンが受け付けない場合は `Engine`。
    pub fn compile(ir: &OptimizedProgram) -> Result<WasmProgram, CompileError> {
        let module = emit(ir)?;
        audit(&module).map_err(CompileError::PolicyViolation)?;
        let loaded = runtime::load(&module)?;
        Ok(WasmProgram {
            source: ir.source().clone(),
            module,
            loaded,
        })
    }

    /// 生成した WASM バイナリ。成果物のバイト数はこの長さで参照する。
    pub fn module_bytes(&self) -> &[u8] {
        &self.module
    }

    /// 実行する。`limit` は命令数上限(有限)。離脱時は上流の再生で停止またはエラーまで進める。
    ///
    /// # Errors
    ///
    /// 方式の失敗(燃料枯渇・出力量超過・不整合な途中状態・エンジン失敗)のみを返す。
    /// Brainfuck の実行エラー(上限・範囲外)は `Ok(WasmRun)` の `result` に現れる。
    pub fn run(&self, input: &[u8], limit: u64) -> Result<WasmRun, RunError> {
        let raw = runtime::run(&self.loaded, input, limit)?;
        finish(&self.source, raw, input, limit)
    }
}

/// 生成コードの戻り値(`RawRun`)を観測値へ確定する。離脱時のみ上流の再生へ接続する。
fn finish(program: &Program, raw: RawRun, input: &[u8], limit: u64) -> Result<WasmRun, RunError> {
    match raw.status {
        Status::Halted => {
            // 停止時の pc は命令数と一致しなければならない。
            if raw.pc != program.len() {
                return Err(RunError::InvalidState);
            }
            check_position(raw.dp, raw.input_pos, input.len())?;
            Ok(WasmRun {
                result: Ok(StopReason::Halted),
                pc: raw.pc,
                data_pointer: raw.dp,
                steps_executed: raw.steps,
                input_position: raw.input_pos,
                output: raw.output,
                tape: raw.tape,
                bailed_out: false,
                replayed_steps: 0,
            })
        }
        Status::Bailout => {
            let steps_before = raw.steps;
            let parts = ReplayParts {
                tape: raw.tape.into_vec(),
                dp: raw.dp,
                steps_executed: raw.steps,
                input: input.to_vec(),
                input_pos: raw.input_pos,
                output: raw.output,
            };
            let mut state = ReplayState::from_parts(parts).map_err(|_| RunError::InvalidState)?;
            let replayed = replay(program, Some(limit), raw.pc, &mut state);
            let parts = state.into_parts();
            Ok(WasmRun {
                result: replayed.result,
                pc: replayed.pc,
                data_pointer: parts.dp,
                steps_executed: parts.steps_executed,
                input_position: parts.input_pos,
                output: parts.output,
                tape: parts.tape.into_boxed_slice(),
                bailed_out: true,
                replayed_steps: parts.steps_executed - steps_before,
            })
        }
    }
}

/// 停止時の状態の整合を確かめる(データポインタの範囲、入力位置の範囲)。
fn check_position(dp: usize, input_pos: usize, input_len: usize) -> Result<(), RunError> {
    if dp >= TAPE_SIZE || input_pos > input_len {
        return Err(RunError::InvalidState);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainfuck_core::{PointerDirection, Program, RuntimeError, StopReason, TAPE_SIZE};
    use brainfuck_optimized::OptimizedProgram;

    fn compile_src(source: &str) -> WasmProgram {
        let program = Program::parse(source).expect("解析できる");
        let ir = OptimizedProgram::compile(&program);
        WasmProgram::compile(&ir).expect("直線的なプログラムはコンパイルできる")
    }

    #[test]
    fn linear_program_runs_through_public_window_without_bailout() {
        let wasm = compile_src("+++.");
        let run = wasm.run(&[], 1_000).expect("実行できる");
        assert_eq!(run.result, Ok(StopReason::Halted));
        assert_eq!(run.pc, 4);
        assert_eq!(run.data_pointer, 0);
        assert_eq!(run.steps_executed, 4);
        assert_eq!(run.input_position, 0);
        assert_eq!(run.output, vec![3]);
        assert_eq!(run.tape.len(), TAPE_SIZE);
        assert_eq!(run.tape[0], 3);
        assert!(!run.bailed_out, "離脱なしの結果は離脱の有無が偽");
        assert_eq!(run.replayed_steps, 0, "離脱なしならホスト再生の命令数は 0");
    }

    #[test]
    fn empty_program_halts_immediately() {
        let wasm = compile_src("");
        let run = wasm.run(&[], 0).expect("上限 0 でも空プログラムは停止する");
        assert_eq!(run.result, Ok(StopReason::Halted));
        assert_eq!(run.pc, 0);
        assert_eq!(run.steps_executed, 0);
        assert!(run.output.is_empty());
        assert!(!run.bailed_out);
        assert_eq!(run.replayed_steps, 0);
    }

    #[test]
    fn runs_are_independent_across_inputs_and_limits() {
        let wasm = compile_src(",+.");
        let first = wasm.run(&[41], 100).expect("実行できる");
        let second = wasm.run(&[0], 100).expect("実行できる");
        let limited = wasm.run(&[41], 1).expect("実行できる");
        let again = wasm.run(&[41], 100).expect("実行できる");

        assert_eq!(first.output, vec![42]);
        assert_eq!(second.output, vec![1]);
        assert_eq!(first, again, "同じ入力・上限の結果は実行の順序に依らない");
        assert_eq!(first.input_position, 1);
        assert_eq!(second.input_position, 1);
        assert_eq!(
            limited.result,
            Err(RuntimeError::StepLimitExceeded { limit: 1, pc: 1 })
        );
    }

    #[test]
    fn module_bytes_are_exposed_and_deterministic() {
        let a = compile_src("+[-].");
        let b = compile_src("+[-].");
        assert!(!a.module_bytes().is_empty());
        assert_eq!(
            a.module_bytes(),
            b.module_bytes(),
            "同一 IR から同一バイト列"
        );
    }

    #[test]
    fn step_limit_inside_linear_op_bails_out_and_replays_upstream() {
        // 5 命令の加算を上限 3 で実行: 生成コードは操作の先頭で離脱し、上流の再生が 3 命令で止まる。
        let wasm = compile_src("+++++");
        let run = wasm.run(&[], 3).expect("実行できる");
        assert_eq!(
            run.result,
            Err(RuntimeError::StepLimitExceeded { limit: 3, pc: 3 })
        );
        assert_eq!(run.pc, 3);
        assert_eq!(run.data_pointer, 0);
        assert_eq!(run.steps_executed, 3);
        assert_eq!(run.tape[0], 3);
        assert!(run.bailed_out);
        assert_eq!(run.replayed_steps, 3);
    }

    #[test]
    fn pointer_out_of_bounds_bails_out_and_reports_upstream_error() {
        let wasm = compile_src("<");
        let run = wasm.run(&[], 1_000).expect("実行できる");
        assert_eq!(
            run.result,
            Err(RuntimeError::PointerOutOfBounds {
                pc: 0,
                dp: 0,
                direction: PointerDirection::Left,
            })
        );
        assert_eq!(run.data_pointer, 0, "範囲外のエラーは状態を変えない");
        assert!(run.bailed_out);
        assert_eq!(run.replayed_steps, 0);
    }

    #[test]
    fn input_exhaustion_reads_zero_through_public_window() {
        let wasm = compile_src(",.");
        let run = wasm.run(&[], 100).expect("実行できる");
        assert_eq!(run.output, vec![0]);
        assert_eq!(run.input_position, 0);
        assert!(!run.bailed_out);
    }

    fn raw_bailout(dp: usize, tape_len: usize, input_pos: usize) -> RawRun {
        RawRun {
            status: Status::Bailout,
            pc: 0,
            dp,
            steps: 0,
            tape: vec![0u8; tape_len].into_boxed_slice(),
            input_pos,
            output: Vec::new(),
        }
    }

    #[test]
    fn inconsistent_bailout_state_is_rejected_as_invalid_state() {
        let program = Program::parse("+").expect("解析できる");
        let input = [1u8, 2];

        // dp が範囲外
        let raw = raw_bailout(TAPE_SIZE, TAPE_SIZE, 0);
        assert_eq!(
            finish(&program, raw, &input, 10),
            Err(RunError::InvalidState)
        );

        // テープ長の不足
        let raw = raw_bailout(0, TAPE_SIZE - 1, 0);
        assert_eq!(
            finish(&program, raw, &input, 10),
            Err(RunError::InvalidState)
        );

        // 入力位置が入力長を超える
        let raw = raw_bailout(0, TAPE_SIZE, input.len() + 1);
        assert_eq!(
            finish(&program, raw, &input, 10),
            Err(RunError::InvalidState)
        );
    }

    #[test]
    fn inconsistent_halted_state_is_rejected_as_invalid_state() {
        let program = Program::parse("+").expect("解析できる");
        let raw = RawRun {
            status: Status::Halted,
            pc: 1,
            dp: TAPE_SIZE,
            steps: 1,
            tape: vec![0u8; TAPE_SIZE].into_boxed_slice(),
            input_pos: 0,
            output: Vec::new(),
        };
        assert_eq!(finish(&program, raw, &[], 10), Err(RunError::InvalidState));
    }

    #[test]
    fn halted_raw_run_is_reported_without_replay() {
        let program = Program::parse("+").expect("解析できる");
        let mut tape = vec![0u8; TAPE_SIZE];
        tape[0] = 1;
        let raw = RawRun {
            status: Status::Halted,
            pc: 1,
            dp: 0,
            steps: 1,
            tape: tape.into_boxed_slice(),
            input_pos: 0,
            output: Vec::new(),
        };
        let run = finish(&program, raw, &[], 10).expect("整合した停止は受け付ける");
        assert_eq!(run.result, Ok(StopReason::Halted));
        assert_eq!(run.pc, 1);
        assert_eq!(run.steps_executed, 1);
        assert!(!run.bailed_out);
        assert_eq!(run.replayed_steps, 0);
    }
}
