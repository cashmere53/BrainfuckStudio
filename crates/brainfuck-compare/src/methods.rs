//! 3 方式の準備・実行・計測。
//!
//! 素朴(核の機械)・最適化(IR と最適化機械)・WASM(IR + 生成 + 監査 + ロード、実行)を、
//! 同じ呼び出し形で「準備時間 → 実行時間 → 観測値と方式固有の単位」を返す形に揃える。
//! 計測区間は準備と実行呼び出しのみで、入力の複製・結果の整形・転送は含めない。

use std::time::{Duration, Instant};

use brainfuck_core::{Machine, MachineConfig, Program};
use brainfuck_optimized::{OptimizedMachine, OptimizedProgram};
use brainfuck_wasm::{CompileError, RunError, WasmProgram};

use crate::observation::Observation;
use crate::report::{FailureKind, Method, Metrics, Units, UnsupportedReason};

/// 方式の 1 回の実行結果(観測値と計測値)。判定は含まない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    /// 完全状態の観測値。
    pub observation: Observation,
    /// 計測値(準備時間・実行時間・単位・WASM の成果物情報)。
    pub metrics: Metrics,
}

/// 方式が完走しなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MethodFailure {
    /// 対象外(規模・入れ子の上限超過)。
    Unsupported(UnsupportedReason),
    /// 失敗(方針違反・エンジン・実行量や出力量の監視違反など)。
    Failed(FailureKind),
}

/// 指定の方式を実行する。`run_naive` / `run_optimized` / `run_wasm` への振り分け。
///
/// 命令数上限は 0 を含む任意の値を受け付ける。
///
/// # Errors
///
/// 方式が対象外または失敗の場合は `MethodFailure` を返す。
pub fn run_method(
    method: Method,
    program: &Program,
    input: &[u8],
    limit: u64,
) -> Result<Execution, MethodFailure> {
    match method {
        Method::Naive => run_naive(program, input, limit),
        Method::Optimized => run_optimized(program, input, limit),
        Method::Wasm => run_wasm(program, input, limit),
    }
}

/// 素朴インタプリタ(核の機械)で実行する。準備はなく、単位は実行命令数。
///
/// # Errors
///
/// 素朴は対象外や失敗を持たないため、常に `Ok` を返す。
pub fn run_naive(program: &Program, input: &[u8], limit: u64) -> Result<Execution, MethodFailure> {
    let config = MachineConfig::with_max_steps(limit);
    let owned_program = program.clone();
    let owned_input = input.to_vec();

    let started = Instant::now();
    let mut machine = Machine::new(owned_program, owned_input, config);
    let result = machine.run();
    let run = started.elapsed();

    let snapshot = machine.snapshot();
    let observation = Observation {
        result,
        pc: snapshot.pc,
        data_pointer: snapshot.data_pointer,
        current_cell: snapshot.current_cell,
        steps_executed: snapshot.steps_executed,
        input_position: machine.input_position(),
        tape: machine.tape().into(),
        output: machine.output().to_vec(),
    };
    let units = Units::NaiveInstructions(observation.steps_executed);
    Ok(execution(observation, None, run, units, None, None))
}

/// 最適化インタプリタで実行する。準備は IR 生成、単位は IR 操作回数。
///
/// # Errors
///
/// 最適化は対象外や失敗を持たないため、常に `Ok` を返す。
pub fn run_optimized(
    program: &Program,
    input: &[u8],
    limit: u64,
) -> Result<Execution, MethodFailure> {
    let config = MachineConfig::with_max_steps(limit);
    let owned_input = input.to_vec();

    let started = Instant::now();
    let ir = OptimizedProgram::compile(program);
    let prepare = started.elapsed();

    let started = Instant::now();
    let mut machine = OptimizedMachine::new(&ir, owned_input, config);
    let result = machine.run();
    let run = started.elapsed();

    let snapshot = machine.snapshot();
    let observation = Observation {
        result,
        pc: snapshot.pc,
        data_pointer: snapshot.data_pointer,
        current_cell: snapshot.current_cell,
        steps_executed: snapshot.steps_executed,
        input_position: snapshot.input_position,
        tape: machine.tape().into(),
        output: machine.output().to_vec(),
    };
    let units = Units::IrOperations(snapshot.ir_ops_executed);
    Ok(execution(
        observation,
        Some(prepare),
        run,
        units,
        None,
        None,
    ))
}

/// WASM バックエンドで実行する。準備は IR 生成・生成・監査・ロード、単位はなし。
///
/// # Errors
///
/// 準備の失敗は `CompileError` の分類に従い、実行の失敗は `RunError` の分類に従う。
/// 実行の途中状態が不整合な場合は `EngineError` として返す。
pub fn run_wasm(program: &Program, input: &[u8], limit: u64) -> Result<Execution, MethodFailure> {
    let started = Instant::now();
    let ir = OptimizedProgram::compile(program);
    let wasm = WasmProgram::compile(&ir).map_err(compile_failure)?;
    let prepare = started.elapsed();

    let started = Instant::now();
    let run = wasm.run(input, limit).map_err(run_failure)?;
    let elapsed = started.elapsed();

    let current_cell = *run
        .tape
        .get(run.data_pointer)
        .ok_or(MethodFailure::Failed(FailureKind::EngineError))?;
    let observation = Observation {
        result: run.result,
        pc: run.pc,
        data_pointer: run.data_pointer,
        current_cell,
        steps_executed: run.steps_executed,
        input_position: run.input_position,
        tape: run.tape,
        output: run.output,
    };
    let artifact_bytes = wasm.module_bytes().len();
    Ok(execution(
        observation,
        Some(prepare),
        elapsed,
        Units::None,
        Some(artifact_bytes),
        Some(run.replayed_steps),
    ))
}

/// 1 回の実行結果を組み立てる。`repetitions` は単発の実行として 1 とする(繰り返しは比較側が記録する)。
fn execution(
    observation: Observation,
    prepare: Option<Duration>,
    run: Duration,
    units: Units,
    artifact_bytes: Option<usize>,
    replayed_steps: Option<u64>,
) -> Execution {
    Execution {
        observation,
        metrics: Metrics {
            prepare,
            run,
            repetitions: 1,
            units,
            artifact_bytes,
            replayed_steps,
        },
    }
}

/// コンパイル失敗の分類。規模・入れ子の超過は未対応、方針違反とエンジンの拒否は失敗。
fn compile_failure(error: CompileError) -> MethodFailure {
    match error {
        CompileError::ModuleTooLarge => {
            MethodFailure::Unsupported(UnsupportedReason::ModuleTooLarge)
        }
        CompileError::NestingTooDeep => {
            MethodFailure::Unsupported(UnsupportedReason::NestingTooDeep)
        }
        CompileError::PolicyViolation(_) => MethodFailure::Failed(FailureKind::PolicyRejected),
        CompileError::Engine => MethodFailure::Failed(FailureKind::EngineError),
        _ => MethodFailure::Failed(FailureKind::EngineError),
    }
}

/// 実行失敗の分類。監視・出力量の違反はそれぞれの失敗、不整合な途中状態とエンジンはエンジンの失敗。
fn run_failure(error: RunError) -> MethodFailure {
    match error {
        RunError::WatchdogTripped => MethodFailure::Failed(FailureKind::WatchdogTripped),
        RunError::OutputLimitExceeded => MethodFailure::Failed(FailureKind::OutputLimitExceeded),
        RunError::InvalidState | RunError::Engine => {
            MethodFailure::Failed(FailureKind::EngineError)
        }
        _ => MethodFailure::Failed(FailureKind::EngineError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainfuck_core::{PointerDirection, RuntimeError, StopReason};
    use brainfuck_wasm::PolicyViolation;

    const LIMIT: u64 = 100_000;

    fn parse(source: &str) -> Program {
        Program::parse(source).expect("テストのソースは括弧が整合している")
    }

    /// 核の機械を完走させた観測値(素朴方式の基準)。
    fn core_observation(source: &str, input: &[u8], limit: u64) -> Observation {
        let mut machine = Machine::new(
            parse(source),
            input.to_vec(),
            MachineConfig::with_max_steps(limit),
        );
        let result = machine.run();
        let snapshot = machine.snapshot();
        Observation {
            result,
            pc: snapshot.pc,
            data_pointer: snapshot.data_pointer,
            current_cell: snapshot.current_cell,
            steps_executed: snapshot.steps_executed,
            input_position: machine.input_position(),
            tape: machine.tape().into(),
            output: machine.output().to_vec(),
        }
    }

    #[test]
    fn all_methods_complete_representative_programs_and_match_core() {
        let cases: [(&str, &[u8]); 4] = [
            ("+++.", b""),
            ("++[>+++<-]>.", b""),
            (",[.,]", b"hi"),
            ("++[>+++++[>+<-]<-]>>.", b""),
        ];
        for (source, input) in cases {
            let expected = core_observation(source, input, LIMIT);
            let program = parse(source);
            for method in [Method::Naive, Method::Optimized, Method::Wasm] {
                let execution =
                    run_method(method, &program, input, LIMIT).unwrap_or_else(|failure| {
                        panic!("{method:?} が {source:?} で失敗: {failure:?}")
                    });
                assert_eq!(
                    execution.observation, expected,
                    "{method:?} の観測が核と一致しない: {source:?}"
                );
            }
        }
    }

    #[test]
    fn naive_has_no_preparation_and_reports_instruction_units() {
        let source = "++[>+++<-]>.";
        let execution = run_naive(&parse(source), b"", LIMIT).expect("素朴は完走する");
        assert_eq!(execution.metrics.prepare, None);
        assert_eq!(
            execution.metrics.units,
            Units::NaiveInstructions(execution.observation.steps_executed)
        );
        assert_eq!(execution.metrics.artifact_bytes, None);
        assert_eq!(execution.metrics.replayed_steps, None);
        assert_eq!(execution.metrics.repetitions, 1);
    }

    #[test]
    fn optimized_reports_ir_operation_units_not_exceeding_naive_steps() {
        let source = "++[>+++<-]>.";
        let execution = run_optimized(&parse(source), b"", LIMIT).expect("最適化は完走する");
        assert!(
            execution.metrics.prepare.is_some(),
            "IR 生成を準備として計測する"
        );
        match execution.metrics.units {
            Units::IrOperations(ops) => {
                assert!(ops > 0, "一括適用した IR 操作がある");
                assert!(ops <= execution.observation.steps_executed);
            }
            other => panic!("最適化の単位は IR 操作回数であるべき: {other:?}"),
        }
        assert_eq!(execution.metrics.artifact_bytes, None);
        assert_eq!(execution.metrics.replayed_steps, None);
    }

    #[test]
    fn wasm_reports_no_units_with_artifact_size_and_preparation() {
        let source = "+++.";
        let execution = run_wasm(&parse(source), b"", LIMIT).expect("WASM は完走する");
        assert!(
            execution.metrics.prepare.is_some(),
            "IR・生成・監査・ロードを準備として計測する"
        );
        assert_eq!(execution.metrics.units, Units::None);
        assert!(
            execution
                .metrics
                .artifact_bytes
                .is_some_and(|bytes| bytes > 0),
            "モジュールのバイト数を返す"
        );
        assert_eq!(
            execution.metrics.replayed_steps,
            Some(0),
            "離脱なしの直線的なプログラムはホスト再生の命令数が 0"
        );
    }

    #[test]
    fn wasm_artifact_and_observation_are_deterministic() {
        let source = "++[>+++<-]>.";
        let first = run_wasm(&parse(source), b"", LIMIT).expect("完走する");
        let second = run_wasm(&parse(source), b"", LIMIT).expect("完走する");
        assert_eq!(first.observation, second.observation);
        assert_eq!(first.metrics.artifact_bytes, second.metrics.artifact_bytes);
    }

    #[test]
    fn naive_is_deterministic_apart_from_time() {
        let source = ",[.,]";
        let first = run_naive(&parse(source), b"abc", LIMIT).expect("完走する");
        let second = run_naive(&parse(source), b"abc", LIMIT).expect("完走する");
        assert_eq!(first.observation, second.observation);
        assert_eq!(first.observation.output, b"abc".to_vec());
    }

    #[test]
    fn limit_sweep_from_zero_matches_core_in_all_methods() {
        let source = "+[>++<-]>.+";
        let program = parse(source);
        for limit in 0..=20 {
            let expected = core_observation(source, &[], limit);
            for method in [Method::Naive, Method::Optimized, Method::Wasm] {
                let execution =
                    run_method(method, &program, &[], limit).unwrap_or_else(|failure| {
                        panic!("{method:?} が上限 {limit} で失敗: {failure:?}")
                    });
                assert_eq!(
                    execution.observation, expected,
                    "{method:?} が上限 {limit} で核と一致しない"
                );
            }
        }
    }

    #[test]
    fn empty_program_with_zero_limit_halts_in_all_methods() {
        let program = parse("");
        for method in [Method::Naive, Method::Optimized, Method::Wasm] {
            let execution = run_method(method, &program, &[], 0).expect("完走する");
            assert_eq!(execution.observation.result, Ok(StopReason::Halted));
            assert_eq!(execution.observation.pc, 0);
            assert_eq!(execution.observation.steps_executed, 0);
        }
    }

    #[test]
    fn step_limit_error_is_an_observation_not_a_failure() {
        let source = "+++++";
        let program = parse(source);
        let expected = Err(RuntimeError::StepLimitExceeded { limit: 0, pc: 0 });
        for method in [Method::Naive, Method::Optimized, Method::Wasm] {
            let execution = run_method(method, &program, &[], 0).expect("エラーは観測として返る");
            assert_eq!(execution.observation.result, expected, "{method:?}");
        }
    }

    #[test]
    fn pointer_out_of_bounds_is_an_observation_in_all_methods() {
        let program = parse("<");
        let expected = core_observation("<", &[], 10);
        assert_eq!(
            expected.result,
            Err(RuntimeError::PointerOutOfBounds {
                pc: 0,
                dp: 0,
                direction: PointerDirection::Left,
            })
        );
        for method in [Method::Naive, Method::Optimized, Method::Wasm] {
            let execution = run_method(method, &program, &[], 10).expect("エラーは観測として返る");
            assert_eq!(execution.observation, expected, "{method:?}");
        }
    }

    #[test]
    fn nesting_beyond_the_wasm_limit_is_unsupported_and_the_limit_itself_is_supported() {
        let deep = |depth: usize| format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        // 入れ子 256(上限ちょうど)は対象。セル 0 が 0 なので先頭の '[' で終端へ飛ぶ。
        let at_limit = parse(&deep(256));
        let supported = run_wasm(&at_limit, &[], LIMIT).expect("上限ちょうどの入れ子は完走する");
        assert_eq!(supported.observation.result, Ok(StopReason::Halted));

        let over_limit = parse(&deep(257));
        assert_eq!(
            run_wasm(&over_limit, &[], LIMIT),
            Err(MethodFailure::Unsupported(
                UnsupportedReason::NestingTooDeep
            ))
        );
        // 素朴には入れ子の上限がないため、同じプログラムを完走させる。
        let naive = run_naive(&over_limit, &[], LIMIT).expect("素朴は完走する");
        assert_eq!(naive.observation.result, Ok(StopReason::Halted));
    }

    #[test]
    fn compile_errors_are_classified_as_unsupported_or_failed() {
        assert_eq!(
            compile_failure(CompileError::ModuleTooLarge),
            MethodFailure::Unsupported(UnsupportedReason::ModuleTooLarge)
        );
        assert_eq!(
            compile_failure(CompileError::NestingTooDeep),
            MethodFailure::Unsupported(UnsupportedReason::NestingTooDeep)
        );
        assert_eq!(
            compile_failure(CompileError::PolicyViolation(PolicyViolation::Start)),
            MethodFailure::Failed(FailureKind::PolicyRejected)
        );
        assert_eq!(
            compile_failure(CompileError::Engine),
            MethodFailure::Failed(FailureKind::EngineError)
        );
    }

    #[test]
    fn run_errors_are_classified_as_resource_or_engine_failures() {
        assert_eq!(
            run_failure(RunError::WatchdogTripped),
            MethodFailure::Failed(FailureKind::WatchdogTripped)
        );
        assert_eq!(
            run_failure(RunError::OutputLimitExceeded),
            MethodFailure::Failed(FailureKind::OutputLimitExceeded)
        );
        assert_eq!(
            run_failure(RunError::InvalidState),
            MethodFailure::Failed(FailureKind::EngineError)
        );
        assert_eq!(
            run_failure(RunError::Engine),
            MethodFailure::Failed(FailureKind::EngineError)
        );
    }
}
