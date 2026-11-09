//! 走査(`Scan`)の WASM 生成に対する差分テストと構造テスト。
//!
//! 正解は素朴実装(`brainfuck-core` の `Machine`)。走査の反復数 `k` は、テストが
//! テープの内容から独立に求めた値を使う(上流のコストヘルパへは反復数を渡すだけで、求め方は持たない)。
//! 入力は `,` で 1 バイトずつ置く(開始セルとその先のセルを任意の値にできる)。

mod common;

use brainfuck_core::{Program, TAPE_SIZE};
use brainfuck_optimized::{Op, OpKind, OptimizedProgram};
use brainfuck_wasm::WasmProgram;
use common::{check_run, compile, core_observe, describe, sweep_all_limits};
use wasmparser::Validator;

/// 最後のセルの添字。
const LAST_CELL: usize = TAPE_SIZE - 1;

fn ir(source: &str) -> OptimizedProgram {
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    OptimizedProgram::compile(&program)
}

fn find_op(program: &OptimizedProgram, pred: impl Fn(&OpKind) -> bool) -> &Op {
    program
        .ops()
        .iter()
        .find(|op| pred(op.kind()))
        .expect("対象の操作がある")
}

/// (ソース, 入力, 開始セルの値 v, 反復数 k)。`k` はテストが手で数えた値。
const SCAN_CASES: &[(&str, &[u8], u8, u64)] = &[
    // 右へ 1 セル: c1 が 0 で 1 反復。
    (",[>]", &[1], 1, 1),
    // 右へ: c1, c2, c3 が非 0、c4 が 0。
    (",>,>,>,<<<[>]", &[1, 2, 3, 4], 1, 4),
    // 左へ: c3, c2, c1 が非 0、c0 が 0。
    (">,>,>,>,[<]", &[1, 2, 3, 4], 4, 4),
    // 幅 2 で右へ: c2, c4 が非 0、c6 が 0。
    (",>>,>>,<<<<[>>]", &[1, 2, 3], 1, 3),
    // 幅 2 で左へ: c4, c2 が非 0、c0 が 0。
    (">>,>>,>>,[<<]", &[1, 2, 3], 3, 3),
    // 開始セルの直後が 0 なら 1 反復。
    ("+[>]", &[], 1, 1),
];

#[test]
fn forward_and_backward_scans_match_core_across_every_limit() {
    for &(source, input, _, _) in SCAN_CASES {
        sweep_all_limits(source, input);
    }
}

#[test]
fn scan_applies_in_place_at_the_exact_limit_and_moves_the_pointer() {
    for &(source, input, _, k) in SCAN_CASES {
        let wasm = compile(source);
        let needed = core_observe(source, input, None).steps_executed;
        let run = check_run(&wasm, source, input, needed);
        assert!(
            !run.bailed_out,
            "{}: 必要数ちょうどでは離脱しない",
            describe(source)
        );
        assert_eq!(run.steps_executed, needed);
        assert!(k >= 1, "テスト表の反復数は 1 以上");
    }
}

#[test]
fn scan_cost_equals_the_naive_cost_helper_with_iterations_counted_by_the_test() {
    for &(source, input, start, k) in SCAN_CASES {
        let program = ir(source);
        let scan = find_op(&program, |kind| matches!(kind, OpKind::Scan { .. }));
        let expected: u64 = program
            .ops()
            .iter()
            .map(|op| {
                if std::ptr::eq(op, scan) {
                    op.naive_cost(start, k)
                } else {
                    op.naive_cost(0, 0)
                }
            })
            .sum();
        let run = check_run(&compile(source), source, input, 10_000);
        assert_eq!(
            run.steps_executed,
            expected,
            "{}: 走査の実行数は上流ヘルパの値と一致する",
            describe(source)
        );
        assert!(!run.bailed_out, "{}: 一括適用される", describe(source));
    }
}

#[test]
fn scan_cost_for_every_start_value_matches_the_helper() {
    // `,[>]` の走査は、開始セル v が非 0 なら c1 (0) で 1 反復、v = 0 なら本体を評価せず 1。
    let source = ",[>]";
    let program = ir(source);
    let scan = find_op(&program, |kind| matches!(kind, OpKind::Scan { .. }));
    let wasm = compile(source);
    for v in 0..=u8::MAX {
        let k = if v == 0 { 0 } else { 1 };
        let expected: u64 = program
            .ops()
            .iter()
            .map(|op| {
                if std::ptr::eq(op, scan) {
                    op.naive_cost(v, k)
                } else {
                    op.naive_cost(0, 0)
                }
            })
            .sum();
        let run = check_run(&wasm, source, &[v], 10_000);
        assert_eq!(run.steps_executed, expected, "セル値 {v} の走査の実行数");
        assert!(!run.bailed_out, "セル値 {v} は一括適用される");
    }
}

#[test]
fn scan_from_a_zero_cell_is_skipped_even_when_the_stride_would_leave_the_tape() {
    // 開始セル 0 では本体を評価しないため、右端の位置 29,999 からでも離脱しない。
    let source = format!("{}[>]+.", ">".repeat(29_999));
    let run = check_run(&compile(&source), &source, &[], 100_000);
    assert!(!run.bailed_out, "セル 0 の走査は範囲外でも離脱しない");
    assert_eq!(run.data_pointer, 29_999);
    assert_eq!(run.output, vec![1]);
}

#[test]
fn scan_out_of_range_at_the_left_edge_bails_and_matches_core() {
    // 位置 0 が非 0 で左へ走査すると、0 のセルに至る前にテープを出る。
    let source = "+[<]";
    let wasm = compile(source);
    for limit in 0..=8 {
        let run = check_run(&wasm, source, &[], limit);
        if limit >= 1 {
            assert!(run.bailed_out, "左端を出る走査は離脱する (上限 {limit})");
        }
    }
    let run = wasm.run(&[], 100).expect("実行できる");
    assert!(run.result.is_err(), "範囲外はエラーとして観測される");
    assert_eq!(run.data_pointer, 0, "素朴と同じく走査前の位置で止まる");
}

#[test]
fn scan_out_of_range_at_the_right_edge_bails_at_its_start() {
    // 位置 29,999 が非 0 で右へ走査すると、次の位置 30,000 はテープの外。
    let source = format!("{}+[>]", ">".repeat(29_999));
    let wasm = compile(&source);
    for limit in [
        0u64, 1, 2, 3, 29_999, 30_000, 30_001, 30_002, 30_003, 1_000_000,
    ] {
        check_run(&wasm, &source, &[], limit);
    }
    let run = wasm.run(&[], 1_000_000).expect("実行できる");
    assert!(run.bailed_out, "右端を出る走査は離脱する");
    assert!(run.result.is_err(), "範囲外はエラーとして観測される");
    assert_eq!(run.data_pointer, 29_999, "素朴と同じく走査前の位置で止まる");
}

#[test]
fn scan_reaching_the_last_cell_exactly_does_not_bail() {
    // 位置 29,998 が非 0 で右へ 1 歩進むと、0 の最後のセル 29,999 に届く。
    let source = format!("{},[>]", ">".repeat(29_998));
    let input = [1u8];
    let wasm = compile(&source);
    let needed = core_observe(&source, &input, None).steps_executed;
    let run = check_run(&wasm, &source, &input, needed);
    assert!(!run.bailed_out, "最後のセルへちょうど届く走査は離脱しない");
    assert_eq!(run.data_pointer, LAST_CELL);
}

#[test]
fn long_scan_over_the_whole_tape_completes_at_the_exact_limit() {
    // `,[>,]` で 29,999 個の 1 と末尾の 0 を書き、先頭へ戻って右へ走査する(反復 29,999)。
    // 最長の走査でも、必要数ちょうどで離脱せず、必要数より 1 少ないと離脱する。
    const RUN: usize = TAPE_SIZE - 1;
    let mut input = vec![1u8; RUN];
    input.push(0);
    let source = format!(",[>,]{}[>]", "<".repeat(RUN));
    let wasm = compile(&source);
    let needed = core_observe(&source, &input, None).steps_executed;

    let exact = check_run(&wasm, &source, &input, needed);
    assert!(!exact.bailed_out, "必要数ちょうどでは離脱しない");
    assert_eq!(exact.data_pointer, RUN);
    assert_eq!(exact.steps_executed, needed);

    let short = check_run(&wasm, &source, &input, needed - 1);
    assert!(short.bailed_out, "必要数より 1 少ないと離脱する");
}

#[test]
fn scan_inside_a_general_loop_bails_and_completes_with_the_right_depth() {
    // 外側の一般ループ(`,` を含む本体)の内側で、読んだ直後のセルを右へ走査する。
    let source = "+++[>,[>]<<-]";
    let input = [5u8, 6, 7];
    sweep_all_limits(source, &input);
    let needed = core_observe(source, &input, None).steps_executed;
    let run = check_run(&compile(source), source, &input, needed);
    assert!(!run.bailed_out, "必要数ちょうどでは離脱しない");
    assert_eq!(run.output, Vec::<u8>::new());
}

#[test]
fn program_with_every_operation_kind_validates_audits_and_is_deterministic() {
    // 加算・移動・出力・入力・一般ループ・クリア・転送・走査を 1 つのプログラムに含める。
    let source = "+,.>,>,<<[>]<[-]>[->+<],[.,]";
    let input = [1u8, 1, 0, 3, 0];
    let program = Program::parse(source).expect("解析できる");
    let optimized = OptimizedProgram::compile(&program);
    let has = |pred: fn(&OpKind) -> bool| optimized.ops().iter().any(|op| pred(op.kind()));
    assert!(has(|k| matches!(k, OpKind::Add(_))));
    assert!(has(|k| matches!(k, OpKind::Move(_))));
    assert!(has(|k| matches!(k, OpKind::Output)));
    assert!(has(|k| matches!(k, OpKind::Input)));
    assert!(has(|k| matches!(k, OpKind::LoopStart { .. })));
    assert!(has(|k| matches!(k, OpKind::LoopEnd { .. })));
    assert!(has(|k| matches!(k, OpKind::Clear { .. })));
    assert!(has(|k| matches!(k, OpKind::Transfer { .. })));
    assert!(has(|k| matches!(k, OpKind::Scan { .. })));

    let first = WasmProgram::compile(&optimized).expect("公開の窓口でコンパイルできる");
    let second = WasmProgram::compile(&OptimizedProgram::compile(&program))
        .expect("同じ入力から再度コンパイルできる");
    assert_eq!(
        first.module_bytes(),
        second.module_bytes(),
        "同じ入力から同じバイト列"
    );
    Validator::new()
        .validate_all(first.module_bytes())
        .expect("生成物は wasmparser の検証を通る");

    sweep_all_limits(source, &input);
    let needed = core_observe(source, &input, None).steps_executed;
    let run = check_run(&first, source, &input, needed);
    assert!(!run.bailed_out, "必要数ちょうどでは離脱しない");
}
