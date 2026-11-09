//! 一般ループ(`LoopStart` / `LoopEnd`)を含むプログラムの WASM 生成に対する差分テストと構造テスト。
//!
//! 正解は素朴実装(`brainfuck-core` の `Machine`)。上限掃引では、停止するプログラムを
//! 0 から「必要数 + 2」までの全上限で比べる。`hello` はワークスペース共通のサンプルを読み込む。
//! 入れ子の上限(256 / 257)とモジュールの上限は、公開の `WasmProgram::compile` の結果で確かめる。

mod common;

use brainfuck_core::{Program, StopReason};
use brainfuck_optimized::{OpKind, OptimizedProgram};
use brainfuck_wasm::{CompileError, WasmProgram};
use common::{check_run, compile, core_observe, sweep_all_limits};

#[test]
fn hello_sample_matches_core_across_every_limit() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hello.bf");
    let source = std::fs::read_to_string(path).expect("共通サンプル examples/hello.bf を読める");
    let full = core_observe(&source, &[], None);
    assert_eq!(full.result, Ok(StopReason::Halted));
    sweep_all_limits(&source, &[]);
}

#[test]
fn echo_matches_core_for_zero_and_ff_inputs() {
    // `,[.,]`: 入力を読み、非 0 の間は出力して次を読む。0x00 と 0xFF を含む入力を使う。
    let echo = ",[.,]";
    sweep_all_limits(echo, &[0x00, 0xFF]);
    sweep_all_limits(echo, &[0xFF, 0x00, 0x41]);
    sweep_all_limits(echo, &[]);
}

#[test]
fn loop_programs_match_core_across_every_limit() {
    let cases: &[(&str, &[u8])] = &[
        ("[]", &[]),
        ("[.]", &[]),
        ("[+]+.", &[]),
        ("+[-].", &[]),
        ("+[.-]", &[]),
        ("++[>+<-]>.", &[]),
        ("+++[>++[>+<-]<-]>.", &[]),
        (",[>+<-]>.", &[7]),
        (">>+[<<+>>-]<<.", &[]),
        ("++[>+<-]>[<+>-]<.", &[]),
        ("++[[-]>+<]>.", &[]),
        ("+[[-]+.-]", &[]),
    ];
    for (source, input) in cases {
        sweep_all_limits(source, input);
    }
}

#[test]
fn loop_body_with_zero_cell_is_skipped_without_entering_the_body() {
    // セル 0 の `[` は本体を実行せず、後続の `+` まで進む。
    let wasm = compile("[.+]+.");
    let run = check_run(&wasm, "[.+]+.", &[], 100);
    assert_eq!(run.result, Ok(StopReason::Halted));
    assert!(!run.bailed_out);
    assert_eq!(run.output, vec![1]);
    // `[` で 1、スキップ後の `+` と `.` で 2。素朴の実行数は 3。
    assert_eq!(run.steps_executed, 3);
}

#[test]
fn infinite_loop_bails_at_every_small_limit_and_matches_core() {
    // `+[]` は永遠に回る。上限で必ず離脱し、再生の結果は上限エラーと一致する。
    let source = "+[]";
    let wasm = compile(source);
    for limit in 0..=50 {
        let run = check_run(&wasm, source, &[], limit);
        assert!(run.bailed_out, "上限 {limit} で離脱する");
        assert!(run.result.is_err(), "上限 {limit} で上限エラー");
    }
}

#[test]
fn loop_costs_match_the_upstream_helper_at_the_exact_boundary() {
    // 括弧の 1 命令は素朴換算で 1 ずつ数えられる(上流のコストヘルパ)。
    // 出力を含むため一般ループのまま(クリア・移動ループの種別にはならない)。
    let source = "++[.-[.-]]+.";
    let program = Program::parse(source).expect("解析できる");
    let ir = OptimizedProgram::compile(&program);
    for op in ir.ops() {
        if matches!(op.kind(), OpKind::LoopStart { .. } | OpKind::LoopEnd { .. }) {
            assert_eq!(op.naive_cost(0, 0), 1, "括弧は 1 実行数");
        }
    }

    // 素朴の必要数ちょうどの上限で停止し、離脱しない。それより 1 少ない上限では離脱する。
    // 生成コードの数える実行数が上流の値より多くても少なくても、この 2 点のどちらかが崩れる。
    let total = core_observe(source, &[], None).steps_executed;
    let wasm = compile(source);
    let exact = check_run(&wasm, source, &[], total);
    assert_eq!(exact.result, Ok(StopReason::Halted));
    assert!(!exact.bailed_out, "必要数ちょうどでは離脱しない");
    assert_eq!(exact.steps_executed, total);
    let short = check_run(&wasm, source, &[], total - 1);
    assert!(short.bailed_out, "必要数より 1 少ないと離脱する");
}

#[test]
fn loop_moving_past_the_right_edge_bails_inside_the_loop() {
    // 位置 29,999 の本体で `>` が範囲外に出る。離脱点はループ内の移動操作の先頭。
    let source = format!("{}+[>+<-]", ">".repeat(29_999));
    let wasm = compile(&source);
    for limit in [
        0u64, 1, 2, 3, 29_999, 30_000, 30_001, 30_002, 30_003, 1_000_000,
    ] {
        check_run(&wasm, &source, &[], limit);
    }
    let run = wasm.run(&[], 1_000_000).expect("実行できる");
    assert!(run.bailed_out, "範囲外への移動で離脱する");
    assert!(run.result.is_err(), "範囲外はエラーとして観測される");
    assert_eq!(run.data_pointer, 29_999, "素朴と同じく移動前の位置で止まる");
}

#[test]
fn loop_moving_past_the_left_edge_bails_and_matches_core() {
    // 位置 0 のセルが非 0 のとき、本体の `<` が範囲外に出る。上限を変えても素朴と完全一致する。
    let source = "+[<+>-]";
    let wasm = compile(source);
    for limit in 0..=20 {
        let run = check_run(&wasm, source, &[], limit);
        assert!(
            run.bailed_out,
            "範囲外の移動または上限で離脱する (上限 {limit})"
        );
    }
}

#[test]
fn nesting_of_256_compiles_and_257_is_rejected_as_too_deep() {
    let ok_source = format!("{}+.{}", "[".repeat(256), "]".repeat(256));
    let ok_program = Program::parse(&ok_source).expect("解析できる");
    let ok_ir = OptimizedProgram::compile(&ok_program);
    assert!(
        WasmProgram::compile(&ok_ir).is_ok(),
        "入れ子 256 は生成できる"
    );

    let deep_source = format!("{}+.{}", "[".repeat(257), "]".repeat(257));
    let deep_program = Program::parse(&deep_source).expect("解析できる");
    let deep_ir = OptimizedProgram::compile(&deep_program);
    match WasmProgram::compile(&deep_ir) {
        Err(CompileError::NestingTooDeep) => {}
        Err(other) => panic!("入れ子 257 は NestingTooDeep になるはず: {other:?}"),
        Ok(_) => panic!("入れ子 257 は生成できないはず"),
    }
}

#[test]
fn module_over_the_size_limit_is_rejected_as_too_large() {
    // 操作が多く、生成物が 8 MiB を超える合成プログラム(`+.` の繰り返し)。
    let source = "+.".repeat(300_000);
    let program = Program::parse(&source).expect("解析できる");
    let ir = OptimizedProgram::compile(&program);
    match WasmProgram::compile(&ir) {
        Err(CompileError::ModuleTooLarge) => {}
        Err(other) => panic!("規模超過は ModuleTooLarge になるはず: {other:?}"),
        Ok(_) => panic!("規模超過は生成できないはず"),
    }
}

#[test]
fn loop_programs_emit_deterministically() {
    let source = "+++[>++[>+<-]<-]>.,[.,]";
    let program = Program::parse(source).expect("解析できる");
    let ir = OptimizedProgram::compile(&program);
    let first = WasmProgram::compile(&ir).expect("生成できる");
    let second = WasmProgram::compile(&ir).expect("生成できる");
    assert_eq!(first.module_bytes(), second.module_bytes());
}

#[test]
fn multi_iteration_general_loops_match_core_across_every_limit() {
    // 2 反復以上回る一般ループ。`[` の検査・実行数は入る時の 1 回だけで、反復ごとには加算されない。
    // いずれも出力を含むため、クリア・移動ループ・走査の種別にはならず一般ループのまま残る。
    let cases: &[(&str, &[u8])] = &[
        ("+++[-.]", &[]),
        ("+++[.-]", &[]),
        (",[.,]", &[1, 2, 3]),
        (",[.,]", &[0xFF, 0x00, 0x41]),
    ];
    for (source, input) in cases {
        let program = Program::parse(source).expect("解析できる");
        let ir = OptimizedProgram::compile(&program);
        assert!(
            ir.ops()
                .iter()
                .any(|op| matches!(op.kind(), OpKind::LoopStart { .. })),
            "{source}: 一般ループとして残る(種別へ置換されない)"
        );
        sweep_all_limits(source, input);
    }
}

#[test]
fn loop_open_bracket_is_charged_once_on_entry_not_per_iteration() {
    // `+++[-.]`: 素朴の実行数は 3 + 1 + 3 × 3 = 13。`[` を反復ごとに数えると 15 になる。
    let source = "+++[-.]";
    let total = core_observe(source, &[], None).steps_executed;
    assert_eq!(total, 13);
    let wasm = compile(source);
    let run = check_run(&wasm, source, &[], total);
    assert!(!run.bailed_out, "必要数ちょうどでは離脱しない");
    assert_eq!(run.steps_executed, 13);
    assert_eq!(run.output, vec![2, 1, 0]);
}
