//! 位置変換(`SourceMap`)の単体テスト。
//!
//! ソース文字列だけから位置を返す `SourceMap::position_at` と、解析済みプログラムから作る
//! `SourceMap::new` の全命令分の範囲が、同じ規則(バイト・行・列・UTF-16)で一致することを確認する。
//! 行・列の規則: 行区切りは `\n`・`\r\n`(1 個)・`\r`、列はスカラ値の個数 + 1、UTF-16 はコード単位数。

use brainfuck_core::{Program, SourceMap, SourceMapError, SourcePosition, SourceRange};

/// 位置を (バイト, 行, 列, UTF-16) の組として表す。
///
/// `SourcePosition` は非網羅的なため、テストでは構造体リテラルを使わず、フィールドを読んで比較する。
type Key = (usize, usize, usize, usize);

fn key(position: SourcePosition) -> Key {
    (
        position.byte_offset,
        position.line,
        position.column,
        position.utf16_offset,
    )
}

/// 期待する位置を (バイト, 行, 列, UTF-16) で書く。
fn at(byte_offset: usize, line: usize, column: usize, utf16_offset: usize) -> Option<Key> {
    Some((byte_offset, line, column, utf16_offset))
}

/// 位置変換の結果(ソース文字列のみから)を比較用の組へ変換する。
fn position_key(source: &str, byte_offset: usize) -> Option<Key> {
    SourceMap::position_at(source, byte_offset).map(key)
}

/// 範囲の始点と終点を比較用の組として返す。
fn range_keys(range: SourceRange) -> (Key, Key) {
    (key(range.start), key(range.end))
}

/// 解析済みプログラムと位置変換情報を作る。解析・構築の失敗はテストを失敗させる。
fn map_of(source: &str) -> (Program, SourceMap) {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    let map = SourceMap::new(&program, source).expect("テスト用ソースは整合している必要がある");
    (program, map)
}

// --- SourceMap::position_at(ソース文字列のみ) ---

#[test]
fn position_at_ascii_text_counts_bytes_columns_and_utf16_units() {
    let source = "+-.";
    assert_eq!(position_key(source, 0), at(0, 1, 1, 0));
    assert_eq!(position_key(source, 2), at(2, 1, 3, 2));
}

#[test]
fn position_at_japanese_character_has_three_bytes_two_columns_and_one_utf16_unit() {
    // `あ` は 3 バイト・1 スカラ値・UTF-16 1 単位。
    let source = "あ+";
    assert_eq!(position_key(source, 3), at(3, 1, 2, 1));
    assert_eq!(position_key(source, 4), at(4, 1, 3, 2));
}

#[test]
fn position_at_surrogate_pair_has_four_bytes_one_column_and_two_utf16_units() {
    // `😀` は 4 バイト・1 スカラ値・UTF-16 2 単位。
    let source = "😀+";
    assert_eq!(position_key(source, 4), at(4, 1, 2, 2));
    assert_eq!(position_key(source, 5), at(5, 1, 3, 3));
}

#[test]
fn position_at_lf_starts_the_next_line_at_column_one() {
    let source = "a\nb";
    assert_eq!(position_key(source, 1), at(1, 1, 2, 1));
    assert_eq!(position_key(source, 2), at(2, 2, 1, 2));
    assert_eq!(position_key(source, 3), at(3, 2, 2, 3));
}

#[test]
fn position_at_crlf_is_one_line_break_and_the_gap_belongs_to_the_cr_line() {
    let source = "a\r\nb";
    // `\r` の位置は 1 行目の列 2。
    assert_eq!(position_key(source, 1), at(1, 1, 2, 1));
    // `\r` と `\n` の間は直前の `\r` と同じ行で、列は `\r` の列 + 1、UTF-16 も `\r` の位置 + 1。
    assert_eq!(position_key(source, 2), at(2, 1, 3, 2));
    // `\r\n` の直後は次の行の列 1。
    assert_eq!(position_key(source, 3), at(3, 2, 1, 3));
    assert_eq!(position_key(source, 4), at(4, 2, 2, 4));
}

#[test]
fn position_at_cr_alone_is_a_line_break() {
    let source = "a\rb";
    assert_eq!(position_key(source, 2), at(2, 2, 1, 2));
    assert_eq!(position_key(source, 3), at(3, 2, 2, 3));
}

#[test]
fn position_at_source_end_is_valid() {
    assert_eq!(position_key("ab", 2), at(2, 1, 3, 2));
    assert_eq!(position_key("ab\n", 3), at(3, 2, 1, 3));
}

#[test]
fn position_at_beyond_source_end_is_none() {
    assert_eq!(position_key("ab", 3), None);
    assert_eq!(position_key("", 1), None);
}

#[test]
fn position_at_inside_a_multibyte_character_is_none() {
    // `あ` の 2 バイト目と 3 バイト目は文字境界ではない。
    assert_eq!(position_key("あ", 1), None);
    assert_eq!(position_key("あ", 2), None);
    // `😀` の途中も同様。
    assert_eq!(position_key("😀", 2), None);
}

#[test]
fn position_at_empty_source_is_the_origin_only() {
    assert_eq!(position_key("", 0), at(0, 1, 1, 0));
}

// --- SourceMap::new と range_of ---

#[test]
fn ascii_instruction_ranges_are_half_open_one_byte_spans() {
    let (_program, map) = map_of("+>-");
    assert_eq!(map.instruction_count(), 3);
    let range = map.range_of(1).expect("命令 1 は存在する");
    assert_eq!(
        range_keys(range),
        (at(1, 1, 2, 1).unwrap(), at(2, 1, 3, 2).unwrap())
    );
    assert_eq!(map.range_of(3), None);
}

#[test]
fn japanese_prefix_moves_the_instruction_to_byte_three_column_two_and_utf16_one() {
    let (_program, map) = map_of("あ+");
    let range = map.range_of(0).expect("命令 0 は存在する");
    assert_eq!(
        range_keys(range),
        (at(3, 1, 2, 1).unwrap(), at(4, 1, 3, 2).unwrap())
    );
}

#[test]
fn surrogate_pair_prefix_moves_the_instruction_to_byte_four_column_two_and_utf16_two() {
    let (_program, map) = map_of("😀+");
    let range = map.range_of(0).expect("命令 0 は存在する");
    assert_eq!(
        range_keys(range),
        (at(4, 1, 2, 2).unwrap(), at(5, 1, 3, 3).unwrap())
    );
}

#[test]
fn lf_line_and_column_of_an_instruction_after_a_break() {
    let (_program, map) = map_of("+\n-");
    let range = map.range_of(1).expect("命令 1 は存在する");
    assert_eq!(
        range_keys(range),
        (at(2, 2, 1, 2).unwrap(), at(3, 2, 2, 3).unwrap())
    );
}

#[test]
fn crlf_counts_as_one_line_break_for_instructions() {
    let (_program, map) = map_of("+\r\n-");
    let range = map.range_of(1).expect("命令 1 は存在する");
    assert_eq!(
        range_keys(range),
        (at(3, 2, 1, 3).unwrap(), at(4, 2, 2, 4).unwrap())
    );
}

#[test]
fn cr_alone_counts_as_a_line_break_for_instructions() {
    let (_program, map) = map_of("+\r-");
    let range = map.range_of(1).expect("命令 1 は存在する");
    assert_eq!(
        range_keys(range),
        (at(2, 2, 1, 2).unwrap(), at(3, 2, 2, 3).unwrap())
    );
}

#[test]
fn instruction_spanning_several_lines_keeps_its_own_line_and_column() {
    // `[` と `]` の間に改行とコメントが挟まる。
    let (_program, map) = map_of("[\n\n  ]");
    let open = map.range_of(0).expect("命令 0 は存在する");
    assert_eq!(key(open.start), at(0, 1, 1, 0).unwrap());
    let close = map.range_of(1).expect("命令 1 は存在する");
    // `[\n\n  ]` の `]` はバイト 5、3 行目の列 3、UTF-16 5。
    assert_eq!(
        range_keys(close),
        (at(5, 3, 3, 5).unwrap(), at(6, 3, 4, 6).unwrap())
    );
}

#[test]
fn range_of_instruction_matches_position_at_its_offset_for_all_instructions() {
    let source = "a\u{3042}+\r\n\u{1F600}[-]\r>\n.";
    let (program, map) = map_of(source);
    assert_eq!(map.instruction_count(), program.len());
    for index in 0..map.instruction_count() {
        let offset = program
            .source_offset(index)
            .expect("範囲内の命令には元のオフセットがある");
        let range = map.range_of(index).expect("範囲内の命令には範囲がある");
        assert_eq!(range.start.byte_offset, offset);
        assert_eq!(range.end.byte_offset, offset + 1);
        assert_eq!(position_key(source, offset), Some(key(range.start)));
    }
}

#[test]
fn comment_only_source_has_no_instructions_and_no_ranges() {
    let (_program, map) = map_of("abc\n\u{3042}");
    assert_eq!(map.instruction_count(), 0);
    assert_eq!(map.range_of(0), None);
}

#[test]
fn empty_source_builds_and_answers_none() {
    let (program, map) = map_of("");
    assert!(program.is_empty());
    assert_eq!(map.instruction_count(), 0);
    assert_eq!(map.range_of(0), None);
}

#[test]
fn range_of_out_of_range_index_is_none() {
    let (_program, map) = map_of("+-");
    assert_eq!(map.range_of(2), None);
    assert_eq!(map.range_of(usize::MAX), None);
}

#[test]
fn source_map_can_move_across_threads() {
    fn assert_send<T: Send>() {}
    assert_send::<SourceMap>();
}

#[test]
fn line_count_is_break_count_plus_one_and_one_for_empty_source() {
    assert_eq!(map_of("").1.line_count(), 1);
    assert_eq!(map_of("+").1.line_count(), 1);
    assert_eq!(map_of("+\n-").1.line_count(), 2);
    assert_eq!(map_of("+\r\n-\r\n").1.line_count(), 3);
    assert_eq!(map_of("+\r-\r").1.line_count(), 3);
}

#[test]
fn position_at_agrees_with_range_start_for_every_instruction_in_mixed_source() {
    let source = "x😀\r\n+あ\r-\n[\n]";
    let (program, map) = map_of(source);
    assert_eq!(map.instruction_count(), 4);
    for index in 0..map.instruction_count() {
        let offset = program.source_offset(index).expect("範囲内");
        let range = map.range_of(index).expect("範囲内");
        assert_eq!(position_key(source, offset), Some(key(range.start)));
    }
}

// --- 整合検証(不整合ソースの拒否) ---

#[test]
fn a_missing_instruction_in_the_source_is_rejected() {
    // プログラムは 2 命令、ソースには 1 命令しかない。
    let program = Program::parse("+-").expect("valid");
    let error = SourceMap::new(&program, "+").expect_err("欠落は拒否される");
    assert_eq!(
        error,
        SourceMapError::InstructionMismatch {
            instruction_index: 1
        }
    );
}

#[test]
fn a_different_character_at_the_same_offset_is_rejected() {
    let program = Program::parse("+").expect("valid");
    let error = SourceMap::new(&program, "-").expect_err("別文字は拒否される");
    assert_eq!(
        error,
        SourceMapError::InstructionMismatch {
            instruction_index: 0
        }
    );
}

#[test]
fn an_extra_instruction_in_the_source_is_rejected() {
    // プログラムは `+` だけ、ソースには余分な `-` がある。
    let program = Program::parse("+").expect("valid");
    let error = SourceMap::new(&program, "+-").expect_err("余剰は拒否される");
    assert_eq!(
        error,
        SourceMapError::UnexpectedInstruction { source_offset: 1 }
    );
}

#[test]
fn mismatch_error_text_contains_only_numbers_and_no_source_text() {
    // ソースの非 ASCII 文字列(機微情報を想定)を含め、エラー表示にソース全文が出ないことを確認する。
    let program = Program::parse("+").expect("valid");
    let error = SourceMap::new(&program, "秘密+").expect_err("位置の不一致は拒否される");
    assert_eq!(
        error,
        SourceMapError::InstructionMismatch {
            instruction_index: 0
        }
    );
    let text = error.to_string();
    assert!(text.is_ascii(), "text: {text}");
    assert!(!text.contains("秘密"), "text: {text}");
    assert!(text.contains('0'), "text: {text}");
}

#[test]
fn unexpected_error_text_contains_the_offset_number() {
    let program = Program::parse("+").expect("valid");
    let error = SourceMap::new(&program, "+-").expect_err("余剰は拒否される");
    let text = error.to_string();
    assert!(text.contains('1'), "text: {text}");
    assert!(!text.contains('+') && !text.contains('-'), "text: {text}");
}

#[test]
fn source_map_error_is_a_std_error() {
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}
    assert_error::<SourceMapError>();
}
