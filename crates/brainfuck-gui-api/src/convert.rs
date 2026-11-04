//! 上流(`brainfuck-core` / `debugger-state`)の値から DTO への写像のみを行う。
//!
//! 位置の計算(行・列・UTF-16・逆引き)はここに置かず、上流の `SourceMap` の結果を写すだけにする。

use brainfuck_core::{ParseError, SourceMap, SourcePosition, SourceRange};

use crate::dto::{
    ApiError, ApiErrorKind, ParseErrorDto, ParseErrorKind, SourcePositionDto, SourceRangeDto,
};

/// 上流の位置の 4 フィールドをそのまま DTO へ写す(値の再計算はしない)。
pub fn source_position_dto(p: &SourcePosition) -> SourcePositionDto {
    SourcePositionDto {
        byte_offset: p.byte_offset,
        utf16_offset: p.utf16_offset,
        line: p.line,
        column: p.column,
    }
}

/// 上流の範囲の始点・終点をそのまま DTO へ写す。
pub fn source_range_dto(r: &SourceRange) -> SourceRangeDto {
    SourceRangeDto {
        start: source_position_dto(&r.start),
        end: source_position_dto(&r.end),
    }
}

/// 解析エラーを DTO へ写す。位置は `SourceMap::position_at(source, 違反のバイトオフセット)` の結果を使う。
///
/// 位置が取得できない(通常は起こらない、ソースと解析結果が食い違う)場合と、上流の
/// 非網羅の列挙に未知の種類が現れた場合は `internal` を返す。
pub fn parse_error_dto(source: &str, e: &ParseError) -> Result<ParseErrorDto, ApiError> {
    let (kind, instruction_index, source_offset) = match *e {
        ParseError::UnmatchedClosingBracket {
            instruction_index,
            source_offset,
        } => (
            ParseErrorKind::UnmatchedClosingBracket,
            instruction_index,
            source_offset,
        ),
        ParseError::UnmatchedOpeningBracket {
            instruction_index,
            source_offset,
        } => (
            ParseErrorKind::UnmatchedOpeningBracket,
            instruction_index,
            source_offset,
        ),
        _ => return Err(ApiError::new(ApiErrorKind::Internal, None)),
    };
    let position = SourceMap::position_at(source, source_offset)
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, None))?;
    Ok(ParseErrorDto {
        kind,
        instruction_index,
        position: source_position_dto(&position),
    })
}
