//! 最適化後の中間表現(IR)の型と読み取り API。
//!
//! 型は不変であり、構築は crate 内部のコンストラクタ(`compile` が使う)に限る。
//! 公開 API からは操作列も元の解析結果も変更できない。

use brainfuck_core::Program;
use std::ops::Range;

/// `Transfer` の 1 項。開始セルからの相対位置の係数を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Term {
    /// 開始セルからの相対位置(0 は含まない)。
    pub offset: isize,
    /// 加算係数(mod 256、0 は含まない)。
    pub factor: u8,
}

/// 最適化後の操作の種別。
///
/// 将来の最適化パターン追加を後方互換で行うため `#[non_exhaustive]` とする。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpKind {
    /// `+`/`-` の連続。正味量(mod 256)を加える。
    Add(u8),
    /// 同方向 `>`/`<` の連続。正は右、負は左。
    Move(isize),
    /// `.`。現在セルを 1 バイト出力する。
    Output,
    /// `,`。1 バイトを入力して現在セルへ格納する。
    Input,
    /// 最適化対象外ループの `[`。`end` は対応する `LoopEnd` の IR 添字。
    LoopStart { end: usize },
    /// 最適化対象外ループの `]`。`start` は対応する `LoopStart` の IR 添字。
    LoopEnd { start: usize },
    /// `[-]`(`increments = false`)/ `[+]`(`increments = true`)。
    Clear { increments: bool },
    /// 開始セルの正味変化が -1、ポインタ正味移動 0 の `+ - > <` のみのループ。
    /// `body_len` は本体の命令数。`reach_min <= 0 <= reach_max` は本体が到達する
    /// 相対位置の範囲。
    Transfer {
        terms: Box<[Term]>,
        body_len: usize,
        reach_min: isize,
        reach_max: isize,
    },
    /// 同方向移動のみの本体。`stride` の符号が向き、`|stride|` は 1 反復の移動命令数。
    Scan { stride: isize },
}

/// 元の命令範囲を持つ最適化後の操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    kind: OpKind,
    source_start: usize,
    source_len: usize,
}

impl Op {
    /// 操作を内部で構築する。現時点では呼び出し元が無いため一時的に未使用を許可し、
    /// 変換器の実装で除去する。
    #[allow(dead_code)]
    pub(crate) fn new(kind: OpKind, source_start: usize, source_len: usize) -> Op {
        Op {
            kind,
            source_start,
            source_len,
        }
    }

    /// 操作の種別を返す。
    pub fn kind(&self) -> &OpKind {
        &self.kind
    }

    /// この操作が対応する素朴実装の命令添字の範囲(半開区間)を返す。
    pub fn source_range(&self) -> Range<usize> {
        self.source_start..self.source_start + self.source_len
    }
}

/// 解析済みプログラムと、その最適化後の操作列の組。構築後は不変。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizedProgram {
    source: Program,
    ops: Vec<Op>,
}

impl OptimizedProgram {
    /// 元の解析結果と操作列から組み立てる。現時点では呼び出し元が無いため
    /// 一時的に未使用を許可し、変換器の実装で除去する。
    #[allow(dead_code)]
    pub(crate) fn from_parts(source: Program, ops: Vec<Op>) -> OptimizedProgram {
        OptimizedProgram { source, ops }
    }

    /// 最適化後の操作列を読み取り専用で返す。
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// 元の解析結果を読み取り専用で返す。
    pub fn source(&self) -> &Program {
        &self.source
    }
}

#[cfg(test)]
mod tests {
    use super::{Op, OpKind, OptimizedProgram, Term};
    use brainfuck_core::Program;

    #[test]
    fn op_reports_its_kind_and_half_open_source_range() {
        let op = Op::new(OpKind::Add(3), 2, 5);
        assert_eq!(op.kind(), &OpKind::Add(3));
        assert_eq!(op.source_range(), 2..7);
    }

    #[test]
    fn every_kind_round_trips_through_its_constructor() {
        let terms: Box<[Term]> = vec![Term {
            offset: 1,
            factor: 2,
        }]
        .into_boxed_slice();
        let kinds = [
            OpKind::Add(255),
            OpKind::Move(-2),
            OpKind::Output,
            OpKind::Input,
            OpKind::LoopStart { end: 4 },
            OpKind::LoopEnd { start: 0 },
            OpKind::Clear { increments: false },
            OpKind::Clear { increments: true },
            OpKind::Transfer {
                terms,
                body_len: 5,
                reach_min: 0,
                reach_max: 1,
            },
            OpKind::Scan { stride: -1 },
        ];
        for kind in kinds {
            let op = Op::new(kind.clone(), 0, 1);
            assert_eq!(op.kind(), &kind);
            assert_eq!(op.source_range(), 0..1);
        }
    }

    #[test]
    fn term_exposes_offset_and_factor() {
        let term = Term {
            offset: -3,
            factor: 7,
        };
        assert_eq!(term.offset, -3);
        assert_eq!(term.factor, 7);
    }

    #[test]
    fn optimized_program_exposes_ops_and_source_program() {
        let source = Program::parse("+-.").expect("valid source");
        let ops = vec![Op::new(OpKind::Add(0), 0, 2), Op::new(OpKind::Output, 2, 1)];
        let optimized = OptimizedProgram::from_parts(source.clone(), ops.clone());
        assert_eq!(optimized.ops(), ops.as_slice());
        assert_eq!(optimized.source(), &source);
        let covered: usize = optimized
            .ops()
            .iter()
            .map(|op| op.source_range().len())
            .sum();
        assert_eq!(covered, optimized.source().len());
    }

    #[test]
    fn empty_program_has_no_ops() {
        let source = Program::parse("").expect("empty source is valid");
        let optimized = OptimizedProgram::from_parts(source, Vec::new());
        assert!(optimized.ops().is_empty());
        assert!(optimized.source().is_empty());
    }
}
