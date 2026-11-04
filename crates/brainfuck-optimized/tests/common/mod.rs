//! 差分比較の共通基盤。素朴実装(core の `Machine`)をオラクルとして、最適化機械と比較する。
//!
//! 各テストファイルから `mod common;` で読み込む。テストファイルごとに使うヘルパが異なるため、
//! 未使用コードによる警告はこのモジュール全体で抑止する。
//!
//! 比較する項目は、停止/エラーの結果、`pc`・データポインタ・現在セル・実行数(素朴換算)・停止フラグ、
//! テープ全体、出力、入力読取位置である。加えて、最適化機械の IR 操作の実行回数が素朴換算の実行数以下であることを検査する。
//!
//! 停止しないプログラムを上限なしで実行すると戻らないため、上限なしで比較するのは停止が確実なプログラムに限る。

#![allow(dead_code)]

use brainfuck_core::{Machine, MachineConfig, Program, RuntimeError, StopReason};
use brainfuck_optimized::{OptimizedMachine, OptimizedProgram};

/// 掃引の既定の打ち切り上限。停止しないプログラムの掃引範囲を有限にするための安全上限。
pub const DEFAULT_CAP: u64 = 256;

/// 1 回の実行で観測できる項目の組(core と最適化機械で共通の形)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// 完走実行の結果(停止理由またはエラー)。
    pub result: Result<StopReason, RuntimeError>,
    /// 命令添字。
    pub pc: usize,
    /// データポインタ。
    pub data_pointer: usize,
    /// 現在のセルの値。
    pub current_cell: u8,
    /// 素朴換算の実行数。
    pub steps_executed: u64,
    /// 停止フラグ。
    pub halted: bool,
    /// テープ全体。
    pub tape: Vec<u8>,
    /// 出力全体。
    pub output: Vec<u8>,
    /// 読み取り済みの入力バイト数。
    pub input_position: usize,
}

/// core の `Machine` を完走実行し、観測結果を返す(オラクル側)。
pub fn observe_core(source: &str, input: &[u8], config: MachineConfig) -> Observation {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    let mut machine = Machine::new(program, input.to_vec(), config);
    let result = machine.run();
    let snapshot = machine.snapshot();
    Observation {
        result,
        pc: snapshot.pc,
        data_pointer: snapshot.data_pointer,
        current_cell: snapshot.current_cell,
        steps_executed: snapshot.steps_executed,
        halted: snapshot.halted,
        tape: machine.tape().to_vec(),
        output: machine.output().to_vec(),
        input_position: machine.input_position(),
    }
}

/// 最適化機械を完走実行し、観測結果と IR 操作の実行回数を返す(検査対象側)。
pub fn observe_optimized(source: &str, input: &[u8], config: MachineConfig) -> (Observation, u64) {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    let optimized = OptimizedProgram::compile(&program);
    let mut machine = OptimizedMachine::new(&optimized, input.to_vec(), config);
    let result = machine.run();
    let snapshot = machine.snapshot();
    let observation = Observation {
        result,
        pc: snapshot.pc,
        data_pointer: snapshot.data_pointer,
        current_cell: snapshot.current_cell,
        steps_executed: snapshot.steps_executed,
        halted: snapshot.halted,
        tape: machine.tape().to_vec(),
        output: machine.output().to_vec(),
        input_position: machine.input_position(),
    };
    (observation, snapshot.ir_ops_executed)
}

/// 収集済みの 2 つの観測結果を比較し、不一致の項目名と値を `"; "` 区切りの文字列で返す。
///
/// 一致する場合は空文字列を返す。テープは最初の不一致位置の値のみを示す。
pub fn compare_observations(core: &Observation, optimized: &Observation) -> String {
    let mut mismatches: Vec<String> = Vec::new();
    if core.result != optimized.result {
        mismatches.push(format!(
            "result: core={:?}, optimized={:?}",
            core.result, optimized.result
        ));
    }
    if core.pc != optimized.pc {
        mismatches.push(format!("pc: core={}, optimized={}", core.pc, optimized.pc));
    }
    if core.data_pointer != optimized.data_pointer {
        mismatches.push(format!(
            "data pointer: core={}, optimized={}",
            core.data_pointer, optimized.data_pointer
        ));
    }
    if core.current_cell != optimized.current_cell {
        mismatches.push(format!(
            "current cell: core={}, optimized={}",
            core.current_cell, optimized.current_cell
        ));
    }
    if core.steps_executed != optimized.steps_executed {
        mismatches.push(format!(
            "steps executed: core={}, optimized={}",
            core.steps_executed, optimized.steps_executed
        ));
    }
    if core.halted != optimized.halted {
        mismatches.push(format!(
            "halted: core={}, optimized={}",
            core.halted, optimized.halted
        ));
    }
    if core.tape != optimized.tape {
        mismatches.push(match first_tape_difference(&core.tape, &optimized.tape) {
            Some((index, expected, actual)) => format!(
                "tape: first difference at index {index}: core={expected}, optimized={actual}"
            ),
            None => format!(
                "tape: length core={}, optimized={}",
                core.tape.len(),
                optimized.tape.len()
            ),
        });
    }
    if core.output != optimized.output {
        mismatches.push(format!(
            "output: core={:?}, optimized={:?}",
            core.output, optimized.output
        ));
    }
    if core.input_position != optimized.input_position {
        mismatches.push(format!(
            "input position: core={}, optimized={}",
            core.input_position, optimized.input_position
        ));
    }
    mismatches.join("; ")
}

/// 2 つのテープの最初の不一致位置(位置、core の値、最適化の値)を返す。
fn first_tape_difference(core: &[u8], optimized: &[u8]) -> Option<(usize, u8, u8)> {
    core.iter()
        .zip(optimized.iter())
        .enumerate()
        .find(|(_, (a, b))| a != b)
        .map(|(index, (&a, &b))| (index, a, b))
}

/// 同一のソース・入力・設定で core と最適化機械を実行して比較する。不一致なら panic する。
///
/// 不一致時は、ソース・入力・設定と不一致項目を失敗メッセージに含める。
/// 最適化機械の IR 操作の実行回数が素朴換算の実行数以下であることも検査する。
/// 上限なしの設定では、停止しないプログラムに対して戻らないため呼び出し側が注意する。
pub fn assert_equivalent(source: &str, input: &[u8], config: MachineConfig) {
    let core = observe_core(source, input, config);
    let (optimized, ir_ops) = observe_optimized(source, input, config);
    let label = format!("source {source:?}, input {input:?}, config {config:?}");
    assert_observations_equivalent(&label, &core, &optimized, ir_ops);
}

/// 収集済みの観測結果の組を検査する。不一致または IR 操作の実行回数の超過があれば、`label` を含めて panic する。
///
/// [`assert_equivalent`] の検査部分。収集を伴わないため、検出の負のテストに用いる。
pub fn assert_observations_equivalent(
    label: &str,
    core: &Observation,
    optimized: &Observation,
    ir_ops: u64,
) {
    let diff = compare_observations(core, optimized);
    assert!(
        diff.is_empty(),
        "optimized differs from core ({label}): {diff}"
    );
    assert!(
        ir_ops <= optimized.steps_executed,
        "IR operations ({ir_ops}) exceed naive steps ({}) ({label})",
        optimized.steps_executed
    );
}

/// 上限を 0 から掃引して比較する。
///
/// まず core を打ち切り上限 `cap`(省略時は [`DEFAULT_CAP`])で実行する。停止する場合は必要実行数 `N` を求め、
/// 上限 0 から `N + 2` まで比較する。停止しない場合(上限超過・エラー)は上限 0 から `cap + 2` まで比較する。
pub fn assert_equivalent_across_limits(source: &str, input: &[u8], cap: Option<u64>) {
    let cap = cap.unwrap_or(DEFAULT_CAP);
    let probe = observe_core(source, input, MachineConfig::with_max_steps(cap));
    let upper = match probe.result {
        Ok(StopReason::Halted) => probe.steps_executed + 2,
        _ => cap + 2,
    };
    for limit in 0..=upper {
        assert_equivalent(source, input, MachineConfig::with_max_steps(limit));
    }
}

/// 使用可能な記号の列から長さ `0..=max_len` の全プログラムを深さ優先順で列挙する。
///
/// `balanced` が真のときは、括弧 `[` `]` が対応するプログラムのみを返す(接頭辞の深さが負になる枝は刈る)。
/// 記号は ASCII のみを想定する。
pub fn enumerate_programs(alphabet: &[u8], max_len: usize, balanced: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    extend_programs(alphabet, max_len, balanced, 0, &mut current, &mut out);
    out
}

fn extend_programs(
    alphabet: &[u8],
    max_len: usize,
    balanced: bool,
    depth: isize,
    current: &mut Vec<u8>,
    out: &mut Vec<String>,
) {
    if !balanced || depth == 0 {
        out.push(String::from_utf8(current.clone()).expect("ASCII alphabet"));
    }
    if current.len() == max_len {
        return;
    }
    for &byte in alphabet {
        let next_depth = match byte {
            b'[' => depth + 1,
            b']' => depth - 1,
            _ => depth,
        };
        if balanced && next_depth < 0 {
            continue;
        }
        current.push(byte);
        extend_programs(alphabet, max_len, balanced, next_depth, current, out);
        current.pop();
    }
}

/// 固定シードで再現できる擬似乱数(xorshift64)。
#[derive(Debug, Clone)]
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    /// シードから生成する。シードが 0 のときは既定の非 0 値を用いる(xorshift は 0 から進まないため)。
    pub fn new(seed: u64) -> XorShift64 {
        XorShift64 {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// 次の 64 ビット値を返す。
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// `0..bound` の一様に近い値を返す。`bound` は 1 以上でなければならない。
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}
