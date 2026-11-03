//! ソースの解析と、命令列・ソースバイトオフセットの保持。
//!
//! 解析はバイト単位で行う。8 命令の文字はすべて ASCII であり、UTF-8 の
//! 継続バイト(0x80 以上)と衝突しないため、文字単位の走査と同じ結果になり、
//! 報告するオフセットは文字数ではなくバイト位置となる。

use crate::error::ParseError;
use crate::instruction::Instruction;

/// 解析済みの不変なプログラム。
///
/// 命令列と、各命令が元ソース上で現れたバイトオフセットを並列に保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// 出現順の命令列。
    instructions: Vec<Instruction>,
    /// `instructions[i]` の元ソース上のバイトオフセット(厳密単調増加)。
    source_offsets: Vec<usize>,
    /// 括弧対応表。Task 3.2 で解析時に構築する。現時点では未構築(空)。
    #[allow(dead_code)]
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
    pub fn parse_bytes(source: &[u8]) -> Result<Program, ParseError> {
        let mut instructions = Vec::new();
        let mut source_offsets = Vec::new();
        for (offset, &byte) in source.iter().enumerate() {
            if let Some(instruction) = Instruction::from_byte(byte) {
                instructions.push(instruction);
                source_offsets.push(offset);
            }
        }
        Ok(Program {
            instructions,
            source_offsets,
            jump_table: Vec::new(),
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
}
