//! 小規模プログラムの網羅比較。素朴実装(core)をオラクルとして、最適化機械と全数比較する。
//!
//! - 全 8 命令(`><+-.,[]`)の長さ 5 以下のプログラムのうち、括弧が対応するもの
//!   (対応しない括弧は解析で拒否されるため、比較の対象にならない)
//! - 命令 `+-<>[]` の括弧が対応する長さ 8 以下の全プログラム
//!
//! 各プログラムは接頭辞(空、`++`、`+++>++<`、`-`)の後ろに置き、ループが実際に反復するようにする。
//! 比較は上限を 0 から掃引して行い、固定入力と小さな安全上限(停止しないプログラムの打ち切り)を用いる。

mod common;

use common::{assert_equivalent_across_limits, enumerate_programs};

/// 接頭辞。ループが実際に反復する状態(セル値 0 以外、ポインタ位置の違い)を作る。
const PREFIXES: [&str; 4] = ["", "++", "+++>++<", "-"];

/// 固定入力。全 8 命令の列挙では `,` が入力を読むため、全件で同じ入力を用いる。
const INPUT: &[u8] = &[0x41, 0x00, 0xFF];

/// 停止しないプログラムを打ち切るための小さな安全上限。
const CAP: u64 = 64;

/// 指定の列挙結果をすべての接頭辞の後ろに置いて比較する。
fn compare_all(programs: &[String]) {
    for body in programs {
        for prefix in PREFIXES {
            let source = format!("{prefix}{body}");
            assert_equivalent_across_limits(&source, INPUT, Some(CAP));
        }
    }
}

/// 全 8 命令の長さ 5 以下の、括弧が対応するプログラムを比較する。
#[test]
fn all_eight_command_programs_up_to_length_five_match_the_core_machine() {
    let programs = enumerate_programs(b"><+-.,[]", 5, true);
    let expected: usize = balanced_counts_by_dynamic_programming(5, 6).iter().sum();
    assert_eq!(programs.len(), expected);
    compare_all(&programs);
}

/// 括弧が対応する `+-<>[]` の長さ 6 以下の全プログラムを比較する。
///
/// 長さ 8 まで(約 27 万本 × 接頭辞 4)はデバッグビルドでは 10 分超で完走せず、release ビルドで約 5 分かかるため、
/// 全体を 1 分程度に収めるために長さ 6 に限る。長さ 8 までの個数は [`BALANCED_COUNTS_UP_TO_EIGHT`] で固定している。
#[test]
fn bracket_balanced_programs_up_to_length_six_match_the_core_machine() {
    let programs = enumerate_programs(b"+-<>[]", 6, true);
    compare_all(&programs);
}

/// 括弧が対応する `+-<>[]` の長さ 8 以下の全プログラムを比較する(重い全数比較のため既定では無視する)。
///
/// 通常の `cargo test` では実行されない。次のコマンドで明示的に実行する:
/// `cargo test --release -p brainfuck-optimized --test diff_exhaustive -- --ignored`
#[test]
#[ignore = "debug ビルドでは 10 分超で完走しない。cargo test --release -p brainfuck-optimized --test diff_exhaustive -- --ignored で約 5 分(release)"]
fn bracket_balanced_programs_up_to_length_eight_match_the_core_machine() {
    let programs = enumerate_programs(b"+-<>[]", 8, true);
    compare_all(&programs);
}

/// 長さ 0..=8 の括弧が対応する `+-<>[]` のプログラムの個数(長さごと)。列挙範囲が将来ずれないよう固定する。
///
/// 列挙器とは独立に求めた値(下記の動的計画法と一致することも確認する)。
const BALANCED_COUNTS_UP_TO_EIGHT: [usize; 9] = [1, 4, 17, 76, 354, 1704, 8421, 42508, 218318];

/// 括弧が対応するプログラムを、長さごとに動的計画法で数える(列挙器とは別の算出)。
///
/// `neutral` は括弧以外の記号の種類数。戻り値の添字は長さ(0..=max_len)。
fn balanced_counts_by_dynamic_programming(max_len: usize, neutral: usize) -> Vec<usize> {
    let mut ways = vec![0usize; max_len + 2];
    ways[0] = 1;
    let mut counts = vec![ways[0]];
    for _ in 1..=max_len {
        let mut next = vec![0usize; max_len + 2];
        for (depth, &w) in ways.iter().enumerate() {
            if w == 0 {
                continue;
            }
            next[depth] += neutral * w;
            next[depth + 1] += w;
            if depth > 0 {
                next[depth - 1] += w;
            }
        }
        ways = next;
        counts.push(ways[0]);
    }
    counts
}

/// 列挙器の出力を長さごとに数え、動的計画法と固定値の両方に一致することを確認する。
#[test]
fn balanced_enumeration_counts_per_length_are_pinned() {
    let programs = enumerate_programs(b"+-<>[]", 8, true);
    let mut counts = vec![0usize; 9];
    for program in &programs {
        counts[program.len()] += 1;
    }
    assert_eq!(
        counts.to_vec(),
        balanced_counts_by_dynamic_programming(8, 4)
    );
    assert_eq!(counts, BALANCED_COUNTS_UP_TO_EIGHT.to_vec());
    assert_eq!(programs.len(), 271_403);
}
