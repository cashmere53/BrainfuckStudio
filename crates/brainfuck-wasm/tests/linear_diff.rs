//! 直線的な操作のみのプログラム(加減算・移動・入出力・空プログラム)に対する
//! WASM 実行と素朴実装(`brainfuck-core` の `Machine`)の差分テスト。
//!
//! 比較する項目は、結果(停止またはエラー)、`pc`、データポインタ、素朴換算の実行数、
//! 入力読取位置、出力、テープ全体である。離脱の有無は `WasmRun::bailed_out` で判定する。
//! 広い網羅は比較クレートの差分テストが持つため、ここでは代表に限る。
//! `hello` はワークスペース共通のサンプル `examples/hello.bf` を読み込み、コピーは置かない。

use brainfuck_core::{Machine, MachineConfig, Program, RuntimeError, StopReason, TAPE_SIZE};
use brainfuck_optimized::{Op, OptimizedProgram};
use brainfuck_wasm::{WasmProgram, WasmRun};

/// 最後のセルの添字(`dp` の有効範囲は `0..=LAST_CELL`)。
const LAST_CELL: isize = TAPE_SIZE as isize - 1;

/// 素朴実装(core)の完全観測値。
#[derive(Debug, PartialEq)]
struct Observed {
    result: Result<StopReason, RuntimeError>,
    pc: usize,
    data_pointer: usize,
    steps_executed: u64,
    input_position: usize,
    output: Vec<u8>,
    tape: Vec<u8>,
}

/// 素朴実装を実行して完全観測値を得る(オラクル側)。`limit` が `None` なら上限なし。
fn core_observe(source: &str, input: &[u8], limit: Option<u64>) -> Observed {
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    let config = match limit {
        Some(n) => MachineConfig::with_max_steps(n),
        None => MachineConfig::unlimited(),
    };
    let mut machine = Machine::new(program, input.to_vec(), config);
    let result = machine.run();
    let snapshot = machine.snapshot();
    Observed {
        result,
        pc: snapshot.pc,
        data_pointer: snapshot.data_pointer,
        steps_executed: snapshot.steps_executed,
        input_position: machine.input_position(),
        output: machine.output().to_vec(),
        tape: machine.tape().to_vec(),
    }
}

/// WASM 実行の観測値を、素朴実装と同じ形へ写す(離脱の情報は含めない)。
fn wasm_observe(run: &WasmRun) -> Observed {
    Observed {
        result: run.result,
        pc: run.pc,
        data_pointer: run.data_pointer,
        steps_executed: run.steps_executed,
        input_position: run.input_position,
        output: run.output.clone(),
        tape: run.tape.to_vec(),
    }
}

/// ソースから WASM プログラムをコンパイルする(解析 → 最適化 → コンパイル)。
fn compile(source: &str) -> WasmProgram {
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    let ir = OptimizedProgram::compile(&program);
    WasmProgram::compile(&ir).expect("直線的なプログラムはコンパイルできる")
}

/// ソースの最適化後の操作列。
fn optimized(source: &str) -> OptimizedProgram {
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    OptimizedProgram::compile(&program)
}

/// 失敗メッセージ用に、長いソースを省略して表す。
fn describe(source: &str) -> String {
    const SHOWN: usize = 40;
    if source.len() <= SHOWN {
        format!("{source:?}")
    } else {
        format!("{:?}...(全{}バイト)", &source[..SHOWN], source.len())
    }
}

/// 2 つの完全観測値が全項目で一致することを確かめる。テープは不一致位置を示す。
fn assert_same(actual: &Observed, expected: &Observed, ctx: &str) {
    assert_eq!(actual.result, expected.result, "{ctx}: 結果");
    assert_eq!(actual.pc, expected.pc, "{ctx}: pc");
    assert_eq!(actual.data_pointer, expected.data_pointer, "{ctx}: dp");
    assert_eq!(
        actual.steps_executed, expected.steps_executed,
        "{ctx}: 素朴換算の実行数"
    );
    assert_eq!(
        actual.input_position, expected.input_position,
        "{ctx}: 入力位置"
    );
    assert_eq!(actual.output, expected.output, "{ctx}: 出力");
    assert_eq!(actual.tape.len(), expected.tape.len(), "{ctx}: テープ長");
    if let Some(i) = actual
        .tape
        .iter()
        .zip(&expected.tape)
        .position(|(a, b)| a != b)
    {
        panic!("{ctx}: テープ {i} 番目が不一致");
    }
}

/// 直線的なプログラムを 1 回実行し、素朴実装と完全一致することと、離脱の対応を確かめる。
///
/// 線形の操作は、上限・範囲外のいずれかのエラーに当たったときだけ離脱する。
/// 停止した実行は離脱せず、ホスト再生の命令数は 0 である。
fn check_linear(wasm: &WasmProgram, source: &str, input: &[u8], limit: u64) -> WasmRun {
    let ctx = format!("ソース {} 入力 {input:?} 上限 {limit}", describe(source));
    let run = wasm.run(input, limit).expect("方式の失敗は起きない");
    let expected = core_observe(source, input, Some(limit));
    assert_same(&wasm_observe(&run), &expected, &ctx);
    assert_eq!(
        run.bailed_out,
        expected.result.is_err(),
        "{ctx}: 離脱の有無はエラーの有無と一致する"
    );
    if !run.bailed_out {
        assert_eq!(
            run.replayed_steps, 0,
            "{ctx}: 離脱なしならホスト再生は 0 命令"
        );
    }
    run
}

/// 上限 0 から「必要数 + 2」までの全上限で、素朴実装と完全一致することを確かめる。
fn sweep_all_limits(source: &str, input: &[u8]) {
    let wasm = compile(source);
    let needed = core_observe(source, input, None).steps_executed;
    for limit in 0..=needed + 2 {
        check_linear(&wasm, source, input, limit);
    }
}

/// 最適化後の各操作の素朴換算コストの総和(開始セル値と走査回数に依存しない操作のみを対象とする)。
fn naive_total(source: &str) -> u64 {
    optimized(source)
        .ops()
        .iter()
        .map(|op| op.naive_cost(0, 0))
        .sum()
}

/// 移動を伴う操作が、位置 `dp` で範囲外に出るか(上流の到達範囲ヘルパの条件)。
fn expected_bail_by_reach(op: &Op, dp: usize) -> bool {
    let (min, max) = op.static_reach().expect("移動を伴う操作は到達範囲を持つ");
    let dp = dp as isize;
    dp + min < 0 || dp + max > LAST_CELL
}

/// 正なら `>` を、負なら `<` を `|n|` 個並べたソース。
fn move_source(n: isize) -> String {
    if n >= 0 {
        ">".repeat(n as usize)
    } else {
        "<".repeat(n.unsigned_abs())
    }
}

#[test]
fn linear_programs_match_core_across_every_limit() {
    let cases: &[(&str, &[u8])] = &[
        ("", &[]),
        ("+++.", &[]),
        ("-.", &[]),
        ("--.+.", &[]),
        (",.", &[]),
        (",.", &[0]),
        (",.", &[255]),
        (",+.", &[0xFF]),
        (",+.", &[0]),
        (",,,.", &[1, 2]),
        (",>,<.>.", &[7, 200]),
        (">+<.", &[]),
        ("+>+<>.", &[]),
        ("+>>>--<<.", &[]),
        ("+++.>>.<<<-.", &[]),
        ("<+.", &[]),
        ("<", &[]),
        (">", &[]),
    ];
    for (source, input) in cases {
        sweep_all_limits(source, input);
    }
}

#[test]
fn empty_program_halts_at_every_limit_including_zero() {
    let wasm = compile("");
    let run = check_linear(&wasm, "", &[], 0);
    assert_eq!(run.result, Ok(StopReason::Halted));
    assert!(!run.bailed_out);
    assert_eq!(run.replayed_steps, 0);
}

#[test]
fn input_consumed_before_a_limit_bailout_keeps_its_position() {
    // `,+++` を上限 2 で実行: 読取後の加算で上限に当たり、読取位置は 1 のまま残る。
    let source = ",+++";
    let wasm = compile(source);
    let run = check_linear(&wasm, source, &[9], 2);
    assert!(run.bailed_out, "上限で離脱する");
    assert_eq!(run.input_position, 1, "消費した入力は戻らない");
    assert_eq!(run.tape[0], 10, "離脱前に実行した加算は反映される");
    assert_eq!(run.steps_executed, 2);
    assert_eq!(run.replayed_steps, 1, "残りの 1 命令だけをホスト再生する");
}

#[test]
fn full_run_costs_match_the_upstream_naive_cost_helper() {
    let cases = ["+++.", "+++++>>>--<<.", ",+.,+.", ">+<-.", "-->++>."];
    for source in cases {
        let wasm = compile(source);
        let total = naive_total(source);
        assert_eq!(
            total,
            source.len() as u64,
            "線形の操作は素朴換算で命令数と一致する: {}",
            describe(source)
        );

        // 上限ちょうどで停止し、生成コードの数えた実行数はコストヘルパの総和と一致する。
        let run = check_linear(&wasm, source, &[3, 4], total);
        assert_eq!(run.result, Ok(StopReason::Halted));
        assert_eq!(run.steps_executed, total, "{}", describe(source));
        assert!(!run.bailed_out);

        // 上限が 1 少ないと、最後の操作の手前で離脱する。
        let short = check_linear(&wasm, source, &[3, 4], total - 1);
        assert!(short.bailed_out, "{}", describe(source));
        assert!(short.steps_executed < total);
    }
}

#[test]
fn halted_run_within_limits_does_not_bail_and_replays_nothing() {
    let source = "+>++<.";
    let wasm = compile(source);
    let run = wasm.run(&[], 1_000).expect("実行できる");
    assert_eq!(run.result, Ok(StopReason::Halted));
    assert_eq!(run.output, vec![1]);
    assert!(!run.bailed_out, "離脱なしの結果は離脱の有無が偽");
    assert_eq!(run.replayed_steps, 0, "離脱なしならホスト再生の命令数は 0");
}

#[test]
fn move_bailout_matches_static_reach_helper_at_both_edges() {
    // 開始位置を左右の端近くに置き、その位置から各移動を行う。
    // 離脱の有無は到達範囲ヘルパの条件と一致し、かつ素朴実装と完全一致する。
    let starts = [0usize, 1, 29_990, 29_998, 29_999];
    let moves = [-10isize, -1, 1, 9, 10];
    for &start in &starts {
        for &n in &moves {
            // `>` * start で位置を作り、加算で移動を区切ってから移動する。
            let source = format!("{}+{}", move_source(start as isize), move_source(n));
            let program = optimized(&source);
            let last = program.ops().last().expect("操作がある");
            assert!(
                matches!(last.kind(), brainfuck_optimized::OpKind::Move(m) if *m == n),
                "末尾は移動操作"
            );
            let expected_bail = expected_bail_by_reach(last, start);

            let wasm = compile(&source);
            let run = check_linear(&wasm, &source, &[], 1_000_000);
            assert_eq!(
                run.bailed_out, expected_bail,
                "開始 {start}・移動 {n}: 離脱の有無は到達範囲の条件と一致する"
            );
            if expected_bail {
                // 素朴実装は 1 歩ずつ進んで端で止まるため、離脱後の位置は端になる(素朴と同一)。
                let edge = if n < 0 { 0 } else { LAST_CELL as usize };
                assert_eq!(run.data_pointer, edge, "範囲外は端で止まる(素朴と同一)");
            } else {
                assert_eq!(
                    run.data_pointer as isize,
                    start as isize + n,
                    "範囲内では移動する"
                );
            }
        }
    }
}

#[test]
fn right_edge_long_prefix_matches_core_at_the_boundary() {
    // 29,999 まで進んで最後のセルに届く/1 つ越えるプログラム。長いため上限は代表値だけを試す。
    let reaches_last = format!("{}+.", ">".repeat(29_999));
    let overflows = format!("{}+.", ">".repeat(30_000));
    let wasm_ok = compile(&reaches_last);
    let wasm_over = compile(&overflows);
    for limit in [0u64, 1, 2, 29_999, 30_000, 30_001, 1_000_000] {
        check_linear(&wasm_ok, &reaches_last, &[], limit);
        check_linear(&wasm_over, &overflows, &[], limit);
    }
    let ok = wasm_ok.run(&[], 1_000_000).expect("実行できる");
    assert_eq!(ok.result, Ok(StopReason::Halted));
    assert_eq!(ok.data_pointer, 29_999);
    assert_eq!(ok.output, vec![1]);
    let over = wasm_over.run(&[], 1_000_000).expect("実行できる");
    assert!(over.bailed_out, "最後のセルを越える移動は離脱する");
    assert_eq!(over.data_pointer, 29_999);
}

#[test]
fn hello_sample_matches_core_across_every_limit() {
    // ワークスペース共通のサンプル。ループを含むため離脱の有無は問わず、結果と状態の一致だけを確かめる。
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hello.bf");
    let source = std::fs::read_to_string(path).expect("共通サンプル examples/hello.bf を読める");
    let wasm = compile(&source);
    let needed = core_observe(&source, &[], None).steps_executed;
    let expected_full = core_observe(&source, &[], None);
    assert_eq!(expected_full.result, Ok(StopReason::Halted));
    for limit in 0..=needed + 2 {
        let run = wasm.run(&[], limit).expect("方式の失敗は起きない");
        let expected = core_observe(&source, &[], Some(limit));
        assert_same(
            &wasm_observe(&run),
            &expected,
            &format!("hello 上限 {limit}"),
        );
    }
}
