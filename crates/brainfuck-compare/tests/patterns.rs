//! パターン別の差分テスト(13.4): 連続命令・クリア・移動/加算/乗算・走査の各テンプレートを、
//! ランダムな係数・オフセット・初期セル値(0, 1, 2, 127, 128, 255)で生成し、WASM と素朴を比較する。
//! あわせて、近いが条件を満たさない形(正味変化 -2、ポインタ正味非 0、本体に入出力・空ループ、
//! 逆向き混在の走査)も比較する。
//!
//! 適用形は最適化 IR が該当するパターン操作を持つこと(係数・符号・幅・向きを含む)を、
//! 非適用形は操作を持たないことを確かめる。失敗メッセージにはパターン名・シード・上限・
//! ソース全体を含め、そのまま再現できるようにする。

mod common;

use brainfuck_compare::methods::{run_method, run_naive};
use brainfuck_compare::Method;
use brainfuck_core::Program;
use brainfuck_optimized::{Op, OpKind, OptimizedProgram};
use common::{pair_diff, XorShift64};

/// 初期セル値(境界を含む)。
const START_VALUES: [u8; 6] = [0, 1, 2, 127, 128, 255];
/// 停止性の判定と余裕のある上限に使う命令数。最大の適用形(v = 255 の乗算: 約 11 万命令)が完走する値。
const STEP_BUDGET: u64 = 300_000;
/// 左端に触れないよう先頭で進める移動数。
const CENTER: usize = 10;
/// 走査の既定の先頭セル(左右どちらへ走査しても正の位置に収まる)。
const SCAN_BASE: isize = 40;
/// 1 テンプレートあたりの乱数インスタンス数。
const INSTANCES: u64 = 4;

/// 適用形が期待する最適化操作(値まで指定する)。
#[derive(Clone, Debug)]
enum Expect {
    /// 同じ `+`/`-` の連続が 1 操作の `Add(net)` に `len` 命令分まとまる。
    Add { net: u8, len: usize },
    /// クリア(`[-]` なら `increments = false`、`[+]` なら `true`)。
    Clear { increments: bool },
    /// 移動/加算/乗算。項は `offset` 昇順。
    Transfer { terms: Vec<(isize, u8)> },
    /// 走査(符号が向き、絶対値が 1 反復の移動数)。
    Scan { stride: isize },
}

/// 1 つのプログラムの期待。
#[derive(Clone, Debug)]
enum Shape {
    /// 適用形: 期待する最適化操作を持つ。
    Applied(Expect),
    /// 非適用形: クリア・移動/加算/乗算・走査のいずれも認識されない。
    NearMiss,
    /// IR の期待を置かない(連続命令の非統合など、本テストは実行結果の一致のみを見る)。
    Unchecked,
}

/// 比較する 1 件。
struct Case {
    label: &'static str,
    seed: u64,
    source: String,
    shape: Shape,
}

#[test]
fn continuous_runs_match_naive_for_wasm() {
    for &count in &[1usize, 2, 255, 256, 257, 300] {
        let mod_count = u8::try_from(count % 256).expect("mod 256 は 1 バイト");
        check(Case {
            label: "連続 +",
            seed: 0,
            source: "+".repeat(count),
            shape: Shape::Applied(Expect::Add {
                net: mod_count,
                len: count,
            }),
        });
        check(Case {
            label: "連続 -",
            seed: 0,
            source: "-".repeat(count),
            shape: Shape::Applied(Expect::Add {
                net: 0u8.wrapping_sub(mod_count),
                len: count,
            }),
        });
    }
    let mut rng = XorShift64::new(0x1341_0001);
    for instance in 0..INSTANCES {
        let mut source = ">".repeat(CENTER);
        for _ in 0..6 {
            let count = 1 + usize::try_from(rng.below(40)).expect("小さい");
            let piece = match rng.below(4) {
                0 => "+".repeat(count),
                1 => "-".repeat(count),
                2 => "+-".repeat(count),
                _ => "><".repeat(count),
            };
            source.push_str(&piece);
        }
        check(Case {
            label: "連続命令の混在",
            seed: instance,
            source,
            shape: Shape::Unchecked,
        });
    }
    // 近いが統合されない形: 向きの異なる隣接移動、正味 0 の加減算(操作は残る)。
    check(Case {
        label: "逆向きの隣接移動(非統合)",
        seed: 0,
        source: format!("{}{}", ">".repeat(CENTER), "><".repeat(20)),
        shape: Shape::Unchecked,
    });
    check(Case {
        label: "正味 0 の加減算",
        seed: 0,
        source: "+-".repeat(20),
        shape: Shape::Unchecked,
    });
}

#[test]
fn centered_runs_the_loop_body_on_the_start_cell() {
    // 開始セルの値 5 が、ループの対象セル(先頭から CENTER 進んだセル)に置かれていること。
    // 本体が一度も実行されないと、素朴の命令数は移動と加算の分(26)しかなくなる。
    let program = Program::parse(&centered(5, "[-]")).expect("テスト用ソースは解析できる");
    let observation = run_naive(&program, b"", STEP_BUDGET)
        .expect("素朴は失敗しない")
        .observation;
    assert!(observation.halted(), "[-] は停止する");
    assert!(
        observation.steps_executed > 26,
        "ループ本体が実行されていない: 実行数 {}",
        observation.steps_executed
    );
    assert_eq!(observation.tape[CENTER], 0, "ループ対象セルは 0 になる");
}

#[test]
fn clear_patterns_match_naive_for_wasm() {
    for &v in &START_VALUES {
        for (body, increments) in [("[-]", false), ("[+]", true)] {
            check(Case {
                label: "クリア",
                seed: u64::from(v),
                source: centered(v, body),
                shape: Shape::Applied(Expect::Clear { increments }),
            });
        }
        // 近いが条件を満たさない形: 正味変化 -2、+2、0。
        for body in ["[--]", "[++]", "[+-]"] {
            check(Case {
                label: "クリアの近傍(非適用)",
                seed: u64::from(v),
                source: centered(v, body),
                shape: Shape::NearMiss,
            });
        }
    }
}

#[test]
fn transfer_patterns_match_naive_for_wasm() {
    for instance in 0..INSTANCES {
        let mut rng = XorShift64::new(0x1341_0100 + instance);
        for &v in &START_VALUES {
            let terms = random_terms(&mut rng);
            let body = transfer_body(&terms);
            let mut sorted = terms.clone();
            sorted.sort();
            check(Case {
                label: "移動/加算/乗算",
                seed: instance,
                source: centered(v, &format!("[{body}]")),
                shape: Shape::Applied(Expect::Transfer { terms: sorted }),
            });

            // 近いが条件を満たさない形(同じ項から作る)。
            let rest = &body[1..];
            let near = [
                ("正味変化 -2", format!("--{rest}")),
                ("ポインタ正味非 0", format!("{body}>")),
                ("本体に出力", format!("{body}.")),
                ("本体に空ループ", format!("-[]{rest}")),
            ];
            for (label, near_body) in near {
                check(Case {
                    label,
                    seed: instance,
                    source: centered(v, &format!("[{near_body}]")),
                    shape: Shape::NearMiss,
                });
            }
        }
    }
}

#[test]
fn scan_patterns_match_naive_for_wasm() {
    const VALUES: [u8; 5] = [1, 2, 127, 128, 255];
    for instance in 0..INSTANCES {
        let mut rng = XorShift64::new(0x1341_0200 + instance);
        for &v in &START_VALUES {
            for direction in [1isize, -1] {
                let step = direction * isize::try_from(1 + rng.below(4)).expect("小さい");
                let count = 1 + usize::try_from(rng.below(6)).expect("小さい");
                let values: Vec<u8> = (0..count)
                    .map(|_| VALUES[usize::try_from(rng.below(5)).expect("小さい")])
                    .collect();
                check(Case {
                    label: "走査",
                    seed: instance,
                    source: scan_program(SCAN_BASE, v, step, &values, &scan_body(step)),
                    shape: Shape::Applied(Expect::Scan { stride: step }),
                });
            }
            // 近いが条件を満たさない走査: 逆向きの混在、入出力、空ループ(本体は外側の `[]` を含まない)。
            let values = [1u8, 255, 2];
            for body in ["><", ">><", "<>>", ">.", ">[]"] {
                check(Case {
                    label: "走査の近傍(非適用)",
                    seed: instance,
                    source: scan_program(SCAN_BASE, v, 1, &values, &format!("[{body}]")),
                    shape: Shape::NearMiss,
                });
            }
        }
    }
}

/// 走査が左端・右端に達する形: 範囲内で完走する形と、範囲外に出る形の両方を比較する。
#[test]
fn scan_reaching_tape_edges_match_naive_for_wasm() {
    // 右向きは末尾付近(29_990)から、左向きは先頭付近(9)から走査する。
    // 値の個数 9 は基点から端のセルまで非 0 で埋め、走査が端の外へ出る形を作る(3 は完走する形)。
    for (base, step) in [(29_990isize, 1isize), (9, -1)] {
        for count in [3usize, 9] {
            let values: Vec<u8> = (0..count).map(|j| [1u8, 127, 255][j % 3]).collect();
            for v in [0u8, 1, 255] {
                check(Case {
                    label: "走査(端)",
                    seed: u64::try_from(count).expect("小さい"),
                    source: scan_program(base, v, step, &values, &scan_body(step)),
                    shape: Shape::Unchecked,
                });
            }
        }
    }
}

/// ポインタを `CENTER` 進めてから、開始セルに `v` を置き、`loop_source` のループを続けたプログラム。
/// ループは開始セル(`v` を置いたセル)を対象にする。
fn centered(v: u8, loop_source: &str) -> String {
    format!(
        "{}{}{}{}",
        ">".repeat(CENTER),
        add_mod256(v),
        loop_source,
        "<".repeat(CENTER)
    )
}

/// 係数 `f`(mod 256)を加算する記号列。`f <= 128` は `+` を、それ以外は `-` を使う。
fn add_mod256(f: u8) -> String {
    if f <= 128 {
        "+".repeat(usize::from(f))
    } else {
        "-".repeat(256 - usize::from(f))
    }
}

/// 相対オフセットと係数(0 は含まない)の項を、先頭 `-` から始まる移動/加算/乗算の本体にする。
fn transfer_body(terms: &[(isize, u8)]) -> String {
    let mut body = String::from("-");
    for &(offset, factor) in terms {
        let (forward, back) = if offset > 0 { (">", "<") } else { ("<", ">") };
        let steps = offset.unsigned_abs();
        body.push_str(&forward.repeat(steps));
        body.push_str(&add_mod256(factor));
        body.push_str(&back.repeat(steps));
    }
    body
}

/// 1〜3 項の乱数(オフセットは -8..=8 の非 0 で重複なし、係数は 1..=255)。
fn random_terms(rng: &mut XorShift64) -> Vec<(isize, u8)> {
    let mut offsets: Vec<isize> = (-8..=8).filter(|&o| o != 0).collect();
    for i in (1..offsets.len()).rev() {
        let j = usize::try_from(rng.below(u64::try_from(i + 1).expect("小さい"))).expect("小さい");
        offsets.swap(i, j);
    }
    let count = 1 + usize::try_from(rng.below(3)).expect("小さい");
    offsets
        .into_iter()
        .take(count)
        .map(|offset| (offset, u8::try_from(1 + rng.below(255)).expect("1..=255")))
        .collect()
}

/// 走査の本体(`step` の符号が向き、絶対値が 1 反復の移動数)。
fn scan_body(step: isize) -> String {
    let unit = if step > 0 { ">" } else { "<" };
    unit.repeat(step.unsigned_abs())
}

/// 基点から `step` ごとに値を並べ、基点に `start` を置いて走査ループを実行するプログラム。
/// 並べた最後の次のセルは 0 のままなので、走査は必ず停止する。
fn scan_program(base: isize, start: u8, step: isize, values: &[u8], body: &str) -> String {
    let mut cells: Vec<(isize, u8)> = vec![(base, start)];
    for (j, &value) in values.iter().enumerate() {
        let index = isize::try_from(j + 1).expect("小さい");
        cells.push((base + index * step, value));
    }
    let mut out = String::new();
    let mut cursor: isize = 0;
    for (position, value) in cells {
        out.push_str(&move_to(&mut cursor, position));
        out.push_str(&add_mod256(value));
    }
    out.push_str(&move_to(&mut cursor, base));
    out.push('[');
    out.push_str(body);
    out.push(']');
    out
}

/// `cursor` から `target` へ移動する記号列を返し、`cursor` を更新する。
fn move_to(cursor: &mut isize, target: isize) -> String {
    let diff = target - *cursor;
    *cursor = target;
    if diff >= 0 {
        ">".repeat(usize::try_from(diff).expect("非負"))
    } else {
        "<".repeat(usize::try_from(-diff).expect("非負"))
    }
}

/// 比較する上限の一覧。停止する形は必要数の前後・途中と、余裕のある上限(全実行)を含める。
/// 停止しない形(または範囲外で止まる形)は予算いっぱいの上限で打ち切る。
fn limits_for(source: &str) -> Vec<u64> {
    let program = Program::parse(source).expect("テスト用ソースは解析できる");
    let execution = run_naive(&program, b"", STEP_BUDGET).expect("素朴は失敗しない");
    let mut limits = vec![0, 1, 2, 17];
    if execution.observation.halted() {
        let needed = execution.observation.steps_executed;
        limits.extend([
            needed / 2,
            needed.saturating_sub(1),
            needed,
            needed + 1,
            STEP_BUDGET,
        ]);
    } else {
        limits.push(STEP_BUDGET);
    }
    limits
}

/// IR が期待どおりの形を持つかを確かめ、全上限で WASM と素朴の一致を検査する。
fn check(case: Case) {
    let program = Program::parse(&case.source).expect("テスト用ソースは解析できる");
    let ops = OptimizedProgram::compile(&program).ops().to_vec();
    match &case.shape {
        Shape::Applied(expect) => assert!(
            has_expected(&ops, expect),
            "パターン {} の期待 {expect:?} を IR が満たさない: シード {}, ソース {:?}, IR {:?}",
            case.label,
            case.seed,
            case.source,
            ops.iter().map(Op::kind).collect::<Vec<_>>()
        ),
        Shape::NearMiss => assert!(
            !ops.iter().any(|op| {
                matches!(
                    op.kind(),
                    OpKind::Clear { .. } | OpKind::Transfer { .. } | OpKind::Scan { .. }
                )
            }),
            "パターン {} は非適用のはずが認識された: シード {}, ソース {:?}",
            case.label,
            case.seed,
            case.source
        ),
        Shape::Unchecked => {}
    }
    for limit in limits_for(&case.source) {
        let diff = pair_diff(&case.source, b"", limit, Method::Wasm);
        assert!(
            diff.is_empty(),
            "パターン {} で WASM が素朴と不一致: 項目 {diff:?}・シード {}・上限 {limit}・ソース {:?}",
            case.label,
            case.seed,
            case.source
        );
    }
}

/// 期待する操作が IR に、期待する値で含まれるか。
fn has_expected(ops: &[Op], expect: &Expect) -> bool {
    ops.iter().any(|op| match (expect, op.kind()) {
        (Expect::Add { net, len }, OpKind::Add(got)) => {
            got == net && op.source_range().len() == *len
        }
        (Expect::Clear { increments }, OpKind::Clear { increments: got }) => got == increments,
        (Expect::Transfer { terms }, OpKind::Transfer { terms: got, .. }) => {
            let actual: Vec<(isize, u8)> = got.iter().map(|t| (t.offset, t.factor)).collect();
            &actual == terms
        }
        (Expect::Scan { stride }, OpKind::Scan { stride: got }) => got == stride,
        _ => false,
    })
}

/// 到達範囲がテープの端ちょうどに届く形は、離脱せずに完走する(必要以上に離脱しない)。
/// 離脱すると再生の命令数が 0 より大きくなるため、再生数で離脱の有無を確かめる。
#[test]
fn reach_exactly_at_the_tape_edges_does_not_bail_out() {
    let programs = [
        // 移動: 右端 29_999 ちょうどへ、続けて左端 0 ちょうどへ。
        format!("{}{}", ">".repeat(29_999), "<".repeat(29_999)),
        // 移動/加算/乗算: 右端ちょうどへ届く項(到達範囲の最大 = LAST)。
        format!(
            "{}+[-{}+{}]",
            ">".repeat(29_990),
            ">".repeat(9),
            "<".repeat(9)
        ),
        // 移動/加算/乗算: 左端ちょうどへ届く項(到達範囲の最小 = 0 の番地)。
        format!("{}+[-{}+{}]", ">".repeat(9), "<".repeat(9), ">".repeat(9)),
    ];
    for source in &programs {
        let program = Program::parse(source).expect("テスト用ソースは解析できる");
        let execution = run_method(Method::Wasm, &program, b"", STEP_BUDGET)
            .expect("範囲内の形は WASM で完走する");
        assert_eq!(
            execution.metrics.replayed_steps,
            Some(0),
            "端ちょうどの形で離脱した(再生が発生): ソース {}",
            describe_source(source)
        );
        let diff = pair_diff(source, b"", STEP_BUDGET, Method::Wasm);
        assert!(
            diff.is_empty(),
            "素朴と不一致 {diff:?}: ソース {}",
            describe_source(source)
        );
    }
}

fn describe_source(source: &str) -> String {
    const SHOWN: usize = 60;
    if source.len() <= SHOWN {
        format!("{source:?}")
    } else {
        format!("{:?}…(全{}バイト)", &source[..SHOWN], source.len())
    }
}
