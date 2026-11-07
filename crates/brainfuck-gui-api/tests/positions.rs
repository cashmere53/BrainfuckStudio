//! 位置・範囲・解析エラーの DTO 写像(`convert` の位置系関数)の契約。
//!
//! 位置規則そのもの(行・列・UTF-16 の数え方)の一貫性は `brainfuck-core` 側のテストが保証する。
//! ここでは上流の値を DTO へフィールドごとに写すこと(取り違えがないこと)だけを検査する。

use brainfuck_core::{ParseError, Program, SourceMap, SourcePosition};
use brainfuck_gui_api::convert::{parse_error_dto, source_position_dto, source_range_dto};
use brainfuck_gui_api::dto::{ApiErrorKind, ParseErrorKind, SourcePositionDto};

/// ASCII・`あ`(3 バイト)・`😀`(BMP 外、UTF-16 2 単位)・`\n` / `\r\n` / `\r` を含む括弧つきソース。
const BALANCED_SOURCES: &[&str] = &[
    "+-<>.,[]",
    "あ+[い]う",
    "😀+😀[>]😀\n",
    "a\nb\r\nc\rd+[\n]",
    "\r\n+\r\n\r[-]\r",
    "あ😀\r\n+[-]",
];

/// 上流の位置を DTO の期待値へ写す(テスト側の写し)。
fn expected(p: SourcePosition) -> SourcePositionDto {
    SourcePositionDto {
        byte_offset: p.byte_offset,
        utf16_offset: p.utf16_offset,
        line: p.line,
        column: p.column,
    }
}

/// 文字境界の全オフセットと末尾オフセットを列挙する。
fn boundaries(source: &str) -> Vec<usize> {
    source
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(source.len()))
        .collect()
}

#[test]
fn source_position_dto_copies_all_four_fields_at_every_boundary() {
    for source in BALANCED_SOURCES {
        for offset in boundaries(source) {
            let position = SourceMap::position_at(source, offset)
                .unwrap_or_else(|| panic!("boundary {offset} of {source:?} must have a position"));
            let dto = source_position_dto(&position);
            assert_eq!(dto, expected(position), "source {source:?} offset {offset}");
        }
    }
}

#[test]
fn source_position_dto_has_the_expected_values_for_multibyte_and_crlf() {
    // "あ" 3 バイト / 1 単位 / 列 2、"+" は 1 バイト、"😀" 4 バイト / 2 単位、"\r\n" 2 バイト。
    // 次の行の "[" は行 2・列 1・UTF-16 は 1 + 1 + 2 + 2 = 6。
    let source = "あ+😀\r\n[";
    let plus = SourceMap::position_at(source, 3).expect("plus position");
    assert_eq!(
        source_position_dto(&plus),
        SourcePositionDto {
            byte_offset: 3,
            utf16_offset: 1,
            line: 1,
            column: 2,
        }
    );
    let bracket = SourceMap::position_at(source, 10).expect("bracket position");
    assert_eq!(
        source_position_dto(&bracket),
        SourcePositionDto {
            byte_offset: 10,
            utf16_offset: 6,
            line: 2,
            column: 1,
        }
    );
}

#[test]
fn source_range_dto_copies_every_instruction_range() {
    for source in BALANCED_SOURCES {
        let program = Program::parse(source).expect("balanced source must parse");
        let map = SourceMap::new(&program, source).expect("source map must build");
        for index in 0..map.instruction_count() {
            let range = map
                .range_of(index)
                .expect("range of an existing instruction");
            let dto = source_range_dto(&range);
            assert_eq!(
                dto.start,
                expected(range.start),
                "{source:?} #{index} start"
            );
            assert_eq!(dto.end, expected(range.end), "{source:?} #{index} end");
        }
    }
}

#[test]
fn source_range_dto_has_the_expected_values_for_a_multibyte_instruction() {
    // "あ+" の "+" は範囲 [3, 4) / UTF-16 [1, 2) / 列 2〜3。
    let source = "あ+";
    let program = Program::parse(source).expect("parse");
    let map = SourceMap::new(&program, source).expect("map");
    let dto = source_range_dto(&map.range_of(0).expect("range"));
    assert_eq!(
        dto.start,
        SourcePositionDto {
            byte_offset: 3,
            utf16_offset: 1,
            line: 1,
            column: 2,
        }
    );
    assert_eq!(
        dto.end,
        SourcePositionDto {
            byte_offset: 4,
            utf16_offset: 2,
            line: 1,
            column: 3,
        }
    );
}

#[test]
fn parse_error_dto_maps_kind_index_and_position_of_an_unmatched_closing_bracket() {
    // "あ😀\r\n+]": "]" は命令添字 1、バイト 10、UTF-16 6、行 2・列 2。
    let source = "あ😀\r\n+]";
    let error = Program::parse(source).expect_err("unmatched closing bracket");
    let dto = parse_error_dto(source, &error).expect("dto");
    assert_eq!(dto.kind, ParseErrorKind::UnmatchedClosingBracket);
    assert_eq!(dto.instruction_index, 1);
    assert_eq!(
        dto.position,
        SourcePositionDto {
            byte_offset: 10,
            utf16_offset: 6,
            line: 2,
            column: 2,
        }
    );
}

#[test]
fn parse_error_dto_maps_an_unmatched_opening_bracket_at_the_source_start() {
    let source = "[😀\n+";
    let error = Program::parse(source).expect_err("unmatched opening bracket");
    let dto = parse_error_dto(source, &error).expect("dto");
    assert_eq!(dto.kind, ParseErrorKind::UnmatchedOpeningBracket);
    assert_eq!(dto.instruction_index, 0);
    assert_eq!(
        dto.position,
        SourcePositionDto {
            byte_offset: 0,
            utf16_offset: 0,
            line: 1,
            column: 1,
        }
    );
}

#[test]
fn parse_error_dto_handles_an_unmatched_opening_bracket_at_the_source_end() {
    // 末尾の "[" がエラー位置。ソース末尾(バイト長と等しい位置)も DTO になる。
    let source = "+\n😀[";
    let error = Program::parse(source).expect_err("unmatched opening bracket");
    let dto = parse_error_dto(source, &error).expect("dto");
    assert_eq!(dto.kind, ParseErrorKind::UnmatchedOpeningBracket);
    assert_eq!(dto.instruction_index, 1);
    assert_eq!(
        dto.position,
        SourcePositionDto {
            byte_offset: 6,
            utf16_offset: 4,
            line: 2,
            column: 2,
        }
    );
    let end = SourceMap::position_at(source, source.len()).expect("end position");
    assert_eq!(
        source_position_dto(&end),
        SourcePositionDto {
            byte_offset: 7,
            utf16_offset: 5,
            line: 2,
            column: 3,
        }
    );
}

#[test]
fn parse_error_dto_position_equals_the_upstream_position_at() {
    let sources = ["+\r[", "]\n", "あ\r\n\r\n[-]]", "😀😀\r+[\r\n]]"];
    for source in sources {
        let error = Program::parse(source).expect_err("unbalanced source");
        let offset = match error {
            ParseError::UnmatchedClosingBracket { source_offset, .. }
            | ParseError::UnmatchedOpeningBracket { source_offset, .. } => source_offset,
            _ => panic!("unexpected parse error variant for {source:?}"),
        };
        let dto = parse_error_dto(source, &error).expect("dto");
        let upstream = SourceMap::position_at(source, offset).expect("upstream position");
        assert_eq!(dto.position, expected(upstream), "{source:?}");
    }
}

#[test]
fn parse_error_dto_is_internal_when_the_position_cannot_be_resolved() {
    // エラーは長いソースから得て、短いソースで写すと位置が取得できない(内部エラー)。
    let source = "\n\n\n+[";
    let error = Program::parse(source).expect_err("unmatched opening bracket");
    let result = parse_error_dto("x", &error);
    let err = result.expect_err("position must not resolve against a shorter source");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.field, None);
    assert_eq!(err.message, "internal error");
}
