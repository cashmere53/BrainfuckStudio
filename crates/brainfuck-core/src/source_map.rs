//! ソース位置変換(命令添字 ↔ ソース範囲)。
//!
//! 位置の数え方はこのモジュールの非公開の走査 [`Cursor`] 1 か所に実装する。
//! [`SourceMap::new`](構築時の全命令分の範囲)と [`SourceMap::position_at`](ソース文字列のみからの位置)
//! の両方がこの走査を使うため、規則を二重に実装しない。
//!
//! 規則:
//! - バイトオフセットは UTF-8 のバイト位置。ソース末尾(ソース長と等しい位置)は有効。
//! - 行区切りは `\n`・`\r\n`(1 個)・`\r` の 3 種。行番号は 1 始まり。
//! - 列は行頭からの Unicode スカラ値の個数 + 1(1 始まり)。
//! - UTF-16 オフセットはドキュメント先頭からの UTF-16 コード単位数(BMP 外の文字は 2)。
//! - `\r\n` の間(`\r` と `\n` の間のバイト位置)は直前の `\r` と同じ行に属し、列と UTF-16 は `\r` の位置 + 1。
//!   `\r\n` の直後は次の行の列 1。
//! - 命令文字は ASCII の 1 バイトで改行ではないため、範囲は始点から 1 バイト後までの半開区間
//!   (同一行、列・UTF-16 も各 +1)。

use std::fmt;

use crate::instruction::Instruction;
use crate::parser::Program;

/// ソース上の位置。
///
/// 4 種の値(バイトオフセット・行・列・UTF-16 オフセット)を同時に持つ。
/// 行と列は 1 始まり、列はスカラ値の個数、UTF-16 はドキュメント先頭からのコード単位数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourcePosition {
    /// ソース先頭からの UTF-8 バイトオフセット。
    pub byte_offset: usize,
    /// 行番号(1 始まり)。
    pub line: usize,
    /// 行頭からの Unicode スカラ値の個数 + 1(1 始まり)。
    pub column: usize,
    /// ドキュメント先頭からの UTF-16 コード単位数。
    pub utf16_offset: usize,
}

/// 命令 1 個のソース上の範囲。半開区間 `[start, end)`。
///
/// `start` は命令文字の位置、`end` は命令文字の直後の位置である。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceRange {
    /// 命令文字の位置。
    pub start: SourcePosition,
    /// 命令文字の直後の位置(半開区間の終点)。
    pub end: SourcePosition,
}

/// ソースと解析済みプログラムの整合エラー。
///
/// 表示文は固定の英語文面と数値のみで構成し、ソース全文や文字列は含めない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceMapError {
    /// プログラムの `instruction_index` 番目の命令が、ソース上の該当位置の文字と一致しない、または存在しない。
    InstructionMismatch {
        /// 不一致の命令添字。
        instruction_index: usize,
    },
    /// ソースの `source_offset` にある命令文字が、プログラムの命令列に存在しない(余剰)。
    UnexpectedInstruction {
        /// 余剰の命令文字のバイトオフセット。
        source_offset: usize,
    },
}

impl fmt::Display for SourceMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceMapError::InstructionMismatch { instruction_index } => {
                write!(
                    f,
                    "instruction mismatch at instruction index {instruction_index}"
                )
            }
            SourceMapError::UnexpectedInstruction { source_offset } => {
                write!(f, "unexpected instruction at source offset {source_offset}")
            }
        }
    }
}

impl std::error::Error for SourceMapError {}

/// 命令添字 ↔ ソース範囲の変換表。構築後は変更不可で、ソース文字列は保持しない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMap {
    /// `starts[i]` は命令 `i` の始点位置。
    starts: Vec<SourcePosition>,
    /// `line_starts[k]` は行 `k + 1` の先頭のバイトオフセット。空ソースでも `[0]` を持つ。
    line_starts: Vec<usize>,
}

impl SourceMap {
    /// 解析済みプログラムと、その解析に用いたソース文字列から変換表を構築する。
    ///
    /// ソース上のすべての命令文字がプログラムの命令列・オフセットと 1 対 1 で一致することを検証する。
    /// 不一致(欠落・別文字・余剰)は [`SourceMapError`] で失敗する。
    pub fn new(program: &Program, source: &str) -> Result<SourceMap, SourceMapError> {
        let instructions = program.instructions();
        let offsets = program.source_offsets();
        let mut starts = Vec::with_capacity(instructions.len());
        let mut line_starts = vec![0];
        let mut cursor = Cursor::START;
        while let Some((character, next)) = cursor.advance(source) {
            if next.line != cursor.line {
                line_starts.push(next.byte);
            }
            if let Some(instruction) = ascii_instruction(character) {
                let expected = starts.len();
                match offsets.get(expected) {
                    None => {
                        return Err(SourceMapError::UnexpectedInstruction {
                            source_offset: cursor.byte,
                        })
                    }
                    Some(&offset) if offset == cursor.byte => {
                        if instructions[expected] != instruction {
                            return Err(SourceMapError::InstructionMismatch {
                                instruction_index: expected,
                            });
                        }
                    }
                    Some(&offset) if offset > cursor.byte => {
                        return Err(SourceMapError::UnexpectedInstruction {
                            source_offset: cursor.byte,
                        })
                    }
                    Some(_) => {
                        return Err(SourceMapError::InstructionMismatch {
                            instruction_index: expected,
                        })
                    }
                }
                starts.push(cursor.position());
            }
            cursor = next;
        }
        if starts.len() < instructions.len() {
            return Err(SourceMapError::InstructionMismatch {
                instruction_index: starts.len(),
            });
        }
        Ok(SourceMap {
            starts,
            line_starts,
        })
    }

    /// 命令の個数を返す。
    pub fn instruction_count(&self) -> usize {
        self.starts.len()
    }

    /// ソースの行数を返す。行区切りの個数 + 1 であり、空ソースは 1 となる。
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// 命令添字の範囲を返す。範囲外は `None`。
    pub fn range_of(&self, instruction_index: usize) -> Option<SourceRange> {
        let start = *self.starts.get(instruction_index)?;
        // 命令文字は ASCII の 1 バイトで改行ではないため、終点は始点の 1 バイト後・同一行。
        let end = SourcePosition {
            byte_offset: start.byte_offset + 1,
            line: start.line,
            column: start.column + 1,
            utf16_offset: start.utf16_offset + 1,
        };
        Some(SourceRange { start, end })
    }

    /// バイトオフセットの位置にある命令を返す(厳密一致)。
    ///
    /// 命令文字の始点でのみ `Some` を返す。コメント文字・範囲外・文字境界でない位置は `None`。
    pub fn instruction_at_offset(&self, byte_offset: usize) -> Option<usize> {
        self.starts
            .binary_search_by_key(&byte_offset, |start| start.byte_offset)
            .ok()
    }

    /// バイトオフセット以降で最初の命令を返す(下限探索)。
    ///
    /// コメント上や文字境界の途中は次の命令になる。後続命令がなければ `None`。
    pub fn instruction_at_or_after_offset(&self, byte_offset: usize) -> Option<usize> {
        self.first_at_or_after(|start| start.byte_offset >= byte_offset)
    }

    /// UTF-16 コード単位オフセットの位置にある命令を返す(厳密一致)。
    ///
    /// 命令文字の始点でのみ `Some` を返す。コメント文字・サロゲートペアの途中・範囲外は `None`。
    /// 呼び出し側が UTF-16 の始点表を自前で作る必要はない。命令 → UTF-16 は
    /// [`SourceMap::range_of`] の始点・終点の `utf16_offset` で得られる。
    pub fn instruction_at_utf16(&self, utf16_offset: usize) -> Option<usize> {
        self.starts
            .binary_search_by_key(&utf16_offset, |start| start.utf16_offset)
            .ok()
    }

    /// UTF-16 コード単位オフセット以降で最初の命令を返す(下限探索)。
    ///
    /// コメント上・サロゲートペアの途中は次の命令になる。後続命令がなければ `None`。
    /// 意味論は [`SourceMap::instruction_at_or_after_offset`] と同一である。
    pub fn instruction_at_or_after_utf16(&self, utf16_offset: usize) -> Option<usize> {
        self.first_at_or_after(|start| start.utf16_offset >= utf16_offset)
    }

    /// 1 始まりの行番号について、その行の先頭以降で最初の命令を返す。
    ///
    /// 命令のない行は後続行の命令に正規化される。行番号 0・行数超過・後続命令なしは `None`。
    pub fn first_instruction_on_line(&self, line: usize) -> Option<usize> {
        let line_start = *self.line_starts.get(line.checked_sub(1)?)?;
        self.first_at_or_after(|start| start.byte_offset >= line_start)
    }

    /// 述語を満たす最初の命令添字を返す。`starts` は命令順に単調増加のため二分探索で求める。
    fn first_at_or_after(&self, reached: impl Fn(&SourcePosition) -> bool) -> Option<usize> {
        let index = self.starts.partition_point(|start| !reached(start));
        (index < self.starts.len()).then_some(index)
    }

    /// ソース文字列とバイトオフセットから位置を返す(プログラム不要)。
    ///
    /// 位置の規則は [`SourceMap::new`] と同一である。`byte_offset == source.len()` は有効(末尾位置)。
    /// `source.len()` を超える位置と文字境界でない位置は `None`(異常終了しない)。
    pub fn position_at(source: &str, byte_offset: usize) -> Option<SourcePosition> {
        if byte_offset > source.len() || !source.is_char_boundary(byte_offset) {
            return None;
        }
        let mut cursor = Cursor::START;
        while cursor.byte < byte_offset {
            let (_, next) = cursor.advance(source)?;
            cursor = next;
        }
        Some(cursor.position())
    }
}

/// ASCII の命令文字なら命令を返す。非 ASCII の文字はコメントなので `None`。
fn ascii_instruction(character: char) -> Option<Instruction> {
    if character.is_ascii() {
        Instruction::from_byte(character as u8)
    } else {
        None
    }
}

/// 位置の数え方を実装する非公開の走査状態。
///
/// `advance` が 1 文字ごとに位置を更新する唯一の場所であり、行・列・UTF-16 の規則はここにだけ置く。
#[derive(Debug, Clone, Copy)]
struct Cursor {
    /// バイトオフセット。文字境界にある。
    byte: usize,
    /// 行番号(1 始まり)。
    line: usize,
    /// 列番号(1 始まり、スカラ値の個数 + 1)。
    column: usize,
    /// UTF-16 コード単位オフセット。
    utf16: usize,
}

impl Cursor {
    /// ソース先頭の位置。
    const START: Cursor = Cursor {
        byte: 0,
        line: 1,
        column: 1,
        utf16: 0,
    };

    /// 現在位置を公開用の値として返す。
    fn position(self) -> SourcePosition {
        SourcePosition {
            byte_offset: self.byte,
            line: self.line,
            column: self.column,
            utf16_offset: self.utf16,
        }
    }

    /// 現在位置の文字を読み、その直後の位置を返す。末尾では `None`。
    ///
    /// 改行は直後から次の行を開始する。`\r` の直後が `\n` の場合は、`\n` までを同じ行に含めるため
    /// `\r` では改行しない(改行は `\n` で行う)。
    fn advance(self, source: &str) -> Option<(char, Cursor)> {
        let character = source.get(self.byte..)?.chars().next()?;
        let mut next = Cursor {
            byte: self.byte + character.len_utf8(),
            line: self.line,
            column: self.column + 1,
            utf16: self.utf16 + character.len_utf16(),
        };
        let breaks_line = match character {
            '\n' => true,
            '\r' => !source[next.byte..].starts_with('\n'),
            _ => false,
        };
        if breaks_line {
            next.line += 1;
            next.column = 1;
        }
        Some((character, next))
    }
}
