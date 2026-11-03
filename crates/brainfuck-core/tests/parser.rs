//! 解析(`Program`)の統合テスト。crate ルート直下のパスのみを使う。

use brainfuck_core::{Instruction, Program};

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
