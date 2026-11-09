//! スモーク(13.1, 13.9): 手書き数本で最適化・WASM が素朴と一致すること、
//! および一致判定が観測の項目ごとの破損を不一致として検出すること。
//!
//! 最適化方式の差分テストはこのスモークに限る(網羅的な差分は `optimized-interpreter` が持つ)。

mod common;

use brainfuck_compare::limits::MAX_COMPARISON_STEPS;
use brainfuck_compare::methods::{run_method, run_naive};
use brainfuck_compare::{Field, Method, Observation, Units};
use brainfuck_core::{PointerDirection, Program, RuntimeError, StopReason};
use brainfuck_optimized::OptimizedProgram;
use common::{hello_source, needed_steps, pair_diff, run_pair, sweep_limits};

/// 手書きの停止するプログラム(ソース, 入力)。
const SAMPLES: &[(&str, &[u8])] = &[
    ("", b""),
    ("+++.", b""),
    (",[.,]", b"\x00\xff"),
    ("++[>+++<-]>.", b""),
    ("+++>++<[>+<-]>.", b""),
    ("+[-]+++.", b""),
    (",>,<[->+<]>.", b"A\x07"),
    ("+>>>>+[<]", b""),
];

#[test]
fn hand_written_programs_match_naive_for_optimized_and_wasm() {
    for &(source, input) in SAMPLES {
        for method in [Method::Optimized, Method::Wasm] {
            run_pair(source, input, MAX_COMPARISON_STEPS, method);
            let needed = needed_steps(source, input);
            for limit in [0, 1, needed.saturating_sub(1), needed, needed + 1] {
                run_pair(source, input, limit, method);
            }
        }
    }
}

#[test]
fn hand_written_programs_sweep_every_limit_for_optimized_and_wasm() {
    for &(source, input) in SAMPLES {
        for method in [Method::Optimized, Method::Wasm] {
            sweep_limits(source, input, method);
        }
    }
}

#[test]
fn hello_sample_matches_naive_for_optimized_and_wasm() {
    let source = hello_source();
    for method in [Method::Optimized, Method::Wasm] {
        run_pair(source, b"", MAX_COMPARISON_STEPS, method);
        let needed = needed_steps(source, b"");
        for limit in [0, needed / 2, needed - 1, needed] {
            run_pair(source, b"", limit, method);
        }
    }
}

#[test]
fn optimized_units_count_ir_operations_as_hand_counted() {
    // 最適化後の IR 操作列は Add(3), Move(1), Add(2) の 3 操作(OptimizedProgram::ops を手で数えた値)。
    // 素朴換算の実行数は 6 命令。単位は IR 操作回数で、素朴換算とは別の値になる。
    let source = "+++>++";
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    let ir = OptimizedProgram::compile(&program);
    assert_eq!(ir.ops().len(), 3, "IR 操作列の手数");

    let execution = run_method(Method::Optimized, &program, b"", MAX_COMPARISON_STEPS)
        .expect("最適化は失敗しない");
    assert_eq!(execution.metrics.units, Units::IrOperations(3));
    assert_eq!(execution.observation.steps_executed, 6);
    assert_eq!(
        execution.observation,
        run_naive(&program, b"", MAX_COMPARISON_STEPS)
            .unwrap()
            .observation
    );
}

#[test]
fn wasm_replays_the_bailed_out_prefix_on_the_host() {
    // 上限 1 では融合された Add(3) が予算を超えて離脱し、素朴の 1 命令をホストが再生する。
    // 再生した命令数は素朴実装が上限までに実行した命令数(1)と等しい。
    let source = "+++>++";
    let execution = run_pair(source, b"", 1, Method::Wasm);
    assert_eq!(execution.metrics.replayed_steps, Some(1));
    assert_eq!(execution.observation.steps_executed, 1);

    // 離脱なしの完走では再生は 0 命令。
    let full = run_pair(source, b"", MAX_COMPARISON_STEPS, Method::Wasm);
    assert_eq!(full.metrics.replayed_steps, Some(0));
}

/// 完全観測値の基準になる実行(入力 1 バイトを読み、出力する)。
fn base_observation() -> Observation {
    let program = Program::parse(",.+").expect("テスト用ソースは解析できる");
    run_naive(&program, b"\x05", MAX_COMPARISON_STEPS)
        .expect("素朴は失敗しない")
        .observation
}

/// 項目ごとに 1 つだけ壊した観測を作り、差分がその項目だけを返すことを確かめる。
#[test]
fn breaking_one_observation_field_is_detected_as_exactly_that_mismatch() {
    let base = base_observation();
    assert!(base.diff(&base).is_empty(), "同一観測の差分は空");

    let mut cases: Vec<(Field, Observation)> = Vec::new();

    let mut termination = base.clone();
    termination.result = Err(RuntimeError::PointerOutOfBounds {
        pc: 0,
        dp: 0,
        direction: PointerDirection::Left,
    });
    cases.push((Field::Termination, termination));

    let mut pc = base.clone();
    pc.pc += 1;
    cases.push((Field::Pc, pc));

    let mut dp = base.clone();
    dp.data_pointer += 1;
    cases.push((Field::DataPointer, dp));

    let mut cell = base.clone();
    cell.current_cell = cell.current_cell.wrapping_add(1);
    cases.push((Field::CurrentCell, cell));

    let mut steps = base.clone();
    steps.steps_executed += 1;
    cases.push((Field::StepsExecuted, steps));

    let mut input = base.clone();
    input.input_position += 1;
    cases.push((Field::InputPosition, input));

    let mut output = base.clone();
    output.output.push(b'!');
    cases.push((Field::Output, output));

    let mut tape = base.clone();
    tape.tape[0] = tape.tape[0].wrapping_add(1);
    cases.push((Field::Tape, tape));

    for (field, broken) in cases {
        assert_eq!(broken.diff(&base), vec![field], "項目 {field:?} の破損");
        assert_eq!(
            base.diff(&broken),
            vec![field],
            "項目 {field:?} の破損(逆順)"
        );
    }
}

/// エラー値だけが異なる場合は `ErrorValue` を返し、終了種別の不一致とは区別する。
#[test]
fn error_value_only_difference_is_reported_as_error_value() {
    let mut base = base_observation();
    base.result = Err(RuntimeError::PointerOutOfBounds {
        pc: 1,
        dp: 0,
        direction: PointerDirection::Left,
    });
    let mut broken = base.clone();
    broken.result = Err(RuntimeError::PointerOutOfBounds {
        pc: 2,
        dp: 0,
        direction: PointerDirection::Left,
    });
    assert_eq!(broken.diff(&base), vec![Field::ErrorValue]);

    let mut halted = base.clone();
    halted.result = Ok(StopReason::Halted);
    assert_eq!(halted.diff(&base), vec![Field::Termination]);
}

/// 実際の方式の観測を壊しても、同じ項目だけが不一致として検出される。
#[test]
fn a_real_method_observation_broken_per_field_is_detected() {
    let source = ",.+";
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    let naive = run_naive(&program, b"\x05", MAX_COMPARISON_STEPS)
        .expect("素朴は失敗しない")
        .observation;
    let wasm = run_method(Method::Wasm, &program, b"\x05", MAX_COMPARISON_STEPS)
        .expect("WASM は完走する")
        .observation;
    assert!(pair_diff(source, b"\x05", MAX_COMPARISON_STEPS, Method::Wasm).is_empty());
    assert!(wasm.diff(&naive).is_empty());

    let mut broken = wasm.clone();
    broken.output = b"X".to_vec();
    assert_eq!(broken.diff(&naive), vec![Field::Output]);

    let mut broken = wasm.clone();
    broken.steps_executed += 1;
    assert_eq!(broken.diff(&naive), vec![Field::StepsExecuted]);
}
