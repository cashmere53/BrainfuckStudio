//! 比較の実行: 基準(素朴)・方式選択・繰り返し・判定・総合結果。
//!
//! 素朴は常に先頭で基準として実行し、選択された方式を同一条件で順に実行する。
//! 方式ごとの未対応・失敗は他方式の実行を妨げない。繰り返しは時間予算で打ち切るが、
//! 実行時間は判定・成否には一切用いない(中央値は報告のための計測値にのみ使う)。

use std::time::Duration;

use brainfuck_core::Program;

use crate::limits::{MAX_COMPARISON_STEPS, MAX_REPETITIONS, REPEAT_TIME_BUDGET};
use crate::methods::{run_method, Execution, MethodFailure};
use crate::observation::Observation;
use crate::report::{
    Comparison, FailureKind, Method, MethodReport, Metrics, Outcome, Overall, Verdict,
};

/// 比較の要求。
#[derive(Debug, Clone, Copy)]
pub struct ComparisonRequest<'a> {
    /// 解析済みのプログラム。
    pub program: &'a Program,
    /// 入力バイト列。
    pub input: &'a [u8],
    /// 命令数の上限(素朴換算)。`1..=MAX_COMPARISON_STEPS`。
    pub limit: u64,
    /// 追加で実行する方式。素朴は常に実行され、重複は無視する。
    pub methods: &'a [Method],
    /// 繰り返し回数。`1..=MAX_REPETITIONS`。
    pub repetitions: u32,
}

/// 要求の検証エラー。値は列挙値のみで、ソース・入力・パスを含まない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompareError {
    /// 命令数の上限が範囲外(0 または最大超過)。
    InvalidLimit,
    /// 繰り返し回数が範囲外(0 または最大超過)。
    InvalidRepetitions,
}

/// 比較を実行する。
///
/// # Errors
///
/// 上限または繰り返し回数が範囲外の場合は `CompareError` を返し、どの方式も実行しない。
pub fn compare(req: &ComparisonRequest<'_>) -> Result<Comparison, CompareError> {
    compare_with(run_method, REPEAT_TIME_BUDGET, req)
}

/// 方式の実行と時間予算を差し替えられる内部の入口。
///
/// 公開の `compare` は `run_method` と `REPEAT_TIME_BUDGET` で呼ぶ。
/// 繰り返しの不一致や時間予算での打ち切りをテストするために、この入口を使う。
fn compare_with<F>(
    mut runner: F,
    budget: Duration,
    req: &ComparisonRequest<'_>,
) -> Result<Comparison, CompareError>
where
    F: FnMut(Method, &Program, &[u8], u64) -> Result<Execution, MethodFailure>,
{
    validate(req)?;

    // 素朴を先頭に、選択された方式を固定の順序で並べる(重複と素朴の指定は無視する)。
    let order: Vec<Method> = [Method::Naive, Method::Optimized, Method::Wasm]
        .into_iter()
        .filter(|method| *method == Method::Naive || req.methods.contains(method))
        .collect();

    let (oracle, oracle_metrics) = match measure(&mut runner, Method::Naive, req, budget) {
        Ok(measured) => measured,
        Err(failure) => return Ok(oracle_unavailable(req.limit, &order, failure)),
    };

    let mut reports = Vec::with_capacity(order.len());
    for method in order.iter().copied().skip(1) {
        let outcome = match measure(&mut runner, method, req, budget) {
            Ok((observation, metrics)) => Outcome::Completed {
                verdict: verdict_of(&oracle, &observation),
                observation,
                metrics,
            },
            Err(failure) => outcome_of_failure(failure),
        };
        reports.push(MethodReport { method, outcome });
    }

    // 基準の観測は判定の後に先頭へ置く。
    reports.insert(
        0,
        MethodReport {
            method: Method::Naive,
            outcome: Outcome::Completed {
                observation: oracle,
                verdict: Verdict::Oracle,
                metrics: oracle_metrics,
            },
        },
    );
    let overall = overall_of(&reports);
    Ok(Comparison {
        limit: req.limit,
        reports,
        overall,
    })
}

/// 要求の範囲を検査する。方式は実行しない。
fn validate(req: &ComparisonRequest<'_>) -> Result<(), CompareError> {
    if req.limit == 0 || req.limit > MAX_COMPARISON_STEPS {
        return Err(CompareError::InvalidLimit);
    }
    if req.repetitions == 0 || req.repetitions > MAX_REPETITIONS {
        return Err(CompareError::InvalidRepetitions);
    }
    Ok(())
}

/// 方式を繰り返し実行する。打ち切りは「繰り返し回数に達した」か「実行時間の合計が予算に達した」のいずれか。
///
/// 最低 1 回は実行する。各回の観測が最初の回と異なれば `InconsistentRepetitions` の失敗とする。
/// 計測値は実行時間の中央値(偶数回は下側)の回のものを採り、回数は実際の回数を記録する。
fn measure<F>(
    runner: &mut F,
    method: Method,
    req: &ComparisonRequest<'_>,
    budget: Duration,
) -> Result<(Observation, Metrics), MethodFailure>
where
    F: FnMut(Method, &Program, &[u8], u64) -> Result<Execution, MethodFailure>,
{
    let mut runs: Vec<Execution> = Vec::new();
    let mut spent = Duration::ZERO;
    let mut count: u32 = 0;
    loop {
        let execution = runner(method, req.program, req.input, req.limit)?;
        spent = spent.saturating_add(execution.metrics.run);
        count += 1;
        runs.push(execution);
        if count >= req.repetitions || spent >= budget {
            break;
        }
    }

    let mut metrics = runs[median_index(&runs)].metrics.clone();
    metrics.repetitions = count;

    let observation = runs.swap_remove(0).observation;
    if runs.iter().any(|run| run.observation != observation) {
        return Err(MethodFailure::Failed(FailureKind::InconsistentRepetitions));
    }
    Ok((observation, metrics))
}

/// 実行時間の中央値の添字(偶数回は下側)。`runs` は空でないこと。
fn median_index(runs: &[Execution]) -> usize {
    let mut order: Vec<usize> = (0..runs.len()).collect();
    order.sort_by_key(|&index| runs[index].metrics.run);
    order[(runs.len() - 1) / 2]
}

/// 基準との判定。差分が空なら一致、そうでなければ差分の項目を持つ不一致。
fn verdict_of(oracle: &Observation, observed: &Observation) -> Verdict {
    let fields = oracle.diff(observed);
    if fields.is_empty() {
        Verdict::Match
    } else {
        Verdict::Mismatch(fields)
    }
}

fn outcome_of_failure(failure: MethodFailure) -> Outcome {
    match failure {
        MethodFailure::Unsupported(reason) => Outcome::Unsupported(reason),
        MethodFailure::Failed(kind) => Outcome::Failed(kind),
    }
}

/// 基準が得られなかった場合の報告。素朴は対象外・失敗を返さないため通常は到達しない。
///
/// 基準がなければ他方式を判定できないため、他方式はエンジンの失敗として報告し、総合結果は未完了にする。
fn oracle_unavailable(limit: u64, order: &[Method], failure: MethodFailure) -> Comparison {
    let mut reports = Vec::with_capacity(order.len());
    for (index, method) in order.iter().copied().enumerate() {
        let outcome = if index == 0 {
            outcome_of_failure(failure)
        } else {
            Outcome::Failed(FailureKind::EngineError)
        };
        reports.push(MethodReport { method, outcome });
    }
    Comparison {
        limit,
        reports,
        overall: Overall::Incomplete,
    }
}

/// 総合結果。不一致が 1 つでもあれば不一致、不一致がなく未対応または失敗があれば未完了、どちらもなければ全一致。
fn overall_of(reports: &[MethodReport]) -> Overall {
    let mut mismatch = false;
    let mut incomplete = false;
    for report in reports {
        match &report.outcome {
            Outcome::Completed {
                verdict: Verdict::Mismatch(_),
                ..
            } => mismatch = true,
            Outcome::Completed { .. } => {}
            Outcome::Unsupported(_) | Outcome::Failed(_) => incomplete = true,
        }
    }
    if mismatch {
        Overall::Mismatch
    } else if incomplete {
        Overall::Incomplete
    } else {
        Overall::AllMatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methods::run_naive;
    use crate::observation::Field;
    use crate::report::{Units, UnsupportedReason};
    use brainfuck_core::StopReason;
    use brainfuck_wasm::limits::MAX_OUTPUT_BYTES;

    /// 比較の最大ステップは出力量の上限以下(生成物の出力量で必ず収まる)。
    const _: () = assert!(MAX_COMPARISON_STEPS <= MAX_OUTPUT_BYTES as u64);

    fn parse(source: &str) -> Program {
        Program::parse(source).expect("テストのソースは括弧が整合している")
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    /// 核の完走観測(空プログラム)。テストの基準の雛形。
    fn base_observation() -> Observation {
        run_naive(&parse(""), &[], 1)
            .expect("素朴は完走する")
            .observation
    }

    fn fake(observation: Observation, run: Duration) -> Execution {
        Execution {
            observation,
            metrics: Metrics {
                prepare: None,
                run,
                repetitions: 1,
                units: Units::None,
                artifact_bytes: None,
                replayed_steps: None,
            },
        }
    }

    fn request<'a>(
        program: &'a Program,
        methods: &'a [Method],
        repetitions: u32,
    ) -> ComparisonRequest<'a> {
        ComparisonRequest {
            program,
            input: &[],
            limit: 1_000,
            methods,
            repetitions,
        }
    }

    fn outcome_for(comparison: &Comparison, method: Method) -> &Outcome {
        &comparison
            .reports
            .iter()
            .find(|report| report.method == method)
            .expect("要求した方式の報告がある")
            .outcome
    }

    fn verdict_for(comparison: &Comparison, method: Method) -> Verdict {
        match outcome_for(comparison, method) {
            Outcome::Completed { verdict, .. } => verdict.clone(),
            other => panic!("完走しているはず: {other:?}"),
        }
    }

    fn metrics_for(comparison: &Comparison, method: Method) -> &Metrics {
        match outcome_for(comparison, method) {
            Outcome::Completed { metrics, .. } => metrics,
            other => panic!("完走しているはず: {other:?}"),
        }
    }

    /// すべての方式が同じ観測を返す実行器。
    fn constant(
        observation: Observation,
    ) -> impl FnMut(Method, &Program, &[u8], u64) -> Result<Execution, MethodFailure> {
        move |_, _, _, _| Ok(fake(observation.clone(), ms(1)))
    }

    /// 時間を除き、同一要求で同一の結果になるかを比べるための形。
    fn strip_timing(mut comparison: Comparison) -> Comparison {
        for report in &mut comparison.reports {
            if let Outcome::Completed { metrics, .. } = &mut report.outcome {
                metrics.prepare = metrics.prepare.map(|_| Duration::ZERO);
                metrics.run = Duration::ZERO;
            }
        }
        comparison
    }

    #[test]
    fn limit_bounds_are_checked_before_any_method_runs() {
        let program = parse("+");
        let base = base_observation();
        let cases = [
            (0, Err(CompareError::InvalidLimit)),
            (1, Ok(())),
            (MAX_COMPARISON_STEPS, Ok(())),
            (MAX_COMPARISON_STEPS + 1, Err(CompareError::InvalidLimit)),
        ];
        for (limit, expected) in cases {
            let mut calls = 0u32;
            let req = ComparisonRequest {
                limit,
                ..request(&program, &[Method::Wasm], 3)
            };
            let result = compare_with(
                |_, _, _, _| {
                    calls += 1;
                    Ok(fake(base.clone(), ms(1)))
                },
                REPEAT_TIME_BUDGET,
                &req,
            )
            .map(|_| ());
            assert_eq!(result, expected, "上限 {limit}");
            // 素朴と Wasm の 2 方式を、それぞれ 3 回ずつ実行する。
            let expected_calls = if expected.is_ok() { 2 * 3 } else { 0 };
            assert_eq!(calls, expected_calls, "上限 {limit} で方式が実行された回数");
        }
    }

    #[test]
    fn repetition_bounds_are_checked_before_any_method_runs() {
        let program = parse("+");
        let base = base_observation();
        let cases = [
            (0, Err(CompareError::InvalidRepetitions)),
            (1, Ok(())),
            (MAX_REPETITIONS, Ok(())),
            (MAX_REPETITIONS + 1, Err(CompareError::InvalidRepetitions)),
        ];
        for (repetitions, expected) in cases {
            let mut calls = 0u32;
            let result = compare_with(
                |_, _, _, _| {
                    calls += 1;
                    Ok(fake(base.clone(), ms(1)))
                },
                REPEAT_TIME_BUDGET,
                &request(&program, &[], repetitions),
            )
            .map(|_| ());
            assert_eq!(result, expected, "繰り返し {repetitions}");
            // 素朴のみを、指定の回数だけ実行する。
            let expected_calls = if expected.is_ok() { repetitions } else { 0 };
            assert_eq!(
                calls, expected_calls,
                "繰り返し {repetitions} で実行された回数"
            );
        }
    }

    #[test]
    fn naive_is_always_first_as_oracle_and_duplicates_are_ignored() {
        let program = parse("+");
        let base = base_observation();

        let none = compare_with(
            constant(base.clone()),
            REPEAT_TIME_BUDGET,
            &request(&program, &[], 1),
        )
        .expect("有効な要求");
        assert_eq!(none.reports.len(), 1);
        assert_eq!(none.reports[0].method, Method::Naive);
        assert_eq!(verdict_for(&none, Method::Naive), Verdict::Oracle);

        let mut called = Vec::new();
        let requested = [Method::Wasm, Method::Optimized, Method::Naive, Method::Wasm];
        let all = compare_with(
            |method, _, _, _| {
                called.push(method);
                Ok(fake(base.clone(), ms(1)))
            },
            REPEAT_TIME_BUDGET,
            &request(&program, &requested, 1),
        )
        .expect("有効な要求");
        assert_eq!(
            all.reports.iter().map(|r| r.method).collect::<Vec<_>>(),
            vec![Method::Naive, Method::Optimized, Method::Wasm]
        );
        assert_eq!(verdict_for(&all, Method::Naive), Verdict::Oracle);
        assert_eq!(verdict_for(&all, Method::Optimized), Verdict::Match);
        assert_eq!(verdict_for(&all, Method::Wasm), Verdict::Match);
        assert_eq!(
            called,
            vec![Method::Naive, Method::Optimized, Method::Wasm],
            "素朴は 1 回だけ、重複する方式は実行しない"
        );
    }

    #[test]
    fn mismatch_reports_the_differing_fields_and_overall_is_mismatch() {
        let program = parse("+");
        let base = base_observation();
        let mut wrong = base.clone();
        wrong.pc = 7;
        wrong.current_cell = 3;
        let comparison = compare_with(
            |method, _, _, _| {
                let observation = if method == Method::Wasm {
                    wrong.clone()
                } else {
                    base.clone()
                };
                Ok(fake(observation, ms(1)))
            },
            REPEAT_TIME_BUDGET,
            &request(&program, &[Method::Optimized, Method::Wasm], 1),
        )
        .expect("有効な要求");
        assert_eq!(verdict_for(&comparison, Method::Optimized), Verdict::Match);
        assert_eq!(
            verdict_for(&comparison, Method::Wasm),
            Verdict::Mismatch(vec![Field::Pc, Field::CurrentCell])
        );
        assert_eq!(comparison.overall, Overall::Mismatch);
    }

    #[test]
    fn all_matching_completed_methods_give_all_match() {
        let program = parse("+");
        let comparison = compare_with(
            constant(base_observation()),
            REPEAT_TIME_BUDGET,
            &request(&program, &[Method::Optimized, Method::Wasm], 1),
        )
        .expect("有効な要求");
        assert_eq!(comparison.overall, Overall::AllMatch);
        assert_eq!(comparison.limit, 1_000);
    }

    #[test]
    fn unsupported_or_failed_without_mismatch_is_incomplete_and_does_not_block_others() {
        let program = parse("+");
        let base = base_observation();
        let comparison = compare_with(
            |method, _, _, _| match method {
                Method::Optimized => Err(MethodFailure::Unsupported(
                    UnsupportedReason::NestingTooDeep,
                )),
                Method::Wasm => Err(MethodFailure::Failed(FailureKind::WatchdogTripped)),
                Method::Naive => Ok(fake(base.clone(), ms(1))),
            },
            REPEAT_TIME_BUDGET,
            &request(&program, &[Method::Optimized, Method::Wasm], 1),
        )
        .expect("有効な要求");
        assert_eq!(
            outcome_for(&comparison, Method::Optimized),
            &Outcome::Unsupported(UnsupportedReason::NestingTooDeep)
        );
        assert_eq!(
            outcome_for(&comparison, Method::Wasm),
            &Outcome::Failed(FailureKind::WatchdogTripped)
        );
        assert_eq!(verdict_for(&comparison, Method::Naive), Verdict::Oracle);
        assert_eq!(comparison.overall, Overall::Incomplete);
    }

    #[test]
    fn mismatch_takes_precedence_over_incomplete() {
        let program = parse("+");
        let base = base_observation();
        let mut wrong = base.clone();
        wrong.output = vec![1];
        let comparison = compare_with(
            |method, _, _, _| match method {
                Method::Optimized => Err(MethodFailure::Unsupported(
                    UnsupportedReason::ModuleTooLarge,
                )),
                Method::Wasm => Ok(fake(wrong.clone(), ms(1))),
                Method::Naive => Ok(fake(base.clone(), ms(1))),
            },
            REPEAT_TIME_BUDGET,
            &request(&program, &[Method::Optimized, Method::Wasm], 1),
        )
        .expect("有効な要求");
        assert_eq!(
            verdict_for(&comparison, Method::Wasm),
            Verdict::Mismatch(vec![Field::Output])
        );
        assert_eq!(comparison.overall, Overall::Mismatch);
    }

    #[test]
    fn repeated_runs_report_the_lower_median_and_the_actual_count() {
        let program = parse("+");
        let base = base_observation();
        // 偶数回(4 回)の下側の中央値: 10, 20, 30, 40 → 20
        let durations = [40u64, 10, 30, 20];
        let mut index = 0;
        let even = compare_with(
            |_, _, _, _| {
                let run = ms(durations[index]);
                index += 1;
                Ok(fake(base.clone(), run))
            },
            REPEAT_TIME_BUDGET,
            &request(&program, &[], 4),
        )
        .expect("有効な要求");
        assert_eq!(metrics_for(&even, Method::Naive).run, ms(20));
        assert_eq!(metrics_for(&even, Method::Naive).repetitions, 4);

        // 奇数回(3 回)の中央値: 10, 30, 40 → 30
        let durations = [30u64, 10, 40];
        let mut index = 0;
        let odd = compare_with(
            |_, _, _, _| {
                let run = ms(durations[index]);
                index += 1;
                Ok(fake(base.clone(), run))
            },
            REPEAT_TIME_BUDGET,
            &request(&program, &[], 3),
        )
        .expect("有効な要求");
        assert_eq!(metrics_for(&odd, Method::Naive).run, ms(30));
        assert_eq!(metrics_for(&odd, Method::Naive).repetitions, 3);
    }

    #[test]
    fn repetitions_stop_at_the_requested_count_when_the_budget_is_large() {
        let program = parse("+");
        let base = base_observation();
        let comparison = compare_with(
            |_, _, _, _| Ok(fake(base.clone(), ms(1))),
            Duration::from_secs(3600),
            &request(&program, &[Method::Wasm], MAX_REPETITIONS),
        )
        .expect("有効な要求");
        assert_eq!(
            metrics_for(&comparison, Method::Naive).repetitions,
            MAX_REPETITIONS
        );
        assert_eq!(
            metrics_for(&comparison, Method::Wasm).repetitions,
            MAX_REPETITIONS
        );
    }

    #[test]
    fn repetitions_stop_when_the_time_budget_is_reached() {
        let program = parse("+");
        let base = base_observation();
        // 各回 20ms。予算 50ms: 20 → 40 → 60(≥50 で打ち切り)の 3 回。
        let over = compare_with(
            |_, _, _, _| Ok(fake(base.clone(), ms(20))),
            ms(50),
            &request(&program, &[], MAX_REPETITIONS),
        )
        .expect("有効な要求");
        assert_eq!(metrics_for(&over, Method::Naive).repetitions, 3);

        // 予算ちょうど(40ms)に達した時点で打ち切る(2 回)。
        let exact = compare_with(
            |_, _, _, _| Ok(fake(base.clone(), ms(20))),
            ms(40),
            &request(&program, &[], MAX_REPETITIONS),
        )
        .expect("有効な要求");
        assert_eq!(metrics_for(&exact, Method::Naive).repetitions, 2);

        // 予算 0 でも最低 1 回は実行する。
        let zero = compare_with(
            |_, _, _, _| Ok(fake(base.clone(), ms(20))),
            Duration::ZERO,
            &request(&program, &[], MAX_REPETITIONS),
        )
        .expect("有効な要求");
        assert_eq!(metrics_for(&zero, Method::Naive).repetitions, 1);
    }

    #[test]
    fn a_repetition_that_observes_differently_is_an_inconsistent_repetitions_failure() {
        let program = parse("+");
        let base = base_observation();
        let mut changed = base.clone();
        changed.pc = 9;
        let mut wasm_calls = 0u32;
        let comparison = compare_with(
            |method, _, _, _| match method {
                Method::Wasm => {
                    wasm_calls += 1;
                    let observation = if wasm_calls == 2 {
                        changed.clone()
                    } else {
                        base.clone()
                    };
                    Ok(fake(observation, ms(1)))
                }
                _ => Ok(fake(base.clone(), ms(1))),
            },
            REPEAT_TIME_BUDGET,
            &request(&program, &[Method::Wasm], 3),
        )
        .expect("有効な要求");
        assert_eq!(
            outcome_for(&comparison, Method::Wasm),
            &Outcome::Failed(FailureKind::InconsistentRepetitions)
        );
        assert_eq!(verdict_for(&comparison, Method::Naive), Verdict::Oracle);
        assert_eq!(comparison.overall, Overall::Incomplete);
    }

    #[test]
    fn a_failing_oracle_does_not_panic_and_marks_the_others_as_engine_failures() {
        let program = parse("+");
        let comparison = compare_with(
            |_, _, _, _| Err(MethodFailure::Failed(FailureKind::EngineError)),
            REPEAT_TIME_BUDGET,
            &request(&program, &[Method::Optimized, Method::Wasm], 1),
        )
        .expect("有効な要求");
        assert_eq!(comparison.reports.len(), 3);
        assert_eq!(
            outcome_for(&comparison, Method::Naive),
            &Outcome::Failed(FailureKind::EngineError)
        );
        assert_eq!(
            outcome_for(&comparison, Method::Wasm),
            &Outcome::Failed(FailureKind::EngineError)
        );
        assert_eq!(comparison.overall, Overall::Incomplete);
    }

    #[test]
    fn execution_time_never_decides_the_verdict() {
        let program = parse("+");
        let base = base_observation();
        // 方式の実行時間だけが大きく異なり、観測は同じ。判定・総合結果は一致のまま。
        let comparison = compare_with(
            |method, _, _, _| {
                let run = if method == Method::Wasm {
                    ms(1_000_000)
                } else {
                    ms(1)
                };
                Ok(fake(base.clone(), run))
            },
            REPEAT_TIME_BUDGET,
            &request(&program, &[Method::Wasm], 3),
        )
        .expect("有効な要求");
        assert_eq!(verdict_for(&comparison, Method::Wasm), Verdict::Match);
        assert_eq!(comparison.overall, Overall::AllMatch);
        // 予算を超えた方式は繰り返しを打ち切られるが、それも判定には影響しない。
        assert_eq!(metrics_for(&comparison, Method::Wasm).repetitions, 1);
    }

    #[test]
    fn real_comparison_matches_core_and_reports_units_per_method() {
        let program = parse("+++.");
        let comparison = compare(&ComparisonRequest {
            input: b"",
            ..request(&program, &[Method::Optimized, Method::Wasm], 3)
        })
        .expect("有効な要求");
        assert_eq!(comparison.overall, Overall::AllMatch);
        assert_eq!(verdict_for(&comparison, Method::Optimized), Verdict::Match);
        assert_eq!(verdict_for(&comparison, Method::Wasm), Verdict::Match);

        match outcome_for(&comparison, Method::Naive) {
            Outcome::Completed {
                observation,
                verdict,
                metrics,
            } => {
                assert_eq!(*verdict, Verdict::Oracle);
                assert_eq!(observation.output, vec![3]);
                assert_eq!(observation.result, Ok(StopReason::Halted));
                assert_eq!(metrics.prepare, None);
                assert_eq!(metrics.repetitions, 3);
                assert_eq!(
                    metrics.units,
                    Units::NaiveInstructions(observation.steps_executed)
                );
            }
            other => panic!("素朴は完走するはず: {other:?}"),
        }
        assert_eq!(metrics_for(&comparison, Method::Wasm).units, Units::None);
        assert_eq!(
            metrics_for(&comparison, Method::Wasm).replayed_steps,
            Some(0)
        );
    }

    #[test]
    fn real_comparison_with_nesting_beyond_the_wasm_limit_is_unsupported_only_for_wasm() {
        let deep = format!("{}{}", "[".repeat(257), "]".repeat(257));
        let program = parse(&deep);
        let comparison =
            compare(&request(&program, &[Method::Optimized, Method::Wasm], 1)).expect("有効な要求");
        assert_eq!(
            outcome_for(&comparison, Method::Wasm),
            &Outcome::Unsupported(UnsupportedReason::NestingTooDeep)
        );
        assert_eq!(verdict_for(&comparison, Method::Optimized), Verdict::Match);
        assert_eq!(comparison.overall, Overall::Incomplete);
    }

    #[test]
    fn real_comparison_is_deterministic_apart_from_time() {
        let program = parse("++[>+++<-]>.");
        let methods = [Method::Optimized, Method::Wasm];
        let first = compare(&request(&program, &methods, 2)).expect("有効な要求");
        let second = compare(&request(&program, &methods, 2)).expect("有効な要求");
        assert_eq!(strip_timing(first), strip_timing(second));
    }

    #[test]
    fn real_comparison_rejects_invalid_requests_without_running_methods() {
        let program = parse("+");
        assert_eq!(
            compare(&ComparisonRequest {
                limit: 0,
                ..request(&program, &[Method::Wasm], 1)
            })
            .map(|_| ()),
            Err(CompareError::InvalidLimit)
        );
        assert_eq!(
            compare(&request(&program, &[Method::Wasm], 0)).map(|_| ()),
            Err(CompareError::InvalidRepetitions)
        );
    }

    #[test]
    fn step_limit_error_is_the_same_observation_in_every_method() {
        let program = parse("+[]");
        let comparison = compare(&ComparisonRequest {
            limit: 50,
            ..request(&program, &[Method::Optimized, Method::Wasm], 1)
        })
        .expect("有効な要求");
        match outcome_for(&comparison, Method::Naive) {
            Outcome::Completed { observation, .. } => assert!(observation.result.is_err()),
            other => panic!("素朴は観測を返すはず: {other:?}"),
        }
        assert_eq!(comparison.overall, Overall::AllMatch);
    }
}
