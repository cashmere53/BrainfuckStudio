//! 固定シード乱数プログラムの比較。素朴実装(core)をオラクルとして、最適化機械と比較する。
//!
//! 固定シード(xorshift64)から小規模プログラムを 2,000 本生成する。各プログラムは長さ 5〜80 で、
//! 括弧は対応し、加減算と移動を多めに配分する。一定確率でクリア・移動/加算/乗算・走査のテンプレート断片を挿入し、
//! 先頭付近に `+` を積む。入力は空と非空の 2 通り、上限は複数の値を用いる。
//! 失敗時はシード・プログラム番号・ソースを失敗メッセージに含め、同じシードで同じプログラム列を再生成できる。

mod common;

use brainfuck_core::{MachineConfig, Program};
use brainfuck_optimized::{Op, OpKind, OptimizedProgram};
use common::{assert_observations_equivalent, observe_core, observe_optimized, XorShift64};

/// 生成に用いる固定シード。失敗時の再現に使う。
const RANDOM_SEED: u64 = 0xB7F0_0D43_2026_0001;

/// 生成するプログラムの本数。
const PROGRAM_COUNT: usize = 2_000;

/// 生成するプログラムの最短長。
const MIN_LEN: usize = 5;

/// 生成するプログラムの最長長。
const MAX_LEN: usize = 80;

/// 比較に用いる入力。空と、`,` が読む値(0x00, 0xFF を含む)を用いる。
const INPUTS: [&[u8]; 2] = [&[], &[0x41, 0x00, 0xFF, 0x7F, 0x80]];

/// 比較に用いる上限。小さい値から停止しない程度の値まで、打ち切りの位置を変える。
const CAPS: [u64; 4] = [1, 16, 256, 2_048];

/// 操作種別ごとに、少なくとも含まれるべきプログラム本数の下限(生成分布の実測値より十分小さい値)。
const MIN_PROGRAMS_PER_KIND: usize = 500;

/// 生成時の記号と重み。加減算と移動を多めに配分し、入出力と括弧も混ぜる。
const WEIGHTED_TOKENS: [(u8, u64); 8] = [
    (b'+', 4),
    (b'-', 4),
    (b'>', 3),
    (b'<', 3),
    (b'[', 2),
    (b']', 2),
    (b'.', 1),
    (b',', 1),
];

/// 重み付きで 1 記号を選ぶ。
fn pick_token(rng: &mut XorShift64) -> u8 {
    let total: u64 = WEIGHTED_TOKENS.iter().map(|&(_, weight)| weight).sum();
    let mut roll = rng.below(total);
    for &(token, weight) in &WEIGHTED_TOKENS {
        if roll < weight {
            return token;
        }
        roll -= weight;
    }
    unreachable!("roll is below the total weight")
}

/// 生成時にテンプレート断片を置く確率の分母(1/4 の確率で断片を選ぶ)。
const FRAGMENT_ONE_IN: u64 = 4;

/// 生成時に挿入するテンプレート断片。すべて括弧が対応し、クリア・移動/加算/乗算・走査に変換されうる。
const FRAGMENTS: [&str; 8] = [
    "[-]",
    "[+]",
    "[->+<]",
    "[->>+++<<]",
    "[>]",
    "[<]",
    "[>>]",
    "[<<]",
];

/// 固定シードの乱数から、長さ [`MIN_LEN`]〜[`MAX_LEN`] の括弧が対応するプログラムを 1 本生成する。
///
/// 半分の確率で先頭に `+` を 1〜4 個積み、直後のループが反復するようにする。
/// 目標長 `target` に達するまで、一定確率でテンプレート断片を、それ以外は重み付きの記号を積む。
/// `total` は `out.len() + depth`(未閉じ括弧を閉じた場合の長さ)で、常に [`MAX_LEN`] 以下に保つ。
/// 最後に未閉じ括弧を閉じるため、長さは `target` 以上 [`MAX_LEN`] 以下になる。
fn generate_program(rng: &mut XorShift64) -> String {
    let target = MIN_LEN + rng.below((MAX_LEN - MIN_LEN + 1) as u64) as usize;
    let mut out: Vec<u8> = Vec::with_capacity(MAX_LEN);
    let mut depth = 0usize;
    if rng.below(2) == 0 {
        let count = 1 + rng.below(4) as usize;
        out.extend(std::iter::repeat_n(b'+', count));
    }
    while out.len() + depth < target {
        if rng.below(FRAGMENT_ONE_IN) == 0 {
            let fragment = FRAGMENTS[rng.below(FRAGMENTS.len() as u64) as usize];
            // 断片は括弧が対応しているので深さを変えない。上限に収まるときだけ置く。
            if out.len() + depth + fragment.len() <= MAX_LEN {
                out.extend_from_slice(fragment.as_bytes());
                continue;
            }
        }
        let token = pick_token(rng);
        match token {
            b'[' => {
                // 開き括弧は、閉じ括弧の追加を含めても上限に収まるときだけ置く。
                if out.len() + depth + 2 > MAX_LEN {
                    continue;
                }
                depth += 1;
            }
            b']' => {
                // 対応する開き括弧がないときは置かない。
                if depth == 0 {
                    continue;
                }
                depth -= 1;
            }
            _ => {}
        }
        out.push(token);
    }
    out.resize(out.len() + depth, b']');
    String::from_utf8(out).expect("generated symbols are ASCII")
}

/// 括弧が対応しているか(深さが負にならず、最後に 0 へ戻るか)。
fn is_balanced(source: &str) -> bool {
    let mut depth = 0i64;
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

/// 失敗メッセージに含める再現情報(シード、プログラム番号、ソース、入力、設定)。
fn program_label(
    seed: u64,
    index: usize,
    source: &str,
    input: &[u8],
    config: MachineConfig,
) -> String {
    format!(
        "seed {seed:#x}, program #{index}, source {source:?}, input {input:?}, config {config:?}"
    )
}

/// 生成したプログラムが長さ・括弧・記号の制約を満たすことを確認する。
#[test]
fn generated_programs_are_balanced_and_within_the_length_range() {
    let mut rng = XorShift64::new(RANDOM_SEED);
    for index in 0..PROGRAM_COUNT {
        let source = generate_program(&mut rng);
        assert!(
            (MIN_LEN..=MAX_LEN).contains(&source.len()),
            "program #{index} (seed {RANDOM_SEED:#x}) has length {}: {source:?}",
            source.len()
        );
        assert!(
            is_balanced(&source),
            "program #{index} (seed {RANDOM_SEED:#x}) has unbalanced brackets: {source:?}"
        );
        assert!(
            source
                .bytes()
                .all(|byte| WEIGHTED_TOKENS.iter().any(|&(token, _)| token == byte)),
            "program #{index} (seed {RANDOM_SEED:#x}) has an unexpected symbol: {source:?}"
        );
    }
}

/// 同じシードからは同じプログラム列が再生成されることを確認する(失敗の再現のため)。
#[test]
fn the_same_seed_reproduces_the_same_programs() {
    let generate = |seed: u64| -> Vec<String> {
        let mut rng = XorShift64::new(seed);
        (0..PROGRAM_COUNT)
            .map(|_| generate_program(&mut rng))
            .collect()
    };
    assert_eq!(generate(RANDOM_SEED), generate(RANDOM_SEED));
}

/// 固定シードのプログラム群を、空と非空の入力・複数の上限で core と最適化機械に比較させる。
#[test]
fn random_programs_match_the_core_machine() {
    let mut rng = XorShift64::new(RANDOM_SEED);
    for index in 0..PROGRAM_COUNT {
        let source = generate_program(&mut rng);
        for input in INPUTS {
            for cap in CAPS {
                let config = MachineConfig::with_max_steps(cap);
                let label = program_label(RANDOM_SEED, index, &source, input, config);
                let core = observe_core(&source, input, config);
                let (optimized, ir_ops) = observe_optimized(&source, input, config);
                assert_observations_equivalent(&label, &core, &optimized, ir_ops);
            }
        }
    }
}

/// 失敗メッセージにシードとプログラム番号、ソースが含まれることを確認する。
#[test]
#[should_panic(expected = "seed 0xb7f00d4320260001, program #3, source \"+.\"")]
fn mismatch_message_names_the_seed_and_the_source() {
    let config = MachineConfig::with_max_steps(16);
    let core = observe_core("+.", &[], config);
    let mut optimized = core.clone();
    optimized.pc += 1;
    let label = program_label(RANDOM_SEED, 3, "+.", &[], config);
    assert_observations_equivalent(&label, &core, &optimized, 0);
}

/// 固定シードの `XorShift64` の最初の 6 個の出力を固定値で照合する(乱数の変更検知)。
#[test]
fn xorshift_first_outputs_are_fixed() {
    let mut rng = XorShift64::new(42);
    let outputs: Vec<u64> = (0..6).map(|_| rng.next_u64()).collect();
    assert_eq!(
        outputs,
        [
            45_454_805_674,
            11_532_217_803_599_905_471,
            10_021_416_941_527_320_954,
            2_899_061_411_254_629_736,
            5_661_411_637_479_084_162,
            10_094_803_945_545_275_427,
        ]
    );
}

/// 固定シードから生成したプログラム列の先頭 3 本を固定値で照合する(生成器・重み・乱数の変更検知)。
#[test]
fn generated_program_prefix_is_fixed() {
    let mut rng = XorShift64::new(RANDOM_SEED);
    let sources: Vec<String> = (0..3).map(|_| generate_program(&mut rng)).collect();
    assert_eq!(
        sources,
        [
            "[->>+++<<].[->+<]+",
            "[-[+][,<+-+[-+[>][->>+++<<].>[->+<]]+>[]]]",
            "++++[+][[<<]<.-]",
        ]
    );
}

/// 生成した本数のうち、クリア・移動/加算/乗算・走査・入出力を含む本数が十分あることを確認する。
///
/// 各プログラムを公開 API で IR に変換し、操作種別ごとに含む本数を数える。
#[test]
fn generated_programs_cover_the_optimized_operation_kinds() {
    let mut rng = XorShift64::new(RANDOM_SEED);
    let (mut clear, mut transfer, mut scan, mut io) = (0usize, 0usize, 0usize, 0usize);
    for _ in 0..PROGRAM_COUNT {
        let source = generate_program(&mut rng);
        let program = Program::parse(&source).expect("generated programs parse");
        let optimized = OptimizedProgram::compile(&program);
        let kinds: Vec<&OpKind> = optimized.ops().iter().map(Op::kind).collect();
        if kinds
            .iter()
            .any(|kind| matches!(kind, OpKind::Clear { .. }))
        {
            clear += 1;
        }
        if kinds
            .iter()
            .any(|kind| matches!(kind, OpKind::Transfer { .. }))
        {
            transfer += 1;
        }
        if kinds.iter().any(|kind| matches!(kind, OpKind::Scan { .. })) {
            scan += 1;
        }
        if kinds
            .iter()
            .any(|kind| matches!(kind, OpKind::Input | OpKind::Output))
        {
            io += 1;
        }
    }
    assert!(clear >= MIN_PROGRAMS_PER_KIND, "clear programs: {clear}");
    assert!(
        transfer >= MIN_PROGRAMS_PER_KIND,
        "transfer programs: {transfer}"
    );
    assert!(scan >= MIN_PROGRAMS_PER_KIND, "scan programs: {scan}");
    assert!(io >= MIN_PROGRAMS_PER_KIND, "io programs: {io}");
}
