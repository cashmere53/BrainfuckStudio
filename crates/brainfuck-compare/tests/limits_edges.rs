//! 上限掃引・端・入出力の差分テスト(3.6, 要件 4.2–4.5, 6.1–6.4, 13.1, 13.5–13.7, 13.11)。
//!
//! 代表プログラム(融合操作、`[-]`、`[+]`、乗算、走査、一般ループ、空、`+[]`)を WASM 方式で
//! 素朴と比較する。上限は 0 から必要数 + 2 までを全数比較し、操作の途中の上限を網羅する。
//! テープの右端は `dp` を 29,990〜29,999 へ運ぶ前置きの後で、左端は `dp = 0` から、それぞれ
//! 範囲外エラーと境界上の成功の両方を比較する。入力は空・尽きる・0 と 255 を含む場合を比較する。
//!
//! パターンテンプレートはこのファイル内に置く(共通基盤は読むだけ)。

mod common;

use std::collections::BTreeSet;

use brainfuck_compare::limits::MAX_COMPARISON_STEPS;
use brainfuck_compare::methods::run_naive;
use brainfuck_compare::Method;
use brainfuck_core::machine::TAPE_SIZE;
use brainfuck_core::{PointerDirection, Program, RuntimeError};
use brainfuck_optimized::OptimizedProgram;
use common::{describe, hello_source, run_pair, sweep_range};

/// 右端の前置きに使うデータポインタの下限と上限(両端を含む)。
const RIGHT_PREFIX_FIRST: usize = TAPE_SIZE - 10;
const RIGHT_PREFIX_LAST: usize = TAPE_SIZE - 1;

/// 代表プログラム(ソース, 入力)。すべて停止する。
const REPRESENTATIVE: &[(&str, &[u8])] = &[
    // 融合操作(同種の加減算・移動の連続)
    ("+++>++<--.>+", b""),
    ("+++++>>--<<.>>.", b""),
    // クリア `[-]` と `[+]`(セル値 0, 1, 2, 127, 128, 255 のいずれにも当たりうる)
    ("+++++[-].", b""),
    ("+[-]+++.", b""),
    ("+++++[+]+.", b""),
    // 乗算ループ
    ("++++[>+++<-]>.", b""),
    ("++[>+<-]>.", b""),
    ("+[>+<-]>.", b""),
    // 走査ループ(右方向は停止、左方向は左端で範囲外)
    ("+>+>+>+[>]+.", b""),
    ("+>+>+>+[<]", b""),
    // 一般ループ(入れ子とクリアを含む、乗算・走査の形を外れるもの)
    ("++[>+[-]<-]", b""),
    ("++[>+<--]", b""),
    (",[.,]", b"\x05\x00\xff"),
    // 空
    ("", b""),
];

/// 上限の掃引を行わない、停止しないプログラム。
const NON_HALTING: &str = "+[]";

/// ソースを素朴で最後まで実行したときの命令数(停止しても範囲外で止まってもよい)。
fn terminal_steps(source: &str, input: &[u8]) -> u64 {
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    run_naive(&program, input, u64::MAX)
        .expect("素朴は失敗しない")
        .observation
        .steps_executed
}

/// 上限 0 から `sweep_range(steps)` の全上限を WASM で比較し、比較した回数を返す。
fn sweep_all(source: &str, input: &[u8]) -> u64 {
    let steps = terminal_steps(source, input);
    let mut compared = 0;
    for limit in sweep_range(steps) {
        run_pair(source, input, limit, Method::Wasm);
        compared += 1;
    }
    assert_eq!(compared, steps + 3, "{}: 掃引の回数", describe(source));
    compared
}

/// 指定の上限候補のうち、必要数の周辺だけを WASM で比較する。
fn sweep_selected(source: &str, input: &[u8], extra: &[u64]) -> usize {
    let steps = terminal_steps(source, input);
    let mut limits = BTreeSet::new();
    for candidate in [
        0,
        1,
        steps.saturating_sub(2),
        steps.saturating_sub(1),
        steps,
        steps + 1,
        steps + 2,
    ]
    .into_iter()
    .chain(extra.iter().copied())
    {
        if candidate <= steps + 2 {
            limits.insert(candidate);
        }
    }
    for &limit in &limits {
        run_pair(source, input, limit, Method::Wasm);
    }
    limits.len()
}

#[test]
fn representative_programs_match_naive_at_every_limit_on_wasm() {
    for &(source, input) in REPRESENTATIVE {
        sweep_all(source, input);
        // 上限なし(比較用の最大値)でも一致する。
        run_pair(source, input, MAX_COMPARISON_STEPS, Method::Wasm);
    }
}

#[test]
fn clear_loops_match_naive_for_cell_values_at_the_range_edges() {
    // `[-]` と `[+]` のクリアを、セル値 0, 1, 2, 127, 128, 255 で全上限比較する。
    for value in [0usize, 1, 2, 127, 128, 255] {
        let set = "+".repeat(value);
        sweep_all(&format!("{set}[-]."), b"");
        sweep_all(&format!("{set}[+]."), b"");
    }
}

#[test]
fn non_halting_loop_matches_naive_at_small_and_large_limits() {
    // 停止しないので必要数を求めず、固定の上限群で比較する。
    for limit in (0..=300).chain([10_000, 100_000]) {
        run_pair(NON_HALTING, b"", limit, Method::Wasm);
    }
}

#[test]
fn hello_sample_matches_naive_at_the_boundary_limits_on_wasm() {
    let source = hello_source();
    let steps = terminal_steps(source, b"");
    for limit in [0, 1, steps / 2, steps - 1, steps, steps + 1, steps + 2] {
        run_pair(source, b"", limit, Method::Wasm);
    }
}

#[test]
fn right_edge_programs_match_naive_for_out_of_range_and_boundary_successes() {
    // 右端: dp を RIGHT_PREFIX_FIRST..=RIGHT_PREFIX_LAST へ運び、末尾に以下の尾部を置く。
    // 各尾部は (a) 最後のセルに届く成功、(b) 1 つ越えて範囲外、(c) 連続移動、(d) 走査、
    // (e) Transfer(本物の移動/加算の形)と、正味移動のある一般ループ、を含む。
    let mut errors = 0;
    let mut successes = 0;
    for prefix_len in RIGHT_PREFIX_FIRST..=RIGHT_PREFIX_LAST {
        let to_last = TAPE_SIZE - 1 - prefix_len;
        let past_last = TAPE_SIZE - prefix_len;
        let tails = [
            String::new(),
            ">".repeat(to_last),
            ">".repeat(past_last),
            "<<".to_string(),
            "+[>]".to_string(),
            "[->+<]".to_string(),
            "+[->+<]".to_string(),
            "[->>+<<]".to_string(),
            "+[->>+<<]".to_string(),
            "[->>+<]".to_string(),
            "+[<]".to_string(),
        ];
        for tail in tails {
            let source = format!("{}{tail}", ">".repeat(prefix_len));
            let steps = terminal_steps(&source, b"");
            let limits = [
                prefix_len as u64,
                prefix_len as u64 - 1,
                prefix_len as u64 + 1,
            ];
            sweep_selected(&source, b"", &limits);

            let program = Program::parse(&source).expect("テスト用ソースは解析できる");
            let oracle = run_naive(&program, b"", MAX_COMPARISON_STEPS)
                .expect("素朴は失敗しない")
                .observation;
            match oracle.result {
                Ok(_) => successes += 1,
                Err(_) => errors += 1,
            }
            // 最後まで実行する上限でも比較する(範囲外エラーの pc・dp・方向を含む)。
            run_pair(&source, b"", steps, Method::Wasm);
        }
    }
    assert!(errors > 0, "範囲外エラーの比較が含まれる");
    assert!(successes > 0, "境界上の成功の比較が含まれる");
}

#[test]
fn right_edge_out_of_range_reports_the_pointer_error_on_the_last_cell() {
    // 最後のセルで 1 つ右へ移動すると範囲外。WASM の結果もこの値と一致する。
    let source = format!("{}>", ">".repeat(TAPE_SIZE - 1));
    let execution = run_pair(&source, b"", MAX_COMPARISON_STEPS, Method::Wasm);
    assert!(
        matches!(
            execution.observation.result,
            Err(RuntimeError::PointerOutOfBounds {
                dp: 29_999,
                direction: PointerDirection::Right,
                ..
            })
        ),
        "最後のセルからの右移動は範囲外 {:?}",
        execution.observation.result
    );
    assert_eq!(execution.observation.data_pointer, TAPE_SIZE - 1);
}

/// 最適化後の最後の操作が `Transfer`(移動/加算/乗算の本物の形)であることを確かめ、その到達範囲を返す。
///
/// `Transfer` は正味移動 0・開始セルの正味変化 -1 の本体だけが該当する。`[->>+<]` のように
/// 正味移動がある本体は一般ループになるため、ここでは使わない。
fn assert_transfer_reach(source: &str) -> (isize, isize) {
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    let ir = OptimizedProgram::compile(&program);
    let last = ir.ops().last().expect("操作が 1 つ以上ある");
    assert!(
        format!("{last:?}").contains("Transfer {"),
        "{}: 最後の操作は Transfer のはず: {last:?}",
        describe(source)
    );
    last.static_reach().expect("Transfer は到達範囲を持つ")
}

#[test]
fn transfer_with_zero_start_cell_at_the_right_edge_is_not_bailed_out() {
    // 開始セル 0 の Transfer は本体を評価せず、範囲外に触れうる到達範囲でも離脱しない。
    for body in ["[->+<]", "[->>+<<]"] {
        let source = format!("{}{body}", ">".repeat(TAPE_SIZE - 1));
        let (_, reach_max) = assert_transfer_reach(&source);
        assert!(
            reach_max > 0,
            "{}: 到達範囲は右端を越える",
            describe(&source)
        );
        let execution = run_pair(&source, b"", MAX_COMPARISON_STEPS, Method::Wasm);
        assert!(
            execution.observation.halted(),
            "{}: 正常停止する",
            describe(&source)
        );
        assert_eq!(
            execution.metrics.replayed_steps,
            Some(0),
            "{}: 離脱せず、ホストの再生は 0 命令",
            describe(&source)
        );
    }
}

#[test]
fn transfer_with_nonzero_start_cell_at_the_right_edge_reports_the_pointer_error() {
    // 同じ位置でセル 1 にすると Transfer の本体が範囲外に触れて失敗する(比較で一致する)。
    for body in ["[->+<]", "[->>+<<]"] {
        let source = format!("{}+{body}", ">".repeat(TAPE_SIZE - 1));
        assert_transfer_reach(&source);
        let execution = run_pair(&source, b"", MAX_COMPARISON_STEPS, Method::Wasm);
        assert!(
            matches!(
                execution.observation.result,
                Err(RuntimeError::PointerOutOfBounds {
                    dp: 29_999,
                    direction: PointerDirection::Right,
                    ..
                })
            ),
            "{}: 範囲外に触れる Transfer は右方向の範囲外エラー {:?}",
            describe(&source),
            execution.observation.result
        );
    }
}

#[test]
fn transfer_with_zero_start_cell_at_the_left_edge_is_not_bailed_out() {
    // dp = 0・セル 0 の Transfer `[<+>-]` は左へ範囲外に触れうるが、評価せず離脱もしない。
    let source = "[<+>-]";
    let (reach_min, _) = assert_transfer_reach(source);
    assert_eq!(reach_min, -1, "到達範囲の左端");
    let execution = run_pair(source, b"", MAX_COMPARISON_STEPS, Method::Wasm);
    assert!(execution.observation.halted(), "正常停止する");
    assert_eq!(execution.metrics.replayed_steps, Some(0), "離脱せず 0 命令");
}

#[test]
fn transfer_with_nonzero_start_cell_at_the_left_edge_reports_the_pointer_error() {
    // dp = 0・セル 1 の Transfer は左へ範囲外に触れて失敗する。
    let source = "+[<+>-]";
    assert_transfer_reach(source);
    let execution = run_pair(source, b"", MAX_COMPARISON_STEPS, Method::Wasm);
    assert!(
        matches!(
            execution.observation.result,
            Err(RuntimeError::PointerOutOfBounds {
                dp: 0,
                direction: PointerDirection::Left,
                ..
            })
        ),
        "dp = 0 の Transfer は左方向の範囲外エラー {:?}",
        execution.observation.result
    );
}

#[test]
fn general_loop_that_moves_with_zero_cell_is_not_evaluated_at_the_right_edge() {
    // 正味移動のある一般ループ `[->>+<]` は Transfer ではない。セル 0 では本体を評価せず、
    // 離脱もしない(一般ループの経路の確認)。
    let source = format!("{}[->>+<]", ">".repeat(TAPE_SIZE - 1));
    let program = Program::parse(&source).expect("テスト用ソースは解析できる");
    let ir = OptimizedProgram::compile(&program);
    assert!(
        !format!("{:?}", ir.ops().last().expect("操作がある")).contains("Transfer {"),
        "正味移動のある本体は Transfer にならない"
    );
    let execution = run_pair(&source, b"", MAX_COMPARISON_STEPS, Method::Wasm);
    assert!(execution.observation.halted(), "正常停止する");
    assert_eq!(execution.metrics.replayed_steps, Some(0), "離脱せず 0 命令");
}

#[test]
fn left_edge_programs_match_naive_for_out_of_range_and_boundary_successes() {
    // 左端: dp = 0 から、範囲外エラーになる形と、境界上で成功する形を並べる。
    const LEFT_EDGE: &[&str] = &[
        "<",
        "<<",
        "+<",
        "+<+>",
        "+[<+>-]",
        "[<+>-]",
        "+[<]",
        "+>+<[<]",
        "+>+<<",
        "+>>+<<<[>]",
    ];
    for &source in LEFT_EDGE {
        sweep_all(source, b"");
    }
}

#[test]
fn left_edge_out_of_range_reports_the_pointer_error_at_dp_zero() {
    let execution = run_pair("+<", b"", MAX_COMPARISON_STEPS, Method::Wasm);
    assert!(
        matches!(
            execution.observation.result,
            Err(RuntimeError::PointerOutOfBounds {
                pc: 1,
                dp: 0,
                direction: PointerDirection::Left,
            })
        ),
        "dp = 0 からの左移動は範囲外 {:?}",
        execution.observation.result
    );
}

#[test]
fn input_programs_match_naive_for_empty_exhausted_zero_and_max_inputs() {
    // (ソース, 入力, 期待する出力)。EOF の読み取りは 0 を格納し、入力位置を進めない。
    const CASES: &[(&str, &[u8], &[u8])] = &[
        (",.,.,.", b"", &[0, 0, 0]),
        (",.,.,.", b"\x00\xff", &[0, 255, 0]),
        (",.,.", b"\xff", &[255, 0]),
        (",+.,+.,+.", b"\xff", &[0, 1, 1]),
        (",[.,]", b"\x05\x00\xff", &[5]),
        (",[.,]", b"\xff\x00\x01", &[255]),
        (",,.", b"\x00", &[0]),
    ];
    for &(source, input, expected) in CASES {
        sweep_all(source, input);
        let execution = run_pair(source, input, MAX_COMPARISON_STEPS, Method::Wasm);
        assert_eq!(
            execution.observation.output,
            expected,
            "{}: 入力 {input:?} の出力",
            describe(source)
        );
    }
}
