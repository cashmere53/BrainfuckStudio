//! 解析(`Program`)の統合テスト。crate ルート直下のパスのみを使う。

use brainfuck_core::{Instruction, ParseError, Program};

#[test]
fn extracts_only_instruction_characters_in_source_order() {
    let program = Program::parse("a+b>c").expect("括弧を含まないので解析は成功する");
    assert_eq!(
        program.instructions(),
        &[Instruction::Increment, Instruction::MoveRight]
    );
    assert_eq!(program.len(), 2);
}

#[test]
fn all_eight_characters_are_extracted_in_source_order() {
    let program = Program::parse("><+-.,[]").expect("解析は成功する");
    assert_eq!(
        program.instructions(),
        &[
            Instruction::MoveRight,
            Instruction::MoveLeft,
            Instruction::Increment,
            Instruction::Decrement,
            Instruction::Output,
            Instruction::Input,
            Instruction::LoopStart,
            Instruction::LoopEnd,
        ]
    );
    assert_eq!(program.source_offsets(), &[0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn comments_are_ignored_and_not_instructions() {
    // 改行・空白・タブ・英字・数字は命令にならない。
    let source = "hello\n  + world\t-\n";
    let program = Program::parse(source).expect("解析は成功する");
    assert_eq!(
        program.instructions(),
        &[Instruction::Increment, Instruction::Decrement]
    );
    // '+' は 8 番目、'-' は 16 番目のバイト位置にある。
    assert_eq!(program.source_offsets(), &[8, 16]);
}

#[test]
fn multibyte_characters_are_comments_and_offsets_are_byte_positions() {
    // 「あ」は UTF-8 で 3 バイト。文字数ではなくバイト位置で報告する。
    let program = Program::parse("あ+").expect("解析は成功する");
    assert_eq!(program.instructions(), &[Instruction::Increment]);
    assert_eq!(program.source_offset(0), Some(3));
}

#[test]
fn offsets_skip_multibyte_comments_between_instructions() {
    // "あい-" → 「あ」「い」で 6 バイト、'-' は 6 番目。
    let program = Program::parse("あい-+").expect("解析は成功する");
    assert_eq!(
        program.instructions(),
        &[Instruction::Decrement, Instruction::Increment]
    );
    assert_eq!(program.source_offsets(), &[6, 7]);
}

#[test]
fn offsets_are_strictly_increasing() {
    let program = Program::parse("+ 日本語 [-] \n>.<,").expect("解析は成功する");
    let offsets = program.source_offsets();
    assert_eq!(offsets.len(), program.len());
    assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn empty_source_yields_zero_instructions() {
    let program = Program::parse("").expect("空入力は正常な解析結果");
    assert_eq!(program.len(), 0);
    assert!(program.is_empty());
    assert!(program.instructions().is_empty());
    assert!(program.source_offsets().is_empty());
}

#[test]
fn comment_only_source_yields_zero_instructions() {
    let program = Program::parse("hello world 日本語\n\t").expect("コメントのみでも成功する");
    assert_eq!(program.len(), 0);
    assert!(program.is_empty());
}

#[test]
fn out_of_range_instruction_index_returns_none() {
    let program = Program::parse("+-").expect("解析は成功する");
    assert_eq!(program.source_offset(0), Some(0));
    assert_eq!(program.source_offset(1), Some(1));
    assert_eq!(program.source_offset(2), None);
    assert_eq!(program.source_offset(usize::MAX), None);
}

#[test]
fn out_of_range_index_on_empty_program_returns_none() {
    let program = Program::parse("").expect("空入力は成功する");
    assert_eq!(program.source_offset(0), None);
}

#[test]
fn every_byte_from_0x80_to_0xff_is_a_comment_in_byte_input() {
    for byte in 0x80u8..=0xFF {
        let program = Program::parse_bytes(&[byte])
            .unwrap_or_else(|error| panic!("byte {byte:#04x} で失敗した: {error:?}"));
        assert_eq!(program.len(), 0, "byte {byte:#04x} は命令ではない");
    }
}

#[test]
fn full_high_byte_range_in_one_input_is_all_comments() {
    let bytes: Vec<u8> = (0x80u8..=0xFF).collect();
    let program = Program::parse_bytes(&bytes).expect("0xFF を含んでも失敗しない");
    assert!(program.is_empty());
}

#[test]
fn non_utf8_bytes_are_skipped_and_offsets_are_input_byte_positions() {
    // 0xFF と 0xFE は UTF-8 として不正だが、コメントとして無視される。
    let program = Program::parse_bytes(&[0xFF, b'+', 0xFE, b'-']).expect("解析は成功する");
    assert_eq!(
        program.instructions(),
        &[Instruction::Increment, Instruction::Decrement]
    );
    assert_eq!(program.source_offsets(), &[1, 3]);
}

#[test]
fn byte_input_and_str_input_agree_on_valid_utf8() {
    let samples = [
        "",
        "+",
        "a+b>c",
        "あ+い-",
        "[+]\n>.<,",
        "日本語 +++ 言語 ---",
        "+[>+<-]\n.",
    ];
    for sample in samples {
        assert_eq!(
            Program::parse(sample),
            Program::parse_bytes(sample.as_bytes()),
            "sample {sample:?}"
        );
    }
}

#[test]
fn str_input_and_byte_input_produce_same_instructions_and_offsets() {
    let source = "あ+[い-]\n>";
    let from_str = Program::parse(source).expect("解析は成功する");
    let from_bytes = Program::parse_bytes(source.as_bytes()).expect("解析は成功する");
    assert_eq!(from_str.instructions(), from_bytes.instructions());
    assert_eq!(from_str.source_offsets(), from_bytes.source_offsets());
    assert_eq!(from_str.len(), from_bytes.len());
}

#[test]
fn simple_pair_matches_each_other() {
    // 0: '[', 1: '-', 2: ']'
    let program = Program::parse("[-]").expect("括弧は対応している");
    assert_eq!(program.matching_bracket(0), Some(2));
    assert_eq!(program.matching_bracket(2), Some(0));
}

#[test]
fn empty_loop_pair_matches_each_other() {
    // 0: '[', 1: ']'
    let program = Program::parse("[]").expect("括弧は対応している");
    assert_eq!(program.matching_bracket(0), Some(1));
    assert_eq!(program.matching_bracket(1), Some(0));
}

#[test]
fn nested_pairs_match_from_inside_out() {
    // 0: '[', 1: '[', 2: '-', 3: ']', 4: '+', 5: ']'
    let program = Program::parse("[[-]+]").expect("括弧は対応している");
    assert_eq!(program.matching_bracket(0), Some(5));
    assert_eq!(program.matching_bracket(1), Some(3));
    assert_eq!(program.matching_bracket(3), Some(1));
    assert_eq!(program.matching_bracket(5), Some(0));
}

#[test]
fn parallel_pairs_match_independently() {
    // 0: '[', 1: '-', 2: ']', 3: '[', 4: '+', 5: ']'
    let program = Program::parse("[-][+]").expect("括弧は対応している");
    assert_eq!(program.matching_bracket(0), Some(2));
    assert_eq!(program.matching_bracket(2), Some(0));
    assert_eq!(program.matching_bracket(3), Some(5));
    assert_eq!(program.matching_bracket(5), Some(3));
}

#[test]
fn matching_bracket_is_symmetric_for_every_bracket() {
    let program = Program::parse("+[>[-]<+]>[[]]").expect("括弧は対応している");
    for (index, instruction) in program.instructions().iter().enumerate() {
        match instruction {
            Instruction::LoopStart | Instruction::LoopEnd => {
                let partner = program
                    .matching_bracket(index)
                    .unwrap_or_else(|| panic!("括弧 {index} に対応先がない"));
                assert_eq!(program.matching_bracket(partner), Some(index));
            }
            _ => assert_eq!(program.matching_bracket(index), None),
        }
    }
}

#[test]
fn non_bracket_and_out_of_range_queries_return_none() {
    let program = Program::parse("[+]").expect("括弧は対応している");
    assert_eq!(program.matching_bracket(1), None);
    assert_eq!(program.matching_bracket(3), None);
    assert_eq!(program.matching_bracket(usize::MAX), None);
    let empty = Program::parse("").expect("空入力は成功する");
    assert_eq!(empty.matching_bracket(0), None);
}

#[test]
fn lone_closing_bracket_is_reported_at_first_position() {
    assert_eq!(
        Program::parse("]"),
        Err(ParseError::UnmatchedClosingBracket {
            instruction_index: 0,
            source_offset: 0,
        })
    );
}

#[test]
fn extra_closing_bracket_after_pair_is_reported() {
    // 0: '[', 1: ']', 2: ']'(対応する '[' が無い)
    assert_eq!(
        Program::parse("[]]"),
        Err(ParseError::UnmatchedClosingBracket {
            instruction_index: 2,
            source_offset: 2,
        })
    );
}

#[test]
fn closing_bracket_is_reported_before_later_unmatched_opening() {
    // 最初に見つかるのは走査中の ']'(添字 0)。'[' の不一致より優先される。
    assert_eq!(
        Program::parse("][["),
        Err(ParseError::UnmatchedClosingBracket {
            instruction_index: 0,
            source_offset: 0,
        })
    );
}

#[test]
fn surplus_closing_bracket_after_nested_pairs_is_reported_at_scan_position() {
    // "[[]]]" の 4 番目(添字 4)が最初の不一致。
    assert_eq!(
        Program::parse("[[]]]"),
        Err(ParseError::UnmatchedClosingBracket {
            instruction_index: 4,
            source_offset: 4,
        })
    );
}

#[test]
fn double_opening_bracket_reports_first_unresolved_opening() {
    // 0: '[', 1: '['。両方が未解決で、最初の '[' を報告する。
    assert_eq!(
        Program::parse("[["),
        Err(ParseError::UnmatchedOpeningBracket {
            instruction_index: 0,
            source_offset: 0,
        })
    );
}

#[test]
fn partially_closed_openings_report_earliest_unresolved_opening() {
    // "[[]" では '[' (添字 1) が閉じ、添字 0 の '[' が残る。
    assert_eq!(
        Program::parse("[[]"),
        Err(ParseError::UnmatchedOpeningBracket {
            instruction_index: 0,
            source_offset: 0,
        })
    );
}

#[test]
fn earliest_unresolved_opening_is_chosen_over_later_ones() {
    // 0: '+', 1: '[' (offset 1), 2: '-', 3: '[' (offset 3), 4: ']' (offset 4)
    // 解決されるのは添字 3 の '['。残る未解決は添字 1 の '[' のみ。
    assert_eq!(
        Program::parse("+[-[]"),
        Err(ParseError::UnmatchedOpeningBracket {
            instruction_index: 1,
            source_offset: 1,
        })
    );
}

#[test]
fn bracket_errors_are_identical_for_str_and_byte_input() {
    for sample in ["]", "[]]", "[[", "[[]", "][", "+[-[]"] {
        assert_eq!(
            Program::parse(sample),
            Program::parse_bytes(sample.as_bytes()),
            "sample {sample:?}"
        );
    }
}

#[test]
fn closing_bracket_after_non_utf8_bytes_reports_input_byte_offset() {
    // 0xFF(offset 0), '+'(offset 1), 0xFE(offset 2), ']'(offset 3)
    assert_eq!(
        Program::parse_bytes(&[0xFF, b'+', 0xFE, b']']),
        Err(ParseError::UnmatchedClosingBracket {
            instruction_index: 1,
            source_offset: 3,
        })
    );
}

#[test]
fn opening_bracket_after_non_utf8_bytes_reports_input_byte_offset() {
    // 0x80(offset 0), '['(offset 1), 0xFF(offset 2)
    assert_eq!(
        Program::parse_bytes(&[0x80, b'[', 0xFF]),
        Err(ParseError::UnmatchedOpeningBracket {
            instruction_index: 0,
            source_offset: 1,
        })
    );
}

#[test]
fn closing_bracket_after_multibyte_text_reports_byte_offset() {
    // 「あ」は 3 バイトなので ']' のバイト位置は 3。
    assert_eq!(
        Program::parse("あ]"),
        Err(ParseError::UnmatchedClosingBracket {
            instruction_index: 0,
            source_offset: 3,
        })
    );
}
