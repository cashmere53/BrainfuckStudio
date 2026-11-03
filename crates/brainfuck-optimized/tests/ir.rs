//! 変換器(`OptimizedProgram::compile`)の公開 API を通した検証。
//!
//! 2.2: 加減算・移動の統合、入出力の 1 命令 1 操作、一般ループの括弧対応。
//! 2.3: 代表的ループ(クリア・移動/加算/乗算・走査)の認識。

use brainfuck_core::Program;
use brainfuck_optimized::{Op, OpKind, OptimizedProgram, Term};

fn compile(source: &str) -> OptimizedProgram {
    let program = Program::parse(source).expect("valid program");
    OptimizedProgram::compile(&program)
}

fn kinds(optimized: &OptimizedProgram) -> Vec<OpKind> {
    optimized.ops().iter().map(|op| op.kind().clone()).collect()
}

fn ranges(optimized: &OptimizedProgram) -> Vec<std::ops::Range<usize>> {
    optimized.ops().iter().map(Op::source_range).collect()
}

#[test]
fn consecutive_add_and_sub_merge_into_net_amount() {
    let optimized = compile("+++--");
    assert_eq!(kinds(&optimized), vec![OpKind::Add(1)]);
    assert_eq!(ranges(&optimized), vec![0..5]);
}

#[test]
fn net_zero_add_sub_remains_as_an_operation() {
    let optimized = compile("+-");
    assert_eq!(kinds(&optimized), vec![OpKind::Add(0)]);
    assert_eq!(ranges(&optimized), vec![0..2]);
}

#[test]
fn net_amount_wraps_modulo_256() {
    let minus_one = compile("-");
    assert_eq!(kinds(&minus_one), vec![OpKind::Add(255)]);
    let plus_256 = compile(&"+".repeat(256));
    assert_eq!(kinds(&plus_256), vec![OpKind::Add(0)]);
    assert_eq!(ranges(&plus_256), vec![0..256]);
}

#[test]
fn same_direction_moves_merge_and_opposite_moves_do_not() {
    let optimized = compile(">>><<");
    assert_eq!(kinds(&optimized), vec![OpKind::Move(3), OpKind::Move(-2)]);
    assert_eq!(ranges(&optimized), vec![0..3, 3..5]);
}

#[test]
fn adjacent_opposite_moves_are_not_merged() {
    let optimized = compile("><");
    assert_eq!(kinds(&optimized), vec![OpKind::Move(1), OpKind::Move(-1)]);
    assert_eq!(ranges(&optimized), vec![0..1, 1..2]);
}

#[test]
fn output_and_input_are_one_operation_per_instruction() {
    let optimized = compile("..,,");
    assert_eq!(
        kinds(&optimized),
        vec![OpKind::Output, OpKind::Output, OpKind::Input, OpKind::Input]
    );
    assert_eq!(ranges(&optimized), vec![0..1, 1..2, 2..3, 3..4]);
}

#[test]
fn nested_general_loops_resolve_partner_operation_indices() {
    let optimized = compile("[.[.]]");
    assert_eq!(
        kinds(&optimized),
        vec![
            OpKind::LoopStart { end: 5 },
            OpKind::Output,
            OpKind::LoopStart { end: 4 },
            OpKind::Output,
            OpKind::LoopEnd { start: 2 },
            OpKind::LoopEnd { start: 0 },
        ]
    );
    assert_eq!(ranges(&optimized), vec![0..1, 1..2, 2..3, 3..4, 4..5, 5..6]);
}

fn transfer(terms: &[(isize, u8)], body_len: usize, reach: (isize, isize)) -> OpKind {
    OpKind::Transfer {
        terms: terms
            .iter()
            .map(|&(offset, factor)| Term { offset, factor })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        body_len,
        reach_min: reach.0,
        reach_max: reach.1,
    }
}

#[test]
fn clear_loops_become_one_clear_operation() {
    let decrement = compile("[-]");
    assert_eq!(kinds(&decrement), vec![OpKind::Clear { increments: false }]);
    assert_eq!(ranges(&decrement), vec![0..3]);

    let increment = compile("[+]");
    assert_eq!(kinds(&increment), vec![OpKind::Clear { increments: true }]);
    assert_eq!(ranges(&increment), vec![0..3]);
}

#[test]
fn transfer_loops_record_terms_reach_and_body_length() {
    let simple = compile("[->+<]");
    assert_eq!(kinds(&simple), vec![transfer(&[(1, 1)], 4, (0, 1))]);
    assert_eq!(ranges(&simple), vec![0..6]);

    let far = compile("[->>+++<<]");
    assert_eq!(kinds(&far), vec![transfer(&[(2, 3)], 8, (0, 2))]);
    assert_eq!(ranges(&far), vec![0..10]);

    let two_targets = compile("[->+>+<<]");
    assert_eq!(
        kinds(&two_targets),
        vec![transfer(&[(1, 1), (2, 1)], 7, (0, 2))]
    );
    assert_eq!(ranges(&two_targets), vec![0..9]);
}

#[test]
fn transfer_terms_are_aggregated_per_offset_and_sorted() {
    // 相対位置 1 への +2 と、相対位置 2 への -1(= 255)。位置 0 の項は含まれない。
    let optimized = compile("[->++>-<<]");
    assert_eq!(
        kinds(&optimized),
        vec![transfer(&[(1, 2), (2, 255)], 8, (0, 2))]
    );
}

#[test]
fn transfer_reach_includes_zero_and_the_extremes_visited() {
    // 本体は左へ 2 戻ってから右へ戻る。到達範囲は -2..=0 になる。
    let optimized = compile("[-<<+>>]");
    assert_eq!(kinds(&optimized), vec![transfer(&[(-2, 1)], 6, (-2, 0))]);
    assert_eq!(ranges(&optimized), vec![0..8]);
}

#[test]
fn transfer_terms_with_zero_net_factor_are_excluded() {
    // 相対位置 1 への +1 と -1 は打ち消し合い、係数 0 の項は残らない。
    let only_zero = compile("[->+-<]");
    assert_eq!(kinds(&only_zero), vec![transfer(&[], 5, (0, 1))]);
    assert_eq!(ranges(&only_zero), vec![0..7]);

    // 相対位置 2 の係数は 0 になり、相対位置 1 の項だけが残る。
    let partial = compile("[->+>+-<<]");
    assert_eq!(kinds(&partial), vec![transfer(&[(1, 1)], 8, (0, 2))]);
    assert_eq!(ranges(&partial), vec![0..10]);
}

#[test]
fn start_cell_net_change_must_be_minus_one_for_transfer() {
    // 開始セルの正味変化が -1 の本体は移動/加算/乗算になる(係数 0 の項は除外される)。
    let no_terms = compile("[-+-]");
    assert_eq!(kinds(&no_terms), vec![transfer(&[], 3, (0, 0))]);
    assert_eq!(ranges(&no_terms), vec![0..5]);

    let shifted = compile("[>+<-]");
    assert_eq!(kinds(&shifted), vec![transfer(&[(1, 1)], 4, (0, 1))]);
    assert_eq!(ranges(&shifted), vec![0..6]);
}

#[test]
fn start_cell_net_change_other_than_minus_one_stays_general() {
    // 開始セルの正味変化が +1 のため一般ループのまま。
    let source = "[+>+<]";
    let optimized = compile(source);
    assert!(
        matches!(optimized.ops()[0].kind(), OpKind::LoopStart { .. }),
        "{source} should stay general, got {:?}",
        kinds(&optimized)
    );
}

#[test]
fn balanced_moves_without_start_cell_change_stay_general() {
    // ポインタの正味移動は 0 だが、開始セルの正味変化が 0 のため一般ループのまま。
    let source = "[<>]";
    let optimized = compile(source);
    assert_eq!(
        kinds(&optimized),
        vec![
            OpKind::LoopStart { end: 3 },
            OpKind::Move(-1),
            OpKind::Move(1),
            OpKind::LoopEnd { start: 0 },
        ]
    );
    assert_eq!(ranges(&optimized), vec![0..1, 1..2, 2..3, 3..4]);
}

#[test]
fn transfer_reach_spans_negative_and_positive_offsets() {
    // 左へ 1 進んで加算し、右へ 2 進んで加算して戻る。到達範囲は -1..=1 になる。
    let optimized = compile("[-<+>>+<]");
    assert_eq!(
        kinds(&optimized),
        vec![transfer(&[(-1, 1), (1, 1)], 7, (-1, 1))]
    );
    assert_eq!(ranges(&optimized), vec![0..9]);
}

#[test]
fn scan_loops_record_direction_and_moves_per_iteration() {
    for (source, stride) in [("[>]", 1), ("[<]", -1), ("[>>]", 2), ("[<<<]", -3)] {
        let optimized = compile(source);
        assert_eq!(kinds(&optimized), vec![OpKind::Scan { stride }], "{source}");
        assert_eq!(ranges(&optimized), vec![0..source.len()], "{source}");
    }
}

#[test]
fn non_matching_loops_stay_general() {
    // 順に: 正味変化が -2、ポインタ正味非 0、出力を含む、入力を含む、
    // 逆向き移動のみで開始セルが変化しない、ポインタ正味 1、本体が空。
    for source in ["[--]", "[>.]", "[,]", "[><]", "[->+]", "[]"] {
        let optimized = compile(source);
        assert!(
            matches!(optimized.ops()[0].kind(), OpKind::LoopStart { .. }),
            "{source} should stay general, got {:?}",
            kinds(&optimized)
        );
        assert!(
            matches!(
                optimized.ops().last().unwrap().kind(),
                OpKind::LoopEnd { .. }
            ),
            "{source}"
        );
    }
}

#[test]
fn outer_loop_stays_general_while_inner_clear_is_optimized() {
    let optimized = compile("[>[-]<-]");
    assert_eq!(
        kinds(&optimized),
        vec![
            OpKind::LoopStart { end: 5 },
            OpKind::Move(1),
            OpKind::Clear { increments: false },
            OpKind::Move(-1),
            OpKind::Add(255),
            OpKind::LoopEnd { start: 0 },
        ]
    );
    assert_eq!(ranges(&optimized), vec![0..1, 1..2, 2..5, 5..6, 6..7, 7..8]);
}

#[test]
fn loop_partner_indices_account_for_merged_operations() {
    // `[+-]` の本体は 1 操作(Add(0))に統合されるが、括弧の対応先は IR 添字で示す。
    let optimized = compile("+[+-]+");
    assert_eq!(
        kinds(&optimized),
        vec![
            OpKind::Add(1),
            OpKind::LoopStart { end: 3 },
            OpKind::Add(0),
            OpKind::LoopEnd { start: 1 },
            OpKind::Add(1),
        ]
    );
    assert_eq!(ranges(&optimized), vec![0..1, 1..2, 2..4, 4..5, 5..6]);
}

#[test]
fn source_ranges_are_contiguous_and_cover_the_program() {
    let sources = [
        "",
        "+++--",
        ">>><<[-]..,",
        "[.[.]]",
        "+[>>+<-]>[<]<<,.",
        "><><[]",
    ];
    for source in sources {
        let program = Program::parse(source).expect("valid program");
        let optimized = OptimizedProgram::compile(&program);
        let mut expected_start = 0;
        for op in optimized.ops() {
            let range = op.source_range();
            assert_eq!(range.start, expected_start, "gap or overlap in {source:?}");
            assert!(range.end > range.start, "empty range in {source:?}");
            expected_start = range.end;
        }
        assert_eq!(expected_start, program.len(), "coverage of {source:?}");
    }
}

#[test]
fn operation_count_does_not_exceed_instruction_count() {
    let sources = ["+-+-+-", "><><><", "[][][]", ".,.,.,", "+++[-]>>>"];
    for source in sources {
        let program = Program::parse(source).expect("valid program");
        let optimized = OptimizedProgram::compile(&program);
        assert!(optimized.ops().len() <= program.len(), "{source:?}");
    }
}

#[test]
fn compiling_the_same_program_twice_gives_the_same_result() {
    let first = compile("+[>>+<-]>[<]<<,.");
    let second = compile("+[>>+<-]>[<]<<,.");
    assert_eq!(first, second);
}

#[test]
fn empty_program_compiles_to_no_operations() {
    let optimized = compile("");
    assert!(optimized.ops().is_empty());
    assert!(optimized.source().is_empty());
}

#[test]
fn compiled_program_keeps_the_parsed_source() {
    let program = Program::parse("+[-].").expect("valid program");
    let optimized = OptimizedProgram::compile(&program);
    assert_eq!(optimized.source(), &program);
}
