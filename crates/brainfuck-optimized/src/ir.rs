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
    /// 操作を内部で構築する(変換器のみが使う)。
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

    /// この操作を素朴実装が実行した場合に数える実行数(素朴換算コスト)を返す。純粋関数。
    ///
    /// - `start_cell`: 操作開始時の現在セル値 `v`。
    /// - `scan_iterations`: `Scan` で `start_cell != 0` のときだけ参照する走査の反復数 `k`
    ///   (同方向に 1 反復ずつ進んで最初に 0 のセルに至るまでの反復数。1 以上)。
    ///   それ以外の操作と `start_cell == 0` の場合は無視される。
    ///
    /// 閉形式(`v` = `start_cell`、`L` = `source_range().len()`、`B` = 本体の命令数、
    /// `m` = `|stride|`、`k` = `scan_iterations`)。素朴実装は `[` を 1 回、各反復で本体 `B`
    /// 命令と `]` 1 命令を実行し、`]` が 0 で抜けるとき `[` へは戻らない。
    ///
    /// | 操作 | 換算実行数 |
    /// |------|-----------|
    /// | `Add(_)` | `L`(正味 0 でも `L`) |
    /// | `Move(n)` | `\|n\|`(= `L`) |
    /// | `Output` / `Input` / `LoopStart` / `LoopEnd` | 1 |
    /// | `Clear { increments: false }` | `v == 0` なら 1、そうでなければ `1 + 2v` |
    /// | `Clear { increments: true }` | `v == 0` なら 1、そうでなければ `1 + 2(256 - v)` |
    /// | `Transfer { body_len: B, .. }` | `v == 0` なら 1、そうでなければ `1 + v(B + 1)` |
    /// | `Scan { stride }` | `v == 0` なら 1、そうでなければ `1 + k(m + 1)` |
    ///
    /// 移動を伴う操作(`Move`・`Transfer`)の閉形式は、[`Op::static_reach`] の範囲条件が満たされ
    /// (範囲外エラーにならず)、かつ実行される場合の実行数である。範囲外に出る場合、素朴実装は
    /// 途中で範囲外エラーを返すため、この値とは一致しない。
    ///
    /// 演算は飽和演算で、panic しない。`Scan` で `start_cell != 0` かつ `scan_iterations == 0` が
    /// 渡された場合は契約違反だが、閉形式どおり 1 を返す。結果は、操作が命令を 1 つ以上持つ限り
    /// 常に 1 以上である。
    pub fn naive_cost(&self, start_cell: u8, scan_iterations: u64) -> u64 {
        let v = u64::from(start_cell);
        match self.kind {
            OpKind::Add(_) => to_u64(self.source_range().len()),
            OpKind::Move(n) => to_u64(n.unsigned_abs()),
            OpKind::Output | OpKind::Input | OpKind::LoopStart { .. } | OpKind::LoopEnd { .. } => 1,
            OpKind::Clear { increments } => {
                if start_cell == 0 {
                    1
                } else if increments {
                    1 + 2 * (256 - v)
                } else {
                    1 + 2 * v
                }
            }
            OpKind::Transfer { body_len, .. } => {
                if start_cell == 0 {
                    1
                } else {
                    1u64.saturating_add(v.saturating_mul(to_u64(body_len).saturating_add(1)))
                }
            }
            OpKind::Scan { stride } => {
                if start_cell == 0 {
                    1
                } else {
                    let magnitude = to_u64(stride.unsigned_abs());
                    1u64.saturating_add(scan_iterations.saturating_mul(magnitude.saturating_add(1)))
                }
            }
        }
    }

    /// 移動を伴う操作が、現在位置 `dp` からの相対位置 `min..=max` をすべてテープ内に収める
    /// 必要があるときの `(min, max)` を返す(0 を含む)。
    ///
    /// 範囲条件は `dp as isize + min >= 0 && dp as isize + max <= TAPE_SIZE - 1` である。
    /// - `Move(n)` は `(min(0, n), max(0, n))` を返し、常に評価が必要。
    /// - `Transfer` は `(reach_min, reach_max)` を返す。評価が必要なのは**開始セルが非 0 のときだけ**
    ///   (0 のときは本体を実行しないため範囲外にならない)。
    /// - それ以外(`Add`・`Clear`・入出力・括弧・`Scan`)は `None`。`Scan` の範囲は走査中の
    ///   セル値に依存するため静的には決まらない。
    pub fn static_reach(&self) -> Option<(isize, isize)> {
        match self.kind {
            OpKind::Move(n) => Some((n.min(0), n.max(0))),
            OpKind::Transfer {
                reach_min,
                reach_max,
                ..
            } => Some((reach_min, reach_max)),
            OpKind::Add(_)
            | OpKind::Output
            | OpKind::Input
            | OpKind::LoopStart { .. }
            | OpKind::LoopEnd { .. }
            | OpKind::Clear { .. }
            | OpKind::Scan { .. } => None,
        }
    }
}

/// 命令数(`usize`)を `u64` へ変換する。64 ビット環境では常に成功する。
fn to_u64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// 解析済みプログラムと、その最適化後の操作列の組。構築後は不変。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizedProgram {
    source: Program,
    ops: Vec<Op>,
}

impl OptimizedProgram {
    /// 元の解析結果と操作列から組み立てる(変換器のみが使う)。
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

    fn op_of(kind: OpKind, source_len: usize) -> Op {
        Op::new(kind, 0, source_len)
    }

    const VALUES: [u8; 6] = [0, 1, 2, 127, 128, 255];

    #[test]
    fn naive_cost_of_linear_operations_is_the_source_length_for_every_value() {
        for v in VALUES {
            assert_eq!(op_of(OpKind::Add(0), 2).naive_cost(v, 0), 2, "Add v={v}");
            assert_eq!(op_of(OpKind::Add(3), 5).naive_cost(v, 0), 5, "Add v={v}");
            assert_eq!(op_of(OpKind::Move(-3), 3).naive_cost(v, 0), 3, "Move v={v}");
            assert_eq!(op_of(OpKind::Move(2), 2).naive_cost(v, 0), 2, "Move v={v}");
        }
    }

    #[test]
    fn naive_cost_of_io_and_general_loop_brackets_is_one() {
        for v in VALUES {
            assert_eq!(op_of(OpKind::Output, 1).naive_cost(v, 0), 1);
            assert_eq!(op_of(OpKind::Input, 1).naive_cost(v, 0), 1);
            assert_eq!(op_of(OpKind::LoopStart { end: 4 }, 1).naive_cost(v, 0), 1);
            assert_eq!(op_of(OpKind::LoopEnd { start: 0 }, 1).naive_cost(v, 0), 1);
        }
    }

    #[test]
    fn naive_cost_of_clear_follows_the_closed_forms() {
        let minus = op_of(OpKind::Clear { increments: false }, 3);
        let plus = op_of(OpKind::Clear { increments: true }, 3);
        // `[-]`: 1 + 2v、`[+]`: 1 + 2(256 - v)。v = 0 のときはいずれも 1。
        let expected_minus = [1, 3, 5, 255, 257, 511];
        let expected_plus = [1, 511, 509, 259, 257, 3];
        for (i, v) in VALUES.into_iter().enumerate() {
            assert_eq!(minus.naive_cost(v, 0), expected_minus[i], "[-] v={v}");
            assert_eq!(plus.naive_cost(v, 0), expected_plus[i], "[+] v={v}");
        }
    }

    #[test]
    fn naive_cost_of_transfer_follows_the_closed_form() {
        let op = Op::new(
            OpKind::Transfer {
                terms: vec![Term {
                    offset: 1,
                    factor: 2,
                }]
                .into_boxed_slice(),
                body_len: 5,
                reach_min: 0,
                reach_max: 1,
            },
            0,
            7,
        );
        // 1 + v(B + 1) = 1 + 6v。v = 0 のときは 1。
        let expected = [1, 7, 13, 763, 769, 1531];
        for (i, v) in VALUES.into_iter().enumerate() {
            assert_eq!(op.naive_cost(v, 0), expected[i], "Transfer v={v}");
        }
    }

    #[test]
    fn naive_cost_of_scan_follows_the_closed_form_and_ignores_k_at_zero() {
        let left = op_of(OpKind::Scan { stride: -2 }, 2);
        let right = op_of(OpKind::Scan { stride: 1 }, 1);
        // 1 + k(m + 1)。m は |stride|。
        assert_eq!(left.naive_cost(1, 3), 10);
        assert_eq!(left.naive_cost(255, 3), 10);
        assert_eq!(right.naive_cost(2, 1), 3);
        // v = 0 は k によらず 1。k = 0 の契約違反も panic せず 1。
        assert_eq!(left.naive_cost(0, 3), 1);
        assert_eq!(left.naive_cost(2, 0), 1);
    }

    #[test]
    fn naive_cost_saturates_instead_of_panicking() {
        let transfer = Op::new(
            OpKind::Transfer {
                terms: Vec::new().into_boxed_slice(),
                body_len: usize::MAX,
                reach_min: 0,
                reach_max: 0,
            },
            0,
            1,
        );
        assert_eq!(transfer.naive_cost(255, 0), u64::MAX);
        let scan = op_of(OpKind::Scan { stride: isize::MAX }, 1);
        assert_eq!(scan.naive_cost(1, u64::MAX), u64::MAX);
    }

    #[test]
    fn static_reach_of_moves_and_transfers_includes_the_start_cell() {
        assert_eq!(op_of(OpKind::Move(3), 3).static_reach(), Some((0, 3)));
        assert_eq!(op_of(OpKind::Move(-2), 2).static_reach(), Some((-2, 0)));
        let transfer = Op::new(
            OpKind::Transfer {
                terms: Vec::new().into_boxed_slice(),
                body_len: 4,
                reach_min: -1,
                reach_max: 2,
            },
            0,
            6,
        );
        assert_eq!(transfer.static_reach(), Some((-1, 2)));
    }

    #[test]
    fn static_reach_is_none_for_operations_without_a_static_range() {
        assert_eq!(op_of(OpKind::Add(1), 1).static_reach(), None);
        assert_eq!(op_of(OpKind::Output, 1).static_reach(), None);
        assert_eq!(op_of(OpKind::Input, 1).static_reach(), None);
        assert_eq!(op_of(OpKind::LoopStart { end: 1 }, 1).static_reach(), None);
        assert_eq!(op_of(OpKind::LoopEnd { start: 0 }, 1).static_reach(), None);
        assert_eq!(
            op_of(OpKind::Clear { increments: true }, 3).static_reach(),
            None
        );
        assert_eq!(op_of(OpKind::Scan { stride: 1 }, 1).static_reach(), None);
    }
}
