//! ソースの解析と、命令列・ソースバイトオフセット・括弧対応の保持。
//!
//! 解析はバイト単位で行う。8 命令の文字はすべて ASCII であり、UTF-8 の
//! 継続バイト(0x80 以上)と衝突しないため、文字単位の走査と同じ結果になり、
//! 報告するオフセットは文字数ではなくバイト位置となる。

use crate::error::ParseError;
use crate::instruction::Instruction;

/// 解析済みの不変なプログラム。
///
/// 命令列、各命令が元ソース上で現れたバイトオフセット、括弧対応表を並列に保持する。
/// 解析に成功した時点で括弧は完全に対応している。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// 出現順の命令列。
    instructions: Vec<Instruction>,
    /// `instructions[i]` の元ソース上のバイトオフセット(厳密単調増加)。
    source_offsets: Vec<usize>,
    /// 括弧対応表。`[` の要素は対応する `]` の命令添字、`]` の要素は対応する `[` の
    /// 命令添字。括弧以外の要素の値は未規定。
    jump_table: Vec<usize>,
}

impl Program {
    /// 文字列を解析する。`Program::parse_bytes(source.as_bytes())` と同一の結果になる。
    pub fn parse(source: &str) -> Result<Program, ParseError> {
        Program::parse_bytes(source.as_bytes())
    }

    /// UTF-8 を要求しないバイト列を解析する。
    ///
    /// 8 命令の ASCII バイト以外(0x80 以上のバイトを含む)はすべてコメントとして無視する。
    /// 報告するオフセットは入力バイト列上の実位置である。
    ///
    /// 括弧が対応しない場合は失敗し、実行可能なプログラムは返さない。
    /// 走査中に対応する `[` の無い `]` が現れた時点でその位置を報告する。
    /// 解析終了時に未解決の `[` が残る場合は、最初に積まれた(最も早い)`[` を報告する。
    pub fn parse_bytes(source: &[u8]) -> Result<Program, ParseError> {
        let mut instructions = Vec::new();
        let mut source_offsets = Vec::new();
        let mut jump_table = Vec::new();
        // 未解決の `[` の命令添字。底が最も早い未解決位置となる。
        let mut open_brackets: Vec<usize> = Vec::new();
        for (offset, &byte) in source.iter().enumerate() {
            let Some(instruction) = Instruction::from_byte(byte) else {
                continue;
            };
            let index = instructions.len();
            // 括弧以外の要素は未規定だが、表の長さを命令数と揃えるため 0 で埋める。
            jump_table.push(0);
            match instruction {
                Instruction::LoopStart => open_brackets.push(index),
                Instruction::LoopEnd => {
                    let Some(open) = open_brackets.pop() else {
                        return Err(ParseError::UnmatchedClosingBracket {
                            instruction_index: index,
                            source_offset: offset,
                        });
                    };
                    jump_table[open] = index;
                    jump_table[index] = open;
                }
                _ => {}
            }
            instructions.push(instruction);
            source_offsets.push(offset);
        }
        if let Some(&first_open) = open_brackets.first() {
            return Err(ParseError::UnmatchedOpeningBracket {
                instruction_index: first_open,
                source_offset: source_offsets[first_open],
            });
        }
        Ok(Program {
            instructions,
            source_offsets,
            jump_table,
        })
    }

    /// 命令列を出現順に返す。
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    /// 命令の個数を返す。
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// 命令が 0 個であるかを返す。
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    /// 命令添字に対応するソースバイトオフセットを返す。範囲外は `None`。
    pub fn source_offset(&self, instruction_index: usize) -> Option<usize> {
        self.source_offsets.get(instruction_index).copied()
    }

    /// 全命令のソースバイトオフセット列を返す。
    pub fn source_offsets(&self) -> &[usize] {
        &self.source_offsets
    }

    /// 括弧命令の対応先の命令添字を返す。括弧以外または範囲外は `None`。
    ///
    /// 対応は対称である。`[` の対応先は `]` の添字、`]` の対応先は `[` の添字となる。
    ///
    /// # Examples
    ///
    /// ```
    /// use brainfuck_core::Program;
    ///
    /// let program = Program::parse("+[-[]]").expect("valid");
    /// assert_eq!(program.matching_bracket(1), Some(5)); // 外側の `[`
    /// assert_eq!(program.matching_bracket(5), Some(1)); // 対応先の対応先は自分自身
    /// assert_eq!(program.matching_bracket(3), Some(4)); // 内側の `[`
    /// assert_eq!(program.matching_bracket(0), None);    // 括弧以外は該当なし
    /// assert_eq!(program.matching_bracket(99), None);   // 範囲外は該当なし
    /// ```
    pub fn matching_bracket(&self, instruction_index: usize) -> Option<usize> {
        match self.instructions.get(instruction_index)? {
            Instruction::LoopStart | Instruction::LoopEnd => {
                self.jump_table.get(instruction_index).copied()
            }
            _ => None,
        }
    }

    /// 括弧対応表の全体を返す。crate 内部専用で、機械が括弧命令の位置でのみ読む。
    ///
    /// 括弧以外の要素の値は未規定であり、公開 API からは読めない。
    pub(crate) fn jump_table(&self) -> &[usize] {
        &self.jump_table
    }
}
