//! クリア(`[-]` / `[+]`)と移動/加算/乗算ループ(`Transfer`)の WASM 生成に対する差分テストと構造テスト。
//!
//! 正解は素朴実装(`brainfuck-core` の `Machine`)。開始セルは `,` で入力 1 バイトから置く
//! (任意の値 0〜255 を作れ、`+` の連続より短い)。「一括適用されている」ことは、
//! 必要数ちょうどの上限で離脱しないことで確かめる。コストは上流の `Op::naive_cost`、
//! 範囲条件は `Op::static_reach` と突き合わせる。

mod common;

use brainfuck_core::{Program, TAPE_SIZE};
use brainfuck_optimized::{Op, OpKind, OptimizedProgram};
use common::{check_run, compile, core_observe, describe, sweep_all_limits};

/// 開始セルの代表値(境界と中央)。
const SAMPLE_VALUES: [u8; 6] = [0, 1, 2, 127, 128, 255];

/// 最後のセルの添字。
const LAST_CELL: isize = TAPE_SIZE as isize - 1;

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

/// 必要数ちょうどの上限では離脱せず完走する(操作が一括適用されている)。
fn assert_applied_at_exact_limit(source: &str, input: &[u8]) {
    let wasm = compile(source);
    let needed = core_observe(source, input, None).steps_executed;
    let run = check_run(&wasm, source, input, needed);
    assert!(
        !run.bailed_out,
        "{}: 必要数ちょうどでは離脱しない",
        describe(source)
    );
}

#[test]
fn clear_matches_core_for_sample_values_in_both_directions() {
    for source in [",[-]", ",[+]"] {
        for v in SAMPLE_VALUES {
            let input = [v];
            sweep_all_limits(source, &input);
            assert_applied_at_exact_limit(source, &input);
        }
    }
}

#[test]
fn clear_cost_equals_the_naive_cost_helper_for_every_cell_value() {
    for source in [",[-]", ",[+]"] {
        let program = ir(source);
        let clear = find_op(&program, |k| matches!(k, OpKind::Clear { .. }));
        let wasm = compile(source);
        for v in 0..=u8::MAX {
            // `,` の 1 と、上流ヘルパが返すクリアのコスト(セル値 v)の和。
            let expected: u64 = program
                .ops()
                .iter()
                .map(|op| op.naive_cost(if std::ptr::eq(op, clear) { v } else { 0 }, 0))
                .sum();
            let run = check_run(&wasm, source, &[v], 10_000);
            assert_eq!(
                run.steps_executed,
                expected,
                "{}: セル値 {v} のクリアの実行数",
                describe(source)
            );
            assert!(
                !run.bailed_out,
                "{}: セル値 {v} は一括適用される",
                describe(source)
            );
        }
    }
}

#[test]
fn transfer_matches_core_for_sample_values_and_shapes() {
    // 右への加算、左への加算(開始位置を 1 つ右から始める)、複数の係数、係数 255(-1)。
    let cases = [">,[->+<]>.", ">,[-<++>]<.", ">>,[->+>++<<]>.", ">,[->-<]."];
    for source in cases {
        for v in SAMPLE_VALUES {
            let input = [v];
            sweep_all_limits(source, &input);
            assert_applied_at_exact_limit(source, &input);
        }
    }
}

#[test]
fn transfer_cost_equals_the_naive_cost_helper_for_every_cell_value() {
    let source = ">,[->+<]>.";
    let program = ir(source);
    let transfer = find_op(&program, |k| matches!(k, OpKind::Transfer { .. }));
    let wasm = compile(source);
    for v in 0..=u8::MAX {
        // 移動・入力・出力は開始セルに依らず、転送だけがセル値 v に依る。
        let expected: u64 = program
            .ops()
            .iter()
            .map(|op| op.naive_cost(if std::ptr::eq(op, transfer) { v } else { 0 }, 0))
            .sum();
        let run = check_run(&wasm, source, &[v], 10_000);
        assert_eq!(run.steps_executed, expected, "セル値 {v} の転送の実行数");
        assert!(!run.bailed_out, "セル値 {v} は一括適用される");
    }
}

#[test]
fn transfer_with_a_zero_start_cell_is_skipped_even_when_its_reach_leaves_the_tape() {
    // 開始セル 0 では本体を評価しないため、範囲外に触れうる `[<+>-]` でも離脱しない。
    let source = "[<+>-]+.";
    let run = check_run(&compile(source), source, &[], 100);
    assert!(!run.bailed_out, "セル 0 の転送は離脱しない");
    assert_eq!(run.output, vec![1]);
    // `[` の 1、`+` の 1、`.` の 1。
    assert_eq!(run.steps_executed, 3);
}

#[test]
fn transfer_skipped_at_the_right_edge_does_not_bail() {
    // 位置 29,999 で `[>+<-]` の `>` は範囲外に出るが、セル 0 なので本体は評価されない。
    let source = format!("{}[>+<-]+.", ">".repeat(29_999));
    let run = check_run(&compile(&source), &source, &[], 100_000);
    assert!(!run.bailed_out, "セル 0 の転送は範囲外でも離脱しない");
    assert_eq!(run.data_pointer, 29_999);
}

#[test]
fn transfer_with_a_nonzero_cell_out_of_reach_bails_at_its_start() {
    // 位置 0 のセルが非 0 のとき、本体の `<` が範囲外に出る。上流と同じく状態を変えずに離脱する。
    let source = "+[<+>-]";
    let run = check_run(&compile(source), source, &[], 100);
    assert!(run.bailed_out, "範囲外に出る非 0 の転送は離脱する");
    assert_eq!(run.data_pointer, 0);
}

#[test]
fn transfer_reach_edge_is_exact_at_the_last_cell() {
    // `[>+<-]` は +1 まで到達する。位置 29,998 は最後のセルに届くので離脱しない。
    // 位置 29,999 では届かないので、非 0 のとき離脱する。
    let input = [1u8];
    let fits = format!("{}, [>+<-]", ">".repeat(29_998)).replace(' ', "");
    let run = check_run(&compile(&fits), &fits, &input, 100_000);
    assert!(!run.bailed_out, "最後のセルへちょうど届く転送は離脱しない");

    let beyond = format!("{}, [>+<-]", ">".repeat(29_999)).replace(' ', "");
    let run = check_run(&compile(&beyond), &beyond, &input, 100_000);
    assert!(run.bailed_out, "最後のセルを越える転送は離脱する");
}

#[test]
fn bail_condition_matches_the_upstream_reach_helper() {
    // 本体ごとの到達範囲(上流の `static_reach`)と開始位置から、離脱の有無を独立に求めて比べる。
    let bodies = ["[<+>-]", "[>+<-]", "[>>+<<-]", "[<<+>>-]"];
    let starts: [isize; 7] = [0, 1, 2, 29_996, 29_997, 29_998, 29_999];
    for body in bodies {
        for start in starts {
            let source = format!("{}, {body}", ">".repeat(start as usize)).replace(' ', "");
            let program = ir(&source);
            let transfer = find_op(&program, |k| matches!(k, OpKind::Transfer { .. }));
            let (min, max) = transfer.static_reach().expect("転送には到達範囲がある");
            let expected_bail = start + min < 0 || start + max > LAST_CELL;

            let wasm = compile(&source);
            let run = check_run(&wasm, &source, &[1], 1_000_000);
            assert_eq!(
                run.bailed_out, expected_bail,
                "開始 {start}・本体 {body}: 離脱の有無は到達範囲の条件と一致する"
            );
        }
    }
}

#[test]
fn clear_and_transfer_inside_general_loops_match_core() {
    // 一般ループの内側(枠が開いた状態)でも、クリアと転送の離脱先の深さが正しい。
    let cases: &[(&str, &[u8])] = &[
        ("+[>,[-]<-]", &[5]),
        ("+[>,[-]<-]", &[0]),
        ("+[>,[-]<-]", &[255]),
        ("+++[>,[->+<]<-]", &[2, 1, 0]),
        ("+++[>,[+]<-]", &[9, 0, 255]),
    ];
    for (source, input) in cases {
        sweep_all_limits(source, input);
        assert_applied_at_exact_limit(source, input);
    }
}

#[test]
fn worst_case_long_transfer_at_cell_255_matches_core_and_does_not_exhaust_fuel() {
    // 本体 301 命令・到達 +100 の転送を、セル値 255 で 1 回実行する。
    // 実行数は `,` の 1 と、`[` の 1 + 255 × (本体の 301 + `]` の 1) の和。
    let body = format!("-{}{}", ">+".repeat(100), "<".repeat(100));
    let source = format!(",[{body}]");
    let expected = 1 + 1 + 255 * (body.len() as u64 + 1);
    let run = check_run(&compile(&source), &source, &[255], 100_000);
    assert!(!run.bailed_out, "最悪ケースも一括適用される");
    assert_eq!(run.steps_executed, expected);
    assert_applied_at_exact_limit(&source, &[255]);
}
