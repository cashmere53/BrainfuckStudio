//! 変換器(`OptimizedProgram::compile`)の公開 API を通した検証。
//!
//! 2.2 の範囲: 加減算・移動の統合、入出力の 1 命令 1 操作、一般ループの括弧対応。
//! 代表的ループの認識(2.3)はまだ行わないため、`[-]` なども一般ループのままになる。

use brainfuck_core::Program;
use brainfuck_optimized::{Op, OpKind, OptimizedProgram};

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

#[test]
fn loops_stay_general_at_this_stage() {
    let optimized = compile("[-]");
    assert_eq!(
        kinds(&optimized),
        vec![
            OpKind::LoopStart { end: 2 },
            OpKind::Add(255),
            OpKind::LoopEnd { start: 0 },
        ]
    );
    assert_eq!(ranges(&optimized), vec![0..1, 1..2, 2..3]);
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
