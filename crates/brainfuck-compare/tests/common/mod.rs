//! 差分テストの共通基盤(`tests/diff_*.rs` と `tests/regression.rs` が共有する)。
//!
//! 素朴インタプリタ(`Method::Naive`)を基準とし、方式の完全観測値(`Observation`)の差分が
//! 空であることを検査する。共有するのは以下の公開ヘルパのみで、パターンテンプレートや
//! 乱数プログラム生成器は各テストファイル内に置く(共有編集の衝突を避けるため)。
//!
//! 比較の本体は実行部から分離してあり、テストは壊した観測を注入して検出を確かめられる。
//!
//! - `run_pair(source, input, limit, method) -> Execution`: 素朴と方式を同一条件で実行し、
//!   差分が空であることを検査して方式の実行結果を返す。不一致時は panic し、メッセージに
//!   ソース・入力・上限・不一致項目(`Field` の名前)を含める。
//! - `run_pair_with(source, input, limit, method, run) -> Execution`: `run_pair` と同じだが、
//!   方式の実行を `run` で差し替える(壊した観測を注入するテスト用)。
//! - `pair_diff(source, input, limit, method) -> Vec<Field>` /
//!   `pair_diff_with(source, input, limit, method, run) -> Vec<Field>`: 差分の項目だけを返す
//!   (panic しない。`run_pair` の検査と同じ判定)。
//! - `judge(oracle, observed) -> Vec<Field>`: 基準と方式の観測の差分(判定の純関数)。
//! - `real_runner(program, input, limit, method)`: 実際の方式実行(`run_pair` が使う既定の `run`)。
//! - `needed_steps(source, input) -> u64`: 上限なしの素朴実行の必要数 N(停止するプログラム専用)。
//! - `sweep_range(needed) -> RangeInclusive<u64>`: 掃引の上限範囲 `0..=N+2`。
//! - `sweep_limits(source, input, method) -> u64`: `sweep_range` の全上限を `run_pair` で比較し、
//!   比較した回数を返す。
//! - `enumerate_balanced(alphabet, max_len) -> Vec<String>`: 括弧が対応する全プログラムを
//!   長さ 0 から `max_len` まで列挙する(列挙順は決定的)。
//! - `XorShift64::new(seed)`, `next_u64()`, `below(n)`: 固定シードの擬似乱数(xorshift64)。
//! - `hello_source()`: ワークスペース共通のサンプル `examples/hello.bf`(コピーは置かない)。
//! - `describe(source)`: 失敗メッセージ用に、長いソースを省略して表す。
//!
//! 命令数上限は `u64`。`run_pair` は 0 を含む任意の上限を受け付ける。

#![allow(dead_code)]

use std::ops::RangeInclusive;

use brainfuck_compare::methods::{run_method, run_naive, Execution, MethodFailure};
use brainfuck_compare::{Field, Method, Observation};
use brainfuck_core::Program;

/// 素朴と方式を同一条件で実行して差分を検査し、方式の実行結果を返す。差分があれば panic する。
pub fn run_pair(source: &str, input: &[u8], limit: u64, method: Method) -> Execution {
    run_pair_with(source, input, limit, method, real_runner)
}

/// `run_pair` と同じ検査を、方式の実行 `run` を差し替えて行う。
pub fn run_pair_with<F>(source: &str, input: &[u8], limit: u64, method: Method, run: F) -> Execution
where
    F: Fn(&Program, &[u8], u64, Method) -> Result<Execution, MethodFailure>,
{
    let (execution, diff) = execute_pair(source, input, limit, method, &run);
    assert!(
        diff.is_empty(),
        "{}: 素朴と {method:?} の不一致 {diff:?}",
        context(source, input, limit)
    );
    execution
}

/// 素朴と方式の差分の項目を返す。完全一致なら空。
pub fn pair_diff(source: &str, input: &[u8], limit: u64, method: Method) -> Vec<Field> {
    pair_diff_with(source, input, limit, method, real_runner)
}

/// `pair_diff` と同じ判定を、方式の実行 `run` を差し替えて行う。
pub fn pair_diff_with<F>(
    source: &str,
    input: &[u8],
    limit: u64,
    method: Method,
    run: F,
) -> Vec<Field>
where
    F: Fn(&Program, &[u8], u64, Method) -> Result<Execution, MethodFailure>,
{
    execute_pair(source, input, limit, method, &run).1
}

/// 基準と方式の観測の差分の項目。完全一致なら空。
pub fn judge(oracle: &Observation, observed: &Observation) -> Vec<Field> {
    observed.diff(oracle)
}

/// 既定の方式実行(`brainfuck_compare::methods::run_method`)。
pub fn real_runner(
    program: &Program,
    input: &[u8],
    limit: u64,
    method: Method,
) -> Result<Execution, MethodFailure> {
    run_method(method, program, input, limit)
}

/// 上限なしの素朴実行の必要数 N。停止しないプログラムには使わない。
pub fn needed_steps(source: &str, input: &[u8]) -> u64 {
    let program = parse(source);
    let execution = run_naive(&program, input, u64::MAX).expect("素朴は失敗しない");
    assert!(
        execution.observation.halted(),
        "{}: 必要数を求めるプログラムは停止する必要がある",
        describe(source)
    );
    execution.observation.steps_executed
}

/// 掃引する上限の範囲。必要数 `needed` に対して `0..=needed + 2`。
pub fn sweep_range(needed: u64) -> RangeInclusive<u64> {
    0..=needed + 2
}

/// `sweep_range` の全上限を `run_pair` で比較し、比較した回数を返す。停止するプログラム専用。
pub fn sweep_limits(source: &str, input: &[u8], method: Method) -> u64 {
    let needed = needed_steps(source, input);
    let mut compared = 0;
    for limit in sweep_range(needed) {
        run_pair(source, input, limit, method);
        compared += 1;
    }
    compared
}

/// 括弧が対応する全プログラムを長さ 0 から `max_len` まで列挙する。
///
/// 列挙は長さ順、同じ長さの中では `alphabet` の添字の辞書順で決定的に行う。
pub fn enumerate_balanced(alphabet: &[u8], max_len: usize) -> Vec<String> {
    let base = alphabet.len();
    let mut out = Vec::new();
    for len in 0..=max_len {
        let total = base.pow(u32::try_from(len).expect("長さは小さい"));
        for code in 0..total {
            let mut rest = code;
            let mut bytes = Vec::with_capacity(len);
            for _ in 0..len {
                bytes.push(alphabet[rest % base]);
                rest /= base;
            }
            if is_balanced(&bytes) {
                out.push(String::from_utf8(bytes).expect("括弧の文字は ASCII"));
            }
        }
    }
    out
}

fn is_balanced(bytes: &[u8]) -> bool {
    let mut depth: i64 = 0;
    for &byte in bytes {
        match byte {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// 固定シードの擬似乱数(xorshift64)。
pub struct XorShift64(u64);

impl XorShift64 {
    /// シードから生成する。0 は内部で非 0 の定数に置き換える(xorshift は 0 から進まないため)。
    pub fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    /// 次の 64 ビット値。
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// `0..n` の値。`n` は 1 以上。
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n >= 1, "below の n は 1 以上");
        self.next_u64() % n
    }
}

/// ワークスペース共通のサンプル `examples/hello.bf` の内容。
pub fn hello_source() -> &'static str {
    include_str!("../../../../examples/hello.bf")
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

fn parse(source: &str) -> Program {
    Program::parse(source).expect("テスト用ソースは解析できる")
}

fn context(source: &str, input: &[u8], limit: u64) -> String {
    format!("ソース {} 入力 {input:?} 上限 {limit}", describe(source))
}

/// 基準と方式を実行し、方式の実行結果と差分の項目を返す。
fn execute_pair<F>(
    source: &str,
    input: &[u8],
    limit: u64,
    method: Method,
    run: &F,
) -> (Execution, Vec<Field>)
where
    F: Fn(&Program, &[u8], u64, Method) -> Result<Execution, MethodFailure>,
{
    let program = parse(source);
    let oracle = run_naive(&program, input, limit)
        .expect("素朴は失敗しない")
        .observation;
    let execution = run(&program, input, limit, method).unwrap_or_else(|failure| {
        panic!(
            "{}: {method:?} が完走しなかった: {failure:?}",
            context(source, input, limit)
        )
    });
    let diff = judge(&oracle, &execution.observation);
    (execution, diff)
}
