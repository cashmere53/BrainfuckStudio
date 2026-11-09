//! 共通基盤(`tests/common`)の自己テスト。
//!
//! 差分テストの検出力を支えるヘルパ自体が正しいことを、壊した観測の注入・件数の既知値・
//! 固定の期待列で固定する。ヘルパが比較を落とす・範囲がずれる・列挙が欠ける・乱数が変わる・
//! サンプルが差し替わる、のいずれでもどれかのテストが失敗する。

mod common;

use std::any::Any;
use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use brainfuck_compare::methods::{run_naive, Execution, MethodFailure};
use brainfuck_compare::{FailureKind, Field, Method, Observation};
use brainfuck_core::{PointerDirection, Program, RuntimeError, StopReason};
use common::{
    describe, enumerate_balanced, hello_source, judge, needed_steps, pair_diff, pair_diff_with,
    real_runner, run_pair, run_pair_with, sweep_limits, sweep_range, XorShift64,
};

const ALL_FIELDS: [Field; 9] = [
    Field::Termination,
    Field::ErrorValue,
    Field::Pc,
    Field::DataPointer,
    Field::CurrentCell,
    Field::StepsExecuted,
    Field::InputPosition,
    Field::Output,
    Field::Tape,
];

/// 項目ごとの基準のプログラムと入力。`ErrorValue` はエラーで停止する必要がある。
fn base_for(field: Field) -> (&'static str, &'static [u8]) {
    match field {
        Field::ErrorValue => ("<", b""),
        _ => (",.+", b"\x05"),
    }
}

/// 項目を 1 つだけ壊す(他の項目は変えない)。
fn mutate(observation: &mut Observation, field: Field) {
    match field {
        Field::Termination => {
            observation.result = match observation.result {
                Ok(_) => Err(RuntimeError::PointerOutOfBounds {
                    pc: 0,
                    dp: 0,
                    direction: PointerDirection::Left,
                }),
                Err(_) => Ok(StopReason::Halted),
            };
        }
        Field::ErrorValue => {
            observation.result = match observation.result {
                Err(RuntimeError::PointerOutOfBounds { pc, dp, direction }) => {
                    Err(RuntimeError::PointerOutOfBounds {
                        pc: pc + 1,
                        dp,
                        direction,
                    })
                }
                other => panic!("ErrorValue の基準はエラーで停止する必要がある: {other:?}"),
            };
        }
        Field::Pc => observation.pc += 1,
        Field::DataPointer => observation.data_pointer += 1,
        Field::CurrentCell => observation.current_cell = observation.current_cell.wrapping_add(1),
        Field::StepsExecuted => observation.steps_executed += 1,
        Field::InputPosition => observation.input_position += 1,
        Field::Output => observation.output.push(b'!'),
        Field::Tape => observation.tape[0] = observation.tape[0].wrapping_add(1),
        other => panic!("この自己テストが扱わない項目: {other:?}"),
    }
}

/// 実際の方式の実行結果のうち、`field` の項目だけを壊す `run`。
fn breaking(
    field: Field,
) -> impl Fn(&Program, &[u8], u64, Method) -> Result<Execution, MethodFailure> {
    move |program, input, limit, method| {
        let mut execution = real_runner(program, input, limit, method)?;
        mutate(&mut execution.observation, field);
        Ok(execution)
    }
}

fn panic_text(result: Result<Execution, Box<dyn Any + Send>>) -> String {
    match result {
        Ok(_) => panic!("panic が期待されたが完走した"),
        Err(payload) => payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .expect("文字列の panic メッセージ"),
    }
}

// --- 比較の注入と検出 ---------------------------------------------------------

#[test]
fn each_field_broken_alone_is_reported_as_exactly_that_field() {
    for field in ALL_FIELDS {
        let (source, input) = base_for(field);
        for method in [Method::Optimized, Method::Wasm] {
            let diff = pair_diff_with(source, input, 100, method, breaking(field));
            assert_eq!(
                diff,
                vec![field],
                "{method:?} で項目 {field:?} だけを壊した差分"
            );
        }
    }
}

#[test]
fn an_unbroken_method_has_no_diff_and_run_pair_returns_its_execution() {
    for field in ALL_FIELDS {
        let (source, input) = base_for(field);
        for method in [Method::Optimized, Method::Wasm] {
            assert!(pair_diff(source, input, 100, method).is_empty());
            let execution = run_pair(source, input, 100, method);
            assert_eq!(execution.metrics.repetitions, 1);
        }
    }
    let execution = run_pair(",.+", b"\x05", 100, Method::Wasm);
    assert!(execution.observation.halted());
}

#[test]
fn judge_reports_the_observation_difference_against_the_oracle() {
    let program = Program::parse(",.+").expect("テスト用ソースは解析できる");
    let oracle = run_naive(&program, b"\x05", 100)
        .expect("素朴は失敗しない")
        .observation;
    assert!(judge(&oracle, &oracle).is_empty());
    let mut broken = oracle.clone();
    broken.output.push(b'!');
    assert_eq!(judge(&oracle, &broken), vec![Field::Output]);
}

#[test]
fn run_pair_panic_names_source_input_limit_and_the_mismatched_field() {
    for field in ALL_FIELDS {
        let (source, input) = base_for(field);
        let text = panic_text(catch_unwind(AssertUnwindSafe(|| {
            run_pair_with(source, input, 7, Method::Optimized, breaking(field))
        })));
        assert!(text.contains(&describe(source)), "ソースを含む: {text}");
        assert!(
            text.contains(&format!("入力 {input:?}")),
            "入力を含む: {text}"
        );
        assert!(text.contains("上限 7"), "上限を含む: {text}");
        assert!(
            text.contains(&format!("{:?}", vec![field])),
            "不一致項目 {field:?} を含む: {text}"
        );
        assert!(
            text.contains("素朴と Optimized の不一致"),
            "方式名を含む: {text}"
        );
    }
}

#[test]
fn run_pair_panics_when_the_method_does_not_complete() {
    let text = panic_text(catch_unwind(AssertUnwindSafe(|| {
        run_pair_with(",.+", b"\x05", 100, Method::Wasm, |_, _, _, _| {
            Err(MethodFailure::Failed(FailureKind::EngineError))
        })
    })));
    assert!(
        text.contains("完走しなかった"),
        "完走失敗のメッセージ: {text}"
    );
}

// --- 掃引 ---------------------------------------------------------------------

#[test]
fn sweep_range_is_zero_through_needed_plus_two() {
    assert_eq!(sweep_range(4), 0..=6);
    assert_eq!(sweep_range(0), 0..=2);
    assert_eq!(sweep_range(4).count(), 7);
}

#[test]
fn needed_steps_counts_each_instruction_and_sweep_compares_n_plus_three_limits() {
    // "+++." は 4 命令。掃引は上限 0..=6 の 7 回。
    assert_eq!(needed_steps("+++.", b""), 4);
    assert_eq!(sweep_limits("+++.", b"", Method::Optimized), 7);
    assert_eq!(sweep_limits("+++.", b"", Method::Wasm), 7);
}

// --- 列挙 ---------------------------------------------------------------------

#[test]
fn enumerate_balanced_counts_match_the_hand_counts() {
    // "[]" の対応列: 長さ 0,2,4,6,8 の Catalan 数 1,1,2,5,14 の合計。
    assert_eq!(enumerate_balanced(b"[]", 8).len(), 23);
    // 非括弧 4 種と括弧 2 種。長さ n の数は Σ C(n,2m)·Cat(m)·4^(n−2m)。
    assert_eq!(enumerate_balanced(b"+-<>[]", 6).len(), 10577);
    // 非括弧 6 種と括弧 2 種(8 文字)。長さ 4 以下は 1 + 6 + 37 + 234 + 1514。
    assert_eq!(enumerate_balanced(b"+-<>[].,", 4).len(), 1792);
}

#[test]
fn enumerate_balanced_lists_small_cases_exactly() {
    let mut got = enumerate_balanced(b"[]", 4);
    got.sort();
    let mut expected = vec!["", "[]", "[[]]", "[][]"];
    expected.sort();
    assert_eq!(got, expected);
}

#[test]
fn enumerate_balanced_is_unique_balanced_and_parseable() {
    let programs = enumerate_balanced(b"+-<>[].,", 4);
    let unique: HashSet<&String> = programs.iter().collect();
    assert_eq!(unique.len(), programs.len(), "重複がない");
    for program in &programs {
        let mut depth = 0i32;
        for byte in program.bytes() {
            match byte {
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
            assert!(depth >= 0, "途中で閉じ過ぎない: {program}");
        }
        assert_eq!(depth, 0, "括弧が対応する: {program}");
        Program::parse(program).expect("列挙したプログラムは解析できる");
    }
    assert_eq!(
        enumerate_balanced(b"+-<>[]", 6),
        enumerate_balanced(b"+-<>[]", 6),
        "決定的"
    );
}

// --- 擬似乱数 -------------------------------------------------------------------

#[test]
fn xorshift_matches_the_golden_sequence_for_seed_one() {
    let mut rng = XorShift64::new(1);
    let got = [rng.next_u64(), rng.next_u64(), rng.next_u64()];
    assert_eq!(got, [1082269761, 1152992998833853505, 11177516664432764457]);
}

#[test]
fn xorshift_zero_seed_uses_the_documented_constant() {
    let mut zero = XorShift64::new(0);
    let mut constant = XorShift64::new(0x9E37_79B9_7F4A_7C15);
    assert_eq!(zero.next_u64(), constant.next_u64());
    assert_eq!(XorShift64::new(0).next_u64(), 15860402102123842989);
    let mut stuck = XorShift64::new(0);
    for _ in 0..1000 {
        assert_ne!(stuck.next_u64(), 0, "0 のシードでも進み続ける");
    }
}

#[test]
fn xorshift_same_seed_gives_the_same_sequence_and_below_is_modulo() {
    let mut a = XorShift64::new(9);
    let mut b = XorShift64::new(9);
    for _ in 0..100 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
    let mut c = XorShift64::new(9);
    let mut d = XorShift64::new(9);
    for _ in 0..100 {
        assert_eq!(c.below(7), d.next_u64() % 7);
    }
    assert_eq!(XorShift64::new(3).below(1), 0);
}

// --- サンプル -------------------------------------------------------------------

#[test]
fn hello_source_is_the_workspace_sample_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hello.bf");
    let on_disk = std::fs::read_to_string(path).expect("共通サンプルを読める");
    assert_eq!(hello_source(), on_disk);
}

#[test]
fn hello_sample_prints_hello_world() {
    let program = Program::parse(hello_source()).expect("サンプルは解析できる");
    let execution = run_naive(&program, b"", u64::MAX).expect("素朴は失敗しない");
    assert_eq!(execution.observation.result, Ok(StopReason::Halted));
    assert_eq!(execution.observation.output, b"Hello World!\n");
}

#[test]
fn pair_diff_helper_returns_no_diff_for_hello() {
    assert!(pair_diff(hello_source(), b"", 100_000, Method::Wasm).is_empty());
}
