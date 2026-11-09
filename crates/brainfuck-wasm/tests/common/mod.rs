//! 差分テストの共通ヘルパ(`tests/emit_loops.rs` などが共有する)。
//!
//! 素朴実装(`brainfuck-core` の `Machine`)を正解とし、WASM 実行の完全観測値と全項目で比べる。
//! `linear_diff.rs` の補助と同じ規約だが、既存テストを変更しないためにここへ複製している。

#![allow(dead_code)]

use brainfuck_core::{Machine, MachineConfig, Program, RuntimeError, StopReason, TAPE_SIZE};
use brainfuck_optimized::OptimizedProgram;
use brainfuck_wasm::{WasmProgram, WasmRun};

/// 素朴実装の完全観測値。
#[derive(Debug, PartialEq)]
pub struct Observed {
    pub result: Result<StopReason, RuntimeError>,
    pub pc: usize,
    pub data_pointer: usize,
    pub steps_executed: u64,
    pub input_position: usize,
    pub output: Vec<u8>,
    pub tape: Vec<u8>,
}

/// 素朴実装を実行して完全観測値を得る(オラクル側)。`limit` が `None` なら上限なし。
pub fn core_observe(source: &str, input: &[u8], limit: Option<u64>) -> Observed {
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
pub fn wasm_observe(run: &WasmRun) -> Observed {
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
pub fn compile(source: &str) -> WasmProgram {
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    let ir = OptimizedProgram::compile(&program);
    WasmProgram::compile(&ir).expect("テスト用プログラムはコンパイルできる")
}

/// 失敗メッセージ用に、長いソースを省略して表す。
pub fn describe(source: &str) -> String {
    const SHOWN: usize = 40;
    if source.len() <= SHOWN {
        format!("{source:?}")
    } else {
        format!("{:?}...(全{}バイト)", &source[..SHOWN], source.len())
    }
}

/// 2 つの完全観測値が全項目で一致することを確かめる。テープは不一致位置を示す。
pub fn assert_same(actual: &Observed, expected: &Observed, ctx: &str) {
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
    assert_eq!(actual.tape.len(), TAPE_SIZE, "{ctx}: テープ長");
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

/// 1 回実行し、素朴実装と完全一致すること、エラーになる実行は必ず離脱していること、
/// 離脱なしならホスト再生が 0 命令であることを確かめる。
///
/// 未実装の種別(クリア・移動ループ・走査、2.7・2.8 まで)は常に離脱するため、
/// 停止する実行が離脱しうる。そのため「離脱 ⇒ エラー」は要求しない。
pub fn check_run(wasm: &WasmProgram, source: &str, input: &[u8], limit: u64) -> WasmRun {
    let ctx = format!("ソース {} 入力 {input:?} 上限 {limit}", describe(source));
    let run = wasm.run(input, limit).expect("方式の失敗は起きない");
    let expected = core_observe(source, input, Some(limit));
    assert_same(&wasm_observe(&run), &expected, &ctx);
    if expected.result.is_err() {
        assert!(run.bailed_out, "{ctx}: エラーになる実行は離脱している");
    }
    if !run.bailed_out {
        assert_eq!(
            run.replayed_steps, 0,
            "{ctx}: 離脱なしならホスト再生は 0 命令"
        );
    }
    run
}

/// 上限 0 から「必要数 + 2」までの全上限で、素朴実装と完全一致することを確かめる。
/// 素朴実装が停止するプログラムにのみ使う(無限ループは対象外)。
pub fn sweep_all_limits(source: &str, input: &[u8]) {
    let wasm = compile(source);
    let needed = core_observe(source, input, None).steps_executed;
    for limit in 0..=needed + 2 {
        check_run(&wasm, source, input, limit);
    }
}
