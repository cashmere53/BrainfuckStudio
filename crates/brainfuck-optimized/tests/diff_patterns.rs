//! 最適化パターンと非適用境界の比較。素朴実装(core)をオラクルとして、最適化機械と比較する。
//!
//! - 連続命令の統合(加減算・同方向移動)
//! - クリア(`[-]` `[+]`)
//! - 移動/加算/乗算(単一項・複数項・負の相対位置・係数の掃引・乱数係数)
//! - 走査(両向き・複数命令ストライド・走査の途中に 0 がない長さの掃引)
//! - 条件を満たさない近傍形(正味変化 -2・ポインタ正味移動非 0・入出力を含む本体・入れ子ループ・
//!   逆向き混在の走査・`[-]` を内側に持つ一般ループ)
//!
//! 各断片は、ポインタを `BASE` 位置へ運んだ後に開始セルを `CELL_VALUES` の各値に設定してから置く。
//! 比較は大きな上限と、途中の境界を含む小さな上限の組で行う。最適化を意図した形は IR を検査し、
//! 実際に 1 操作へ置換されていることを確認する。非適用の形は一般ループのまま残ることを確認する。
//! 共通基盤(`tests/common`)は変更せず、テンプレートと乱数係数の生成器はこのファイル内に置く。

mod common;

use brainfuck_core::{MachineConfig, Program};
use brainfuck_optimized::{OpKind, OptimizedProgram};
use common::{assert_equivalent, XorShift64};

/// ポインタを置く初期位置。負方向の移動・走査が範囲外にならないよう、テープの中央寄りに置く。
const BASE: usize = 64;

/// 開始セルに設定する値。0・1・2 と符号境界(127, 128, 255)を含む。
const CELL_VALUES: [u8; 6] = [0, 1, 2, 127, 128, 255];

/// 比較に用いる入力。空と、`,` が読む値(0x00, 0xFF を含む)を用いる。
const INPUTS: [&[u8]; 2] = [&[], &[0x41, 0x00, 0xFF]];

/// 大きな上限。停止するプログラムは停止まで、停止しないプログラムはこの値で打ち切る。
const LARGE_CAP: u64 = 50_000;

/// 途中の境界を含む小さな上限。最適化操作の途中で打ち切られる位置を覆う。
const SMALL_LIMITS: [u64; 6] = [0, 1, 2, 3, 7, 40];

/// 乱数係数の生成に用いる固定シード。失敗時の再現に使う。
const SEED: u64 = 0x4A47_1E12_0000_0044;

/// 乱数係数の生成本数。
const GENERATED_BODIES: usize = 200;

/// 走査で置く非 0 セルの本数(0 は走査が即座に止まる形を含む)。
const RUNS: [usize; 4] = [0, 1, 2, 5];

/// 係数の掃引に用いる係数(1 と 255 の端、128 付近の境界を含む)。
const FACTORS: [u8; 11] = [1, 2, 3, 4, 10, 64, 127, 128, 129, 200, 255];

/// 係数の掃引に用いる相対位置(正と負、複数離れた位置を含む)。
const OFFSETS: [isize; 6] = [1, -1, 2, -2, 5, -5];

/// 連続命令の断片。統合・非統合の境界(逆向きの隣接、0 となる正味量、256 回以上の加算)を含む。
const CONSECUTIVE: [&str; 14] = [
    "+", "+++", "-", "--", "+-+-", "-+-+-", ">", ">>>", "<", "<<", "><", "+>+<", "+>-<>", "+++>--<",
];

/// 走査の断片と、その 1 反復あたりの移動量(符号が向き)。
const SCANS: [(&str, isize); 6] = [
    ("[>]", 1),
    ("[<]", -1),
    ("[>>]", 2),
    ("[<<]", -2),
    ("[>>>]", 3),
    ("[<<<]", -3),
];

/// 移動/加算/乗算の固定の断片(本体が `+ - > <` のみ、ポインタ正味移動 0、開始セルの正味変化 -1)。
const TRANSFERS: [&str; 9] = [
    "[->+<]",
    "[->>+++<<]",
    "[->+>+<<]",
    "[-<+>]",
    "[->-<]",
    "[->++++<]",
    "[-<<+>>]",
    "[>+<-]",
    "[->+>++>+++<<<]",
];

/// クリアの断片。
const CLEARS: [&str; 2] = ["[-]", "[+]"];

/// 条件を満たさない近傍形。どれも最適化されず、一般ループとして残ることを確認する。
///
/// 正味変化 -2・-3・0・+1 の開始セル、ポインタ正味移動が非 0、本体に入出力、入れ子ループ(内側に `[-]`)、
/// 逆向き混在の走査を含む。
const NEAR_MISSES: [&str; 15] = [
    "[--]",
    "[--->+<]",
    "[-->+<]",
    "[-+]",
    "[->]",
    "[->+]",
    "[-<+>+]",
    "[-.]",
    "[->,<]",
    "[>[-]<-]",
    "[>[->+<]<-]",
    "[><]",
    "[>><]",
    "[+>]",
    "[<>+]",
];

/// `n` 個の `>` (正)または `<` (負)を返す。
fn moves(offset: isize) -> String {
    if offset >= 0 {
        ">".repeat(offset as usize)
    } else {
        "<".repeat(offset.unsigned_abs())
    }
}

/// 係数 `factor`(mod 256、0 でない)を、現在セルへの加算として `+` または `-` の列で表す。
fn factor_ops(factor: u8) -> String {
    if factor <= 128 {
        "+".repeat(usize::from(factor))
    } else {
        "-".repeat(256 - usize::from(factor))
    }
}

/// 開始セルを `CELL_VALUES` の各値に設定し、`setup`(ポインタを動かさず、必要なら周辺セルを設定する)を経て `body` を置いた
/// 各プログラムを、各入力・大きな上限と小さな上限の組で core と比較する。
fn compare_across_cells(setup: &str, body: &str) {
    for value in CELL_VALUES {
        let source = format!(
            "{}{}{}{}",
            ">".repeat(BASE),
            "+".repeat(usize::from(value)),
            setup,
            body
        );
        for input in INPUTS {
            assert_equivalent(&source, input, MachineConfig::with_max_steps(LARGE_CAP));
            for limit in SMALL_LIMITS {
                assert_equivalent(&source, input, MachineConfig::with_max_steps(limit));
            }
        }
    }
}

/// `source`(括弧を含むプログラム)を最適化した IR 操作の種別の列を返す。
fn compile_kinds(source: &str) -> Vec<OpKind> {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    OptimizedProgram::compile(&program)
        .ops()
        .iter()
        .map(|op| op.kind().clone())
        .collect()
}

/// 走査のテンプレートの前に置く、周辺の非 0 セルの列。`stride` 方向へ `run` 個のセルを非 0 にし、ポインタを戻す。
///
/// 走査は非 0 のセルを続けて渡り、最初の 0 のセルで止まる。`run == 0` のときは何も置かない。
fn run_setup(stride: isize, run: usize) -> String {
    let mut out = String::new();
    for i in 0..run {
        out.push_str(&moves(stride));
        out.push_str(if i % 2 == 0 { "+" } else { "++" });
    }
    out.push_str(&moves(-stride * run as isize));
    out
}

/// 固定シードの乱数から、移動/加算/乗算の本体を 1 本生成する。
///
/// 1〜3 項の相対位置(-3〜3、0 を除く)に、正負それぞれ 1〜20 の係数を割り当てる。開始セルの `-` は
/// 項の列の任意の境界に挿入するため、本体は必ず `ポインタ正味 0・開始セル正味 -1` を満たす。
fn random_transfer_body(rng: &mut XorShift64) -> String {
    let term_count = 1 + rng.below(3) as usize;
    let mut offsets: Vec<isize> = Vec::new();
    while offsets.len() < term_count {
        let offset = rng.below(7) as isize - 3;
        if offset != 0 && !offsets.contains(&offset) {
            offsets.push(offset);
        }
    }
    let mut segments: Vec<String> = offsets
        .iter()
        .map(|&offset| {
            let magnitude = 1 + rng.below(20) as u8;
            let factor = if rng.below(2) == 0 {
                magnitude
            } else {
                256u16.wrapping_sub(u16::from(magnitude)) as u8
            };
            format!("{}{}{}", moves(offset), factor_ops(factor), moves(-offset))
        })
        .collect();
    let at = rng.below(segments.len() as u64 + 1) as usize;
    segments.insert(at, "-".to_string());
    format!("[{}]", segments.concat())
}

/// 連続命令の統合が、素朴実装と同じ観測結果を返すことを確認する。
#[test]
fn consecutive_instructions_match_the_core_machine() {
    for body in CONSECUTIVE {
        compare_across_cells("", body);
    }
    // 256 回以上の加算は mod 256 で正味量になるため、ラップの境界を含めて比較する。
    compare_across_cells("", &"+".repeat(300));
    compare_across_cells("", &"-".repeat(257));
}

/// 連続命令が 1 操作に統合されることを IR で確認する(統合の意図した形)。
#[test]
fn consecutive_instructions_fuse_into_single_operations() {
    assert_eq!(compile_kinds("+++"), vec![OpKind::Add(3)]);
    assert_eq!(compile_kinds("-"), vec![OpKind::Add(255)]);
    assert_eq!(compile_kinds(">>>"), vec![OpKind::Move(3)]);
    assert_eq!(compile_kinds("<<"), vec![OpKind::Move(-2)]);
    // 逆向きの隣接は統合しない。
    assert_eq!(compile_kinds("><"), vec![OpKind::Move(1), OpKind::Move(-1)]);
}

/// クリアが素朴実装と同じ観測結果を返し、1 操作へ置換されることを確認する。
#[test]
fn clear_loops_match_the_core_machine() {
    for body in CLEARS {
        compare_across_cells("", body);
        compare_across_cells(">>>", body);
    }
    assert_eq!(
        compile_kinds("[-]"),
        vec![OpKind::Clear { increments: false }]
    );
    assert_eq!(
        compile_kinds("[+]"),
        vec![OpKind::Clear { increments: true }]
    );
}

/// 移動/加算/乗算の固定の断片が、素朴実装と同じ観測結果を返し、1 操作へ置換されることを確認する。
#[test]
fn transfer_loops_match_the_core_machine() {
    for body in TRANSFERS {
        compare_across_cells("", body);
        let kinds = compile_kinds(body);
        assert_eq!(kinds.len(), 1, "{body:?} should fuse into one operation");
        assert!(
            matches!(kinds[0], OpKind::Transfer { .. }),
            "{body:?} should be a transfer loop, got {:?}",
            kinds[0]
        );
    }
}

/// 移動/加算/乗算の係数と相対位置を掃引して比較する。
#[test]
fn transfer_coefficient_sweep_matches_the_core_machine() {
    for factor in FACTORS {
        for offset in OFFSETS {
            let body = format!(
                "[-{}{}{}]",
                moves(offset),
                factor_ops(factor),
                moves(-offset)
            );
            compare_across_cells("", &body);
            let kinds = compile_kinds(&body);
            assert!(
                matches!(kinds.as_slice(), [OpKind::Transfer { .. }]),
                "{body:?} should be a transfer loop, got {kinds:?}"
            );
        }
    }
}

/// 固定シードで生成した移動/加算/乗算の本体(複数項・負の相対位置・乱数係数)が素朴実装と一致することを確認する。
#[test]
fn generated_transfer_bodies_match_the_core_machine() {
    let mut rng = XorShift64::new(SEED);
    for _ in 0..GENERATED_BODIES {
        let body = random_transfer_body(&mut rng);
        compare_across_cells("", &body);
        let kinds = compile_kinds(&body);
        assert!(
            matches!(kinds.as_slice(), [OpKind::Transfer { .. }]),
            "generated body {body:?} (seed {SEED:#x}) should be a transfer loop, got {kinds:?}"
        );
    }
}

/// 走査が素朴実装と同じ観測結果を返し、1 操作へ置換されることを確認する。
///
/// 走査の途中にある非 0 セルの本数(0 を含む)を掃引し、走査の反復数を変えて比較する。
#[test]
fn scan_loops_match_the_core_machine() {
    for (body, stride) in SCANS {
        for run in RUNS {
            compare_across_cells(&run_setup(stride, run), body);
        }
        let kinds = compile_kinds(body);
        assert_eq!(
            kinds,
            vec![OpKind::Scan { stride }],
            "{body:?} should be a scan loop"
        );
    }
}

/// 条件を満たさない近傍形が、素朴実装と同じ観測結果を返すことを確認する。
#[test]
fn non_applicable_neighbourhoods_match_the_core_machine() {
    for body in NEAR_MISSES {
        compare_across_cells("", body);
    }
}

/// 条件を満たさない近傍形が、最適化されず一般ループ(`[` で始まる)として残ることを確認する。
///
/// 入れ子の `[-]` は内側で通常どおりクリアとして認識されるため、判定は外側ループの先頭操作で行う。
#[test]
fn non_applicable_neighbourhoods_stay_general_loops() {
    for body in NEAR_MISSES {
        let kinds = compile_kinds(body);
        assert!(
            matches!(kinds.first(), Some(OpKind::LoopStart { .. })),
            "{body:?} should stay a general loop, got {kinds:?}"
        );
    }
}

/// 連続命令・クリア・移動/加算/乗算・走査を組み合わせた複合プログラムが一致することを確認する。
#[test]
fn combined_patterns_match_the_core_machine() {
    compare_across_cells("", "+++[->++<]>[-]+<[>]");
    compare_across_cells("", "++>[-<+>]<[->>+<<]>>[<]");
}
