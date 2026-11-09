//! `ComparisonService`: 比較の要求検証・解析・比較の呼び出し・DTO 変換。
//!
//! 検証の順序は「要求の範囲 → 解析 → 比較 → DTO 変換」。要求の範囲外は操作エラーで拒否し、
//! 方式は一切実行しない。解析エラーは `ComparisonResultDto::ParseError` として返し、方式は実行しない。
//! テープ窓・出力末尾・位置・解析エラーの写像は上流(`brainfuck-core` / `brainfuck-gui-api`)の
//! 部品を呼ぶだけで、本モジュールは独自の計算を持たない。
//!
//! 比較は `Mutex<()>` で直列化する。ガードは状態を持たないため、汚染されても回復して続ける。
//! `DebuggerService` は共有・参照しない。

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use brainfuck_compare::compare::{compare, CompareError, ComparisonRequest};
use brainfuck_compare::limits::{DEFAULT_REPETITIONS, MAX_COMPARISON_STEPS, MAX_REPETITIONS};
use brainfuck_compare::{
    Comparison, FailureKind, Field, Method, MethodReport, Metrics, Observation, Outcome, Overall,
    Units, UnsupportedReason, Verdict,
};
use brainfuck_core::{tape_window_of, Program, StopReason, DEFAULT_WINDOW_RADIUS};
use brainfuck_gui_api::codec::{self, CodecError};
use brainfuck_gui_api::convert::{
    output_chunk, parse_error_dto, runtime_error_dto, tape_window_dto,
};
use brainfuck_gui_api::dto::{ApiError, ApiErrorKind, OutputChunkDto};
use brainfuck_gui_api::limits::{MAX_INPUT_BYTES, MAX_SOURCE_BYTES, MAX_STEP_LIMIT};

use crate::dto::{
    ComparisonInfoDto, ComparisonMethodDto, ComparisonResultDto, ExitStateDto, FailureKindDto,
    MethodResultDto, MetricsDto, MismatchFieldDto, OverallDto, RunComparisonRequest,
    TerminationDto, UnitDto, UnsupportedReasonDto, VerdictDto,
};

/// 内部エラー(固定文面。上流の想定外の値・不変条件違反に使う)。
fn internal() -> ApiError {
    ApiError::new(ApiErrorKind::Internal, None)
}

/// 要求の項目名を付けた不正の操作エラー。
fn invalid(field: &str) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidArgument, Some(field))
}

/// 比較サービス。`Send + Sync`。
pub struct ComparisonService {
    /// 比較の直列化ガード(状態は持たない)。
    guard: Mutex<()>,
}

impl Default for ComparisonService {
    fn default() -> Self {
        Self::new()
    }
}

impl ComparisonService {
    /// 空のサービスを作る。
    pub fn new() -> Self {
        Self {
            guard: Mutex::new(()),
        }
    }

    /// 比較の定数と、このビルドで利用できる方式を返す。
    pub fn info(&self) -> ComparisonInfoDto {
        ComparisonInfoDto {
            max_steps: MAX_COMPARISON_STEPS,
            max_repetitions: MAX_REPETITIONS,
            default_repetitions: DEFAULT_REPETITIONS,
            methods: vec![
                ComparisonMethodDto::Naive,
                ComparisonMethodDto::Optimized,
                ComparisonMethodDto::Wasm,
            ],
        }
    }

    /// 比較を実行する。要求の範囲外は `ApiError` で拒否し、方式は実行しない。
    ///
    /// 解析エラーは `ComparisonResultDto::ParseError` で返す(方式は実行しない)。
    pub fn run(&self, req: RunComparisonRequest) -> Result<ComparisonResultDto, ApiError> {
        let input = validate(&req)?;
        let methods: Vec<Method> = req.methods.iter().copied().map(method_of).collect();

        let program = match Program::parse(&req.source) {
            Ok(program) => program,
            Err(error) => {
                return Ok(ComparisonResultDto::ParseError {
                    error: parse_error_dto(&req.source, &error)?,
                })
            }
        };

        // ガードは状態を持たないため、汚染されても回復して続ける(要件 12.5)。
        let _guard = self.guard.lock().unwrap_or_else(PoisonError::into_inner);
        let comparison = compare(&ComparisonRequest {
            program: &program,
            input: &input,
            limit: req.max_steps,
            methods: &methods,
            repetitions: req.repetitions,
        })
        .map_err(compare_error)?;
        completed_dto(&comparison, input.len())
    }
}

/// 要求の範囲を検査し、復号した入力を返す。拒否の順序は項目の並びどおり。
fn validate(req: &RunComparisonRequest) -> Result<Vec<u8>, ApiError> {
    if req.source.len() > MAX_SOURCE_BYTES {
        return Err(ApiError::new(ApiErrorKind::SourceTooLarge, Some("source")));
    }
    let input = codec::decode(&req.input_base64, MAX_INPUT_BYTES).map_err(|error| match error {
        CodecError::TooLong { .. } => {
            ApiError::new(ApiErrorKind::InputTooLarge, Some("inputBase64"))
        }
        CodecError::InvalidBase64 => invalid("inputBase64"),
    })?;
    if req.max_steps == 0 || req.max_steps > MAX_COMPARISON_STEPS {
        return Err(invalid("maxSteps"));
    }
    if req.repetitions == 0 || req.repetitions > MAX_REPETITIONS {
        return Err(invalid("repetitions"));
    }
    for (index, method) in req.methods.iter().enumerate() {
        // 素朴は常に実行されるため、要求に含めるのも重複させるのも拒否する。
        if *method == ComparisonMethodDto::Naive || req.methods[..index].contains(method) {
            return Err(invalid("methods"));
        }
    }
    Ok(input)
}

/// 比較の範囲外エラーを操作エラーへ写す。上流の非網羅列挙の未知の値は内部エラー。
fn compare_error(error: CompareError) -> ApiError {
    match error {
        CompareError::InvalidLimit => invalid("maxSteps"),
        CompareError::InvalidRepetitions => invalid("repetitions"),
        _ => internal(),
    }
}

fn method_of(method: ComparisonMethodDto) -> Method {
    match method {
        ComparisonMethodDto::Naive => Method::Naive,
        ComparisonMethodDto::Optimized => Method::Optimized,
        ComparisonMethodDto::Wasm => Method::Wasm,
    }
}

fn method_dto(method: Method) -> Result<ComparisonMethodDto, ApiError> {
    match method {
        Method::Naive => Ok(ComparisonMethodDto::Naive),
        Method::Optimized => Ok(ComparisonMethodDto::Optimized),
        Method::Wasm => Ok(ComparisonMethodDto::Wasm),
        _ => Err(internal()),
    }
}

fn completed_dto(
    comparison: &Comparison,
    input_total: usize,
) -> Result<ComparisonResultDto, ApiError> {
    let methods = comparison
        .reports
        .iter()
        .map(|report| method_result_dto(report, input_total))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ComparisonResultDto::Completed {
        limit: comparison.limit,
        overall: overall_dto(comparison.overall),
        methods,
    })
}

fn overall_dto(overall: Overall) -> OverallDto {
    match overall {
        Overall::AllMatch => OverallDto::AllMatch,
        Overall::Mismatch => OverallDto::Mismatch,
        Overall::Incomplete => OverallDto::Incomplete,
    }
}

fn method_result_dto(
    report: &MethodReport,
    input_total: usize,
) -> Result<MethodResultDto, ApiError> {
    let method = method_dto(report.method)?;
    match &report.outcome {
        Outcome::Completed {
            observation,
            verdict,
            metrics,
        } => Ok(MethodResultDto::Completed {
            method,
            verdict: verdict_dto(verdict)?,
            exit: Box::new(exit_state(observation, input_total)?),
            metrics: metrics_dto(metrics)?,
        }),
        Outcome::Unsupported(reason) => Ok(MethodResultDto::Unsupported {
            method,
            reason: unsupported_dto(*reason)?,
        }),
        Outcome::Failed(kind) => Ok(MethodResultDto::Failed {
            method,
            failure: failure_dto(*kind)?,
        }),
    }
}

fn verdict_dto(verdict: &Verdict) -> Result<VerdictDto, ApiError> {
    Ok(match verdict {
        Verdict::Oracle => VerdictDto::Oracle,
        Verdict::Match => VerdictDto::Match,
        Verdict::Mismatch(fields) => VerdictDto::Mismatch {
            fields: fields
                .iter()
                .map(|field| mismatch_dto(*field))
                .collect::<Result<Vec<_>, _>>()?,
        },
    })
}

fn mismatch_dto(field: Field) -> Result<MismatchFieldDto, ApiError> {
    match field {
        Field::Termination => Ok(MismatchFieldDto::Termination),
        Field::ErrorValue => Ok(MismatchFieldDto::ErrorValue),
        Field::Pc => Ok(MismatchFieldDto::Pc),
        Field::DataPointer => Ok(MismatchFieldDto::DataPointer),
        Field::CurrentCell => Ok(MismatchFieldDto::CurrentCell),
        Field::StepsExecuted => Ok(MismatchFieldDto::StepsExecuted),
        Field::InputPosition => Ok(MismatchFieldDto::InputPosition),
        Field::Output => Ok(MismatchFieldDto::Output),
        Field::Tape => Ok(MismatchFieldDto::Tape),
        _ => Err(internal()),
    }
}

fn unsupported_dto(reason: UnsupportedReason) -> Result<UnsupportedReasonDto, ApiError> {
    match reason {
        UnsupportedReason::ModuleTooLarge => Ok(UnsupportedReasonDto::ModuleTooLarge),
        UnsupportedReason::NestingTooDeep => Ok(UnsupportedReasonDto::NestingTooDeep),
        _ => Err(internal()),
    }
}

fn failure_dto(kind: FailureKind) -> Result<FailureKindDto, ApiError> {
    match kind {
        FailureKind::WatchdogTripped => Ok(FailureKindDto::WatchdogTripped),
        FailureKind::OutputLimitExceeded => Ok(FailureKindDto::OutputLimitExceeded),
        FailureKind::EngineError => Ok(FailureKindDto::EngineError),
        FailureKind::PolicyRejected => Ok(FailureKindDto::PolicyRejected),
        FailureKind::InconsistentRepetitions => Ok(FailureKindDto::InconsistentRepetitions),
        _ => Err(internal()),
    }
}

/// 時間をマイクロ秒にする。JSON で正確に表せる範囲(2^53 − 1)で頭打ちにする。
fn micros(duration: Duration) -> u64 {
    let value = u64::try_from(duration.as_micros()).unwrap_or(MAX_STEP_LIMIT);
    value.min(MAX_STEP_LIMIT)
}

fn metrics_dto(metrics: &Metrics) -> Result<MetricsDto, ApiError> {
    Ok(MetricsDto {
        prepare_micros: metrics.prepare.map(micros),
        run_micros: micros(metrics.run),
        repetitions: metrics.repetitions,
        unit: units_dto(metrics.units)?,
        artifact_bytes: metrics
            .artifact_bytes
            .map(|bytes| u64::try_from(bytes).map_err(|_| internal()))
            .transpose()?,
        replayed_steps: metrics.replayed_steps,
    })
}

fn units_dto(units: Units) -> Result<UnitDto, ApiError> {
    match units {
        Units::NaiveInstructions(count) => Ok(UnitDto::NaiveInstructions { count }),
        Units::IrOperations(count) => Ok(UnitDto::IrOperations { count }),
        Units::None => Ok(UnitDto::None),
        _ => Err(internal()),
    }
}

/// 観測値を終了状態の DTO へ写す。テープ窓・出力末尾・指紋・エラーは上流の部品をそのまま使う。
///
/// 窓が空になるのは `dp` がテープ範囲外のときで、観測値の不変条件違反として内部エラーにする(panic しない)。
fn exit_state(observation: &Observation, input_total: usize) -> Result<ExitStateDto, ApiError> {
    let termination = match &observation.result {
        Ok(StopReason::Halted) => TerminationDto::Halted,
        Ok(_) => return Err(internal()),
        Err(error) => TerminationDto::Error {
            error: runtime_error_dto(error)?,
        },
    };
    let window = tape_window_of(
        &observation.tape,
        observation.data_pointer,
        DEFAULT_WINDOW_RADIUS,
    );
    if window.cells.is_empty() {
        return Err(internal());
    }
    let output_tail: OutputChunkDto = output_chunk(&observation.output, 0);
    Ok(ExitStateDto {
        termination,
        pc: observation.pc,
        dp: observation.data_pointer,
        current_cell: observation.current_cell,
        steps_executed: observation.steps_executed,
        input_consumed: observation.input_position,
        input_total,
        output_tail,
        tape_window: tape_window_dto(&window),
        tape_digest: format!("{:016x}", observation.tape_digest()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainfuck_core::{RuntimeError, TAPE_SIZE};

    #[test]
    fn out_of_range_pointer_observation_is_an_internal_error_not_a_panic() {
        // dp がテープ範囲外の観測値は不変条件違反。窓が空のため内部エラーになる(panic しない)。
        let observation = Observation {
            result: Ok(StopReason::Halted),
            pc: 0,
            data_pointer: TAPE_SIZE,
            current_cell: 0,
            steps_executed: 0,
            input_position: 0,
            tape: vec![0u8; TAPE_SIZE].into_boxed_slice(),
            output: Vec::new(),
        };
        let error = exit_state(&observation, 0).expect_err("empty window must be rejected");
        assert_eq!(error.kind, ApiErrorKind::Internal);
        assert_eq!(error.field, None);
    }

    #[test]
    fn runtime_error_observation_maps_to_the_error_termination() {
        let observation = Observation {
            result: Err(RuntimeError::StepLimitExceeded { limit: 5, pc: 1 }),
            pc: 1,
            data_pointer: 0,
            current_cell: 0,
            steps_executed: 5,
            input_position: 0,
            tape: vec![0u8; TAPE_SIZE].into_boxed_slice(),
            output: Vec::new(),
        };
        let exit = exit_state(&observation, 0).expect("exit state");
        assert_eq!(
            exit.termination,
            TerminationDto::Error {
                error: brainfuck_gui_api::dto::RuntimeErrorDto::StepLimitExceeded {
                    limit: 5,
                    pc: 1,
                },
            }
        );
    }

    fn tiny_request() -> RunComparisonRequest {
        RunComparisonRequest {
            source: "+".to_string(),
            input_base64: String::new(),
            max_steps: 10,
            methods: Vec::new(),
            repetitions: 1,
        }
    }

    #[test]
    fn run_waits_for_the_serialization_guard_and_completes_after_release() {
        use std::sync::{mpsc, Arc};

        let service = Arc::new(ComparisonService::new());
        let held = service.guard.lock().expect("hold the guard");
        let (sender, receiver) = mpsc::channel();
        let worker = {
            let service = Arc::clone(&service);
            std::thread::spawn(move || {
                let outcome = service.run(tiny_request()).is_ok();
                let _ = sender.send(outcome);
            })
        };
        // 保持中は完了しない(ロックを除くと即座に完了して失敗する)。
        assert!(
            receiver.recv_timeout(Duration::from_millis(300)).is_err(),
            "run must wait while the guard is held"
        );
        drop(held);
        assert_eq!(receiver.recv_timeout(Duration::from_secs(60)), Ok(true));
        worker.join().expect("worker thread");
    }

    /// 上流と独立に FNV-1a 64 を計算する(指紋の参照値)。
    fn reference_fnv1a(tape: &[u8]) -> u64 {
        tape.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    fn observation_with_tape(tape: Vec<u8>) -> Observation {
        Observation {
            result: Ok(StopReason::Halted),
            pc: 0,
            data_pointer: 0,
            current_cell: 0,
            steps_executed: 0,
            input_position: 0,
            tape: tape.into_boxed_slice(),
            output: Vec::new(),
        }
    }

    #[test]
    fn tape_digest_is_sixteen_lowercase_hex_digits_with_leading_zeros_kept() {
        // 先頭付近の 4 セルの値を総当たりし、先頭が 0 の指紋と先頭が f の指紋を探す。
        // (末尾のセルだけを変えると乗算の拡散が足りず、上位ビットが変わらないため。)
        let mut leading_zero = Vec::new();
        let mut leading_f = Vec::new();
        for (cell, value) in (0..4usize).flat_map(|cell| (0..=255u8).map(move |v| (cell, v))) {
            let mut tape = vec![0u8; TAPE_SIZE];
            tape[cell] = value;
            let reference = reference_fnv1a(&tape);
            if reference >> 60 == 0 && leading_zero.len() < 3 {
                leading_zero.push((tape.clone(), reference));
            }
            if reference >> 60 == 0xf && leading_f.is_empty() {
                leading_f.push((tape.clone(), reference));
            }
        }
        assert_eq!(leading_zero.len(), 3, "leading-zero digests must exist");
        assert_eq!(leading_f.len(), 1, "a leading-f digest must exist");

        for (tape, reference) in leading_zero.into_iter().chain(leading_f) {
            let exit = exit_state(&observation_with_tape(tape), 0).expect("exit state");
            assert_eq!(exit.tape_digest, format!("{reference:016x}"));
            assert_eq!(exit.tape_digest.len(), 16);
        }
    }

    #[test]
    fn micros_converts_durations_to_whole_microseconds() {
        assert_eq!(micros(Duration::from_millis(3)), 3_000);
        assert_eq!(micros(Duration::from_micros(7)), 7);
        assert_eq!(micros(Duration::from_secs(1)), 1_000_000);
        assert_eq!(micros(Duration::from_nanos(1_999)), 1);
        assert_eq!(micros(Duration::MAX), MAX_STEP_LIMIT);
    }
}
