//! ステップ上限の全境界掃引(`tests/common` の差分基盤を用いる)。
//!
//! 素朴実装(core)をオラクルとし、代表プログラム群について上限を変えた全値で最適化機械と比較する。
//!
//! - 停止するプログラムは、上限を 0 から「素朴の必要実行数 + 2」まで 1 ずつ変える。統合操作・最適化ループの途中に当たる上限を含む。
//! - 停止しないプログラムは、小さな打ち切り上限(`NON_HALTING_CAP`)まで掃引する。
//! - 上限 0 の空/非空、ちょうど必要実行数の上限での正常停止、その 1 小さい上限でのステップ制限超過、
//!   上限エラー後の再実行が同じエラーで状態を変えないこと、を個別に確認する。
//!
//! 共通基盤(`tests/common`)は変更しない。

mod common;

use brainfuck_core::{MachineConfig, Program, RuntimeError, StopReason};
use brainfuck_optimized::{OptimizedMachine, OptimizedProgram};
use common::{
    assert_equivalent_across_limits, compare_observations, observe_core, observe_optimized,
    Observation,
};

/// 固定入力。`,` が 0x00 と 0xFF を読み、その後 EOF になる。
const INPUT: &[u8] = &[0x41, 0x00, 0xFF];

/// 必要実行数を求める際の上限、および掃引の探索上限。代表プログラムの必要実行数はこれより小さい。
const PROBE_LIMIT: u64 = 10_000;

/// 停止しないプログラムを掃引する際の打ち切り上限。
const NON_HALTING_CAP: u64 = 64;

/// 正常に停止する代表プログラム(ソース, 入力)。
const HALTING: &[(&str, &[u8])] = &[
    // 空プログラム(上限 0 で停止する)
    ("", &[]),
    // 括弧のみ(セル 0 では本体を実行しない)
    ("[]", &[]),
    // 連続命令の統合
    ("+++++", &[]),
    ("+>+<>--<", &[]),
    // クリア(加算・減算の両方向、先頭セル 255 の長い形を含む)
    ("+++++[-]", &[]),
    ("+++[+]", &[]),
    ("-[-]", &[]),
    // 移動/加算/乗算
    ("+++[->++<]", &[]),
    ("++++++++[>++++++++<-]>+.", &[]),
    // 走査(右向き・左向き、ストライド 1 と複数命令ストライド、反復数 k = 1, 2, 3, 5)
    ("+[>]", &[]),
    ("+>+>+<<[>]", &[]),
    ("+>+<[>]", &[]),
    ("+>+>+>+>+<<<<[>]", &[]),
    ("+>>+>>+<<<<[>>]", &[]),
    ("+>>+<<[>>]", &[]),
    (">+>+>+>+>+[<]", &[]),
    (">>+>+[<]", &[]),
    (">>>>+<<+<<>>>>[<<]", &[]),
    // 一般ループ(本体に `[-]` を含み、最適化されない形)
    ("+[>[-]<-]", &[]),
    // 入出力(EOF で停止)
    (",[.,]", INPUT),
];

/// ポインタ範囲外で誤りとなる代表プログラム(ソース, 入力)。
const FAULTING: &[(&str, &[u8])] = &[
    ("<", &[]),
    ("+++<", &[]),
    ("+[-<]", &[]),
    ("++[>+<-]<", &[]),
];

/// 停止しない代表プログラム(ソース, 入力)。
const NON_HALTING: &[(&str, &[u8])] = &[("+[]", &[]), ("+[>+]", &[])];

fn compile(source: &str) -> OptimizedProgram {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    OptimizedProgram::compile(&program)
}

/// core で完走させ、停止までの必要実行数を返す。停止しないプログラムには使えない。
fn required_steps(source: &str, input: &[u8]) -> u64 {
    let observation = observe_core(source, input, MachineConfig::with_max_steps(PROBE_LIMIT));
    assert_eq!(
        observation.result,
        Ok(StopReason::Halted),
        "{source:?} must halt within the probe limit"
    );
    observation.steps_executed
}

/// 同じ上限で core と最適化機械を実行し、双方の観測結果を返す。
fn run_both(source: &str, input: &[u8], limit: u64) -> (Observation, Observation) {
    let core = observe_core(source, input, MachineConfig::with_max_steps(limit));
    let (optimized, _) = observe_optimized(source, input, MachineConfig::with_max_steps(limit));
    (core, optimized)
}

/// 観測結果がステップ制限超過で、報告された上限が `limit` であることを確認する。
fn assert_step_limit_error(observation: &Observation, limit: u64, label: &str) {
    match observation.result {
        Err(RuntimeError::StepLimitExceeded {
            limit: reported, ..
        }) => assert_eq!(reported, limit, "{label}: reported limit"),
        ref other => panic!("{label}: expected a step limit error, got {other:?}"),
    }
}

/// 停止する代表プログラムを、上限 0 から必要実行数 + 2 まで全値で比較する。
#[test]
fn halting_programs_match_the_core_at_every_limit_up_to_required_plus_two() {
    for &(source, input) in HALTING {
        assert_equivalent_across_limits(source, input, Some(PROBE_LIMIT));
    }
}

/// ポインタ範囲外で誤るプログラムを、上限 0 から探索上限 + 2 まで全値で比較する。
/// 範囲外に至った時点でエラーとなるため、それ以降の上限は同じ結果になる。
#[test]
fn faulting_programs_match_the_core_at_every_limit() {
    for &(source, input) in FAULTING {
        assert_equivalent_across_limits(source, input, Some(PROBE_LIMIT));
    }
}

/// 停止しないプログラムを、上限 0 から打ち切り上限 + 2 まで全値で比較する。
#[test]
fn non_halting_programs_match_the_core_at_every_limit_up_to_the_cap() {
    for &(source, input) in NON_HALTING {
        assert_equivalent_across_limits(source, input, Some(NON_HALTING_CAP));
    }
}

/// 上限がちょうど必要実行数と等しいとき正常停止し、1 小さいとステップ制限超過になることを確認する。
///
/// 統合操作や最適化ループの途中に当たる上限ではなく、停止の境界そのものを検査する。
#[test]
fn limit_equal_to_required_steps_halts_and_one_less_is_a_step_limit_error() {
    for &(source, input) in HALTING {
        let required = required_steps(source, input);

        let (core, optimized) = run_both(source, input, required);
        assert_eq!(
            core.result,
            Ok(StopReason::Halted),
            "core at required limit {required}: {source:?}"
        );
        assert_eq!(
            optimized.result,
            Ok(StopReason::Halted),
            "optimized at required limit {required}: {source:?}"
        );
        assert_eq!(compare_observations(&core, &optimized), "", "{source:?}");

        let Some(limit) = required.checked_sub(1) else {
            continue;
        };
        let (core, optimized) = run_both(source, input, limit);
        assert_step_limit_error(&core, limit, &format!("core {source:?}"));
        assert_step_limit_error(&optimized, limit, &format!("optimized {source:?}"));
        assert_eq!(compare_observations(&core, &optimized), "", "{source:?}");
    }
}

/// 上限 0 では、空プログラムは停止し、非空プログラムは最初の操作の前にステップ制限超過となる。
#[test]
fn zero_limit_halts_the_empty_program_and_fails_a_non_empty_one_before_any_step() {
    let (core, optimized) = run_both("", &[], 0);
    assert_eq!(core.result, Ok(StopReason::Halted), "core: empty program");
    assert_eq!(
        optimized.result,
        Ok(StopReason::Halted),
        "optimized: empty program"
    );
    assert_eq!(compare_observations(&core, &optimized), "", "empty program");

    let (core, optimized) = run_both("+++++", &[], 0);
    let expected = Err(RuntimeError::StepLimitExceeded { limit: 0, pc: 0 });
    assert_eq!(core.result, expected, "core: non-empty program");
    assert_eq!(optimized.result, expected, "optimized: non-empty program");
    assert_eq!(optimized.steps_executed, 0, "no step may be counted");
    assert_eq!(optimized.pc, 0, "pc must stay at the first instruction");
    assert!(
        optimized.tape.iter().all(|&cell| cell == 0),
        "no cell may change"
    );
    assert_eq!(
        compare_observations(&core, &optimized),
        "",
        "non-empty program"
    );
}

/// ステップ制限超過の後で再実行すると、同じエラーを返し、状態を変えない。
///
/// 停止するプログラムは必要実行数 - 1 の上限で、停止しないプログラムは 5 の上限で検査する。
#[test]
fn rerunning_after_a_step_limit_error_returns_the_same_error_without_changing_state() {
    let mut cases: Vec<(&str, &[u8], u64)> = Vec::new();
    for &(source, input) in HALTING {
        if let Some(limit) = required_steps(source, input).checked_sub(1) {
            cases.push((source, input, limit));
        }
    }
    for &(source, input) in NON_HALTING {
        cases.push((source, input, 5));
    }

    for (source, input, limit) in cases {
        let program = compile(source);
        let mut machine = OptimizedMachine::new(
            &program,
            input.to_vec(),
            MachineConfig::with_max_steps(limit),
        );

        let first = machine.run();
        assert!(
            matches!(
                first,
                Err(RuntimeError::StepLimitExceeded { limit: reported, .. }) if reported == limit
            ),
            "{source:?}: expected a step limit error at limit {limit}, got {first:?}"
        );
        let snapshot = machine.snapshot();
        let tape = machine.tape().to_vec();
        let output = machine.output().to_vec();
        let input_position = machine.input_position();

        assert_eq!(
            machine.run(),
            first,
            "{source:?}: rerun must return the same error"
        );
        assert_eq!(
            machine.snapshot(),
            snapshot,
            "{source:?}: rerun must not change the snapshot"
        );
        assert_eq!(
            machine.tape(),
            tape.as_slice(),
            "{source:?}: rerun must not change the tape"
        );
        assert_eq!(
            machine.output(),
            output.as_slice(),
            "{source:?}: rerun must not change the output"
        );
        assert_eq!(
            machine.input_position(),
            input_position,
            "{source:?}: rerun must not change the input position"
        );
    }
}
