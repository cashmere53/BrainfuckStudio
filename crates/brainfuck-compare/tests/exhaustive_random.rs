//! 網羅・乱数の差分テスト(13.1, 13.2, 13.3, 13.11): WASM 方式を素朴と全数・固定シードで比較する。
//!
//! - 網羅: 全 8 文字・長さ 4 以下の列と、`+-<>[]` の括弧対応プログラム(長さ 6 以下)を、
//!   接頭辞(なし、`+++>++<`)の後ろに置いて小さな安全上限と固定入力で比較する。
//! - 乱数: 固定シードの括弧対応プログラム(長さ 5〜80、加減算・移動を高め配分)を 1,000 本比較する。
//!   失敗メッセージにはシードとソースを含める。

mod common;

use std::panic::{catch_unwind, AssertUnwindSafe};

use brainfuck_compare::Method;
use brainfuck_core::Program;
use common::{describe, enumerate_balanced, run_pair, XorShift64};

/// 網羅の全 8 文字(括弧を含む)。
const ALL_EIGHT: &[u8] = b"+-<>[],.";
/// 網羅の全 8 文字の最大長。
const EXHAUSTIVE_MAX_LEN: usize = 4;
/// 括弧対応の網羅の文字(括弧と加減算・移動)と最大長。
const BALANCED_ALPHABET: &[u8] = b"+-<>[]";
const BALANCED_MAX_LEN: usize = 6;
/// 接頭辞。空と、正味の移動・加算を含む短い列。
const PREFIXES: [&str; 2] = ["", "+++>++<"];
/// 網羅の安全上限(設計書の値)。
const EXHAUSTIVE_LIMIT: u64 = 512;
/// 乱数の安全上限。ループが止まらないプログラムも上限で打ち切って比較する。
const RANDOM_LIMIT: u64 = 20_000;
/// 両テスト共通の固定入力(`,` が読む 2 バイト。`0x00` と `0x05` を含む)。
const INPUT: &[u8] = b"\x05\x00";
/// 乱数プログラムの本数と長さ範囲。
const RANDOM_COUNT: u64 = 1_000;
const RANDOM_MIN_LEN: usize = 5;
const RANDOM_MAX_LEN: usize = 80;

/// 長さ 0 から `max_len` までの全列(括弧の対応は問わない)。列挙順は決定的。
fn all_strings(alphabet: &[u8], max_len: usize) -> Vec<String> {
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
            out.push(String::from_utf8(bytes).expect("ASCII"));
        }
    }
    out
}

/// 接頭辞を付けて WASM を素朴と比較し、比較した回数を返す。
fn compare_with_prefixes(bodies: &[String]) -> u64 {
    let mut compared = 0;
    for body in bodies {
        for prefix in PREFIXES {
            let source = format!("{prefix}{body}");
            run_pair(&source, INPUT, EXHAUSTIVE_LIMIT, Method::Wasm);
            compared += 1;
        }
    }
    compared
}

/// 括弧の対応を問わず解析できる列(全 8 文字のうち `[` `]` の不整合を除く)。
fn parsable(strings: Vec<String>) -> Vec<String> {
    strings
        .into_iter()
        .filter(|s| Program::parse(s).is_ok())
        .collect()
}

/// 全 8 文字・長さ 4 以下の解析できる列の数。括弧の対応の数え上げ(Catalan)から独立に求めた値。
///
/// 長さ n の列のうち括弧の部品が対応するものは、括弧 k 個(k は偶数)を選ぶ組合せ C(n,k)、
/// 残りの 6 文字の並び 6^(n-k)、括弧部品の対応数 Catalan(k/2) の積の総和。
const EXPECTED_EIGHT_CHAR_PARSABLE: usize = 1 + 6 + 37 + 234 + 1514;
// 上式: 長さ 0,1,2,3,4 の順に 1, 6, 37, 234, 1514。

#[test]
fn all_eight_char_programs_up_to_length_four_match_naive_under_wasm() {
    let programs = parsable(all_strings(ALL_EIGHT, EXHAUSTIVE_MAX_LEN));
    assert_eq!(
        programs.len(),
        EXPECTED_EIGHT_CHAR_PARSABLE,
        "全 8 文字・長さ 4 以下の解析できる列の数"
    );
    let compared = compare_with_prefixes(&programs);
    assert_eq!(compared, 2 * EXPECTED_EIGHT_CHAR_PARSABLE as u64);
}

#[test]
fn balanced_programs_up_to_length_six_match_naive_under_wasm() {
    let programs = enumerate_balanced(BALANCED_ALPHABET, BALANCED_MAX_LEN);
    assert!(
        programs.len() > 1,
        "括弧対応の列挙が空でない: {}",
        programs.len()
    );
    let compared = compare_with_prefixes(&programs);
    assert_eq!(compared, 2 * programs.len() as u64);
}

/// 固定シードの括弧対応プログラム(長さ 5〜80、加減算・移動を高め配分)。
///
/// 各位置で残りの長さに対する括弧の深さの不変条件(深さ ≤ 残り)を保ち、末尾で必ず閉じる。
/// 配分: 開き括弧・閉じ括弧が各 8%、残りは `+` 30%、`-` 25%、`>` 17%、`<` 17%、`.` 5%、`,` 6%。
fn random_program(rng: &mut XorShift64) -> String {
    let len = RANDOM_MIN_LEN + rng.below((RANDOM_MAX_LEN - RANDOM_MIN_LEN + 1) as u64) as usize;
    let mut out = String::with_capacity(len);
    let mut depth = 0usize;
    for position in 0..len {
        let remaining = len - position;
        if depth == remaining {
            out.push(']');
            depth -= 1;
            continue;
        }
        let roll = rng.below(100);
        if roll < 8 && depth + 1 < remaining {
            out.push('[');
            depth += 1;
        } else if (8..16).contains(&roll) && depth > 0 {
            out.push(']');
            depth -= 1;
        } else {
            out.push(plain_instruction(rng));
        }
    }
    debug_assert_eq!(depth, 0, "括弧が閉じている");
    out
}

/// 括弧以外の命令を高め配分で選ぶ(加減算・移動を多くする)。
fn plain_instruction(rng: &mut XorShift64) -> char {
    match rng.below(100) {
        0..=29 => '+',
        30..=54 => '-',
        55..=71 => '>',
        72..=88 => '<',
        89..=93 => '.',
        _ => ',',
    }
}

#[test]
fn random_generator_yields_balanced_programs_of_the_requested_length() {
    for seed in 1..=RANDOM_COUNT {
        let mut rng = XorShift64::new(seed);
        let source = random_program(&mut rng);
        assert!(
            (RANDOM_MIN_LEN..=RANDOM_MAX_LEN).contains(&source.len()),
            "seed {seed}: 長さ {} は範囲外 {}",
            source.len(),
            describe(&source)
        );
        assert!(
            is_balanced(&source),
            "seed {seed}: 括弧が対応しない {}",
            describe(&source)
        );
    }
}

#[test]
fn random_balanced_programs_match_naive_under_wasm() {
    for seed in 1..=RANDOM_COUNT {
        let mut rng = XorShift64::new(seed);
        let source = random_program(&mut rng);
        check_with_seed(seed, &source);
    }
}

/// 比較し、失敗時はシードを先頭に付けて panic する(run_pair のメッセージにはソースが含まれる)。
fn check_with_seed(seed: u64, source: &str) {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        run_pair(source, INPUT, RANDOM_LIMIT, Method::Wasm)
    }));
    if let Err(payload) = outcome {
        let detail = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "詳細不明".to_string());
        panic!("乱数の差分テスト seed={seed}: {detail}");
    }
}

fn is_balanced(source: &str) -> bool {
    let mut depth: i64 = 0;
    for byte in source.bytes() {
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
