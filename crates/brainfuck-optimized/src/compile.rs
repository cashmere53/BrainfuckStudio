//! 解析済みプログラムから最適化後の IR への変換。
//!
//! 変換は純粋かつ全域であり、失敗しない。入力の命令列のみに依存する。
//! 加減算・同方向移動の統合に加え、代表的ループ(クリア・移動/加算/乗算・走査)を認識する。
//! 認識されないループは一般ループとして出力し、その本体は同じ規則で続けて変換する。

use crate::ir::{Op, OpKind, OptimizedProgram, Term};
use brainfuck_core::{Instruction, Program};
use std::collections::BTreeMap;

impl OptimizedProgram {
    /// 解析済みプログラムを 1 回の線形走査で最適化後の IR へ変換する。
    ///
    /// - `+`/`-` の連続は正味量(mod 256)の `Add` 1 操作へ統合する(正味 0 でも操作は残す)。
    /// - 同方向の `>`/`<` の連続は `Move` 1 操作へ統合する。逆向きの隣接は統合しない。
    /// - `.`/`,` は 1 命令 1 操作とし、統合・削除・並べ替えをしない。
    /// - 代表的ループ(クリア・移動/加算/乗算・走査)は 1 操作へ置換する(`recognize_loop` を参照)。
    /// - それ以外のループは `LoopStart`/`LoopEnd` の一般ループとし、対応する IR 添字は
    ///   変換時にスタックで 1 回だけ確定する。
    ///
    /// 命令が 0 個のプログラムは操作 0 個を返す。
    pub fn compile(program: &Program) -> OptimizedProgram {
        let instructions = program.instructions();
        let mut ops: Vec<Op> = Vec::with_capacity(instructions.len());
        // 未対応の `LoopStart` の IR 添字。`Program` が括弧を検証済みのため空にはならない前提。
        let mut open_loops: Vec<usize> = Vec::new();

        let mut i = 0;
        while i < instructions.len() {
            let start = i;
            match instructions[i] {
                Instruction::Increment | Instruction::Decrement => {
                    let mut net: u8 = 0;
                    while let Some(&instruction) = instructions.get(i) {
                        match instruction {
                            Instruction::Increment => net = net.wrapping_add(1),
                            Instruction::Decrement => net = net.wrapping_sub(1),
                            _ => break,
                        }
                        i += 1;
                    }
                    ops.push(Op::new(OpKind::Add(net), start, i - start));
                }
                Instruction::MoveRight | Instruction::MoveLeft => {
                    let direction = instructions[i];
                    let mut delta: isize = 0;
                    while instructions.get(i) == Some(&direction) {
                        // 総和は命令数以下なので `isize` の範囲内に収まる。
                        delta += if direction == Instruction::MoveRight {
                            1
                        } else {
                            -1
                        };
                        i += 1;
                    }
                    ops.push(Op::new(OpKind::Move(delta), start, i - start));
                }
                Instruction::Output => {
                    ops.push(Op::new(OpKind::Output, start, 1));
                    i += 1;
                }
                Instruction::Input => {
                    ops.push(Op::new(OpKind::Input, start, 1));
                    i += 1;
                }
                Instruction::LoopStart => {
                    if let Some(end) = program.matching_bracket(i) {
                        if let Some(kind) = recognize_loop(&instructions[i + 1..end]) {
                            ops.push(Op::new(kind, start, end + 1 - start));
                            i = end + 1;
                            continue;
                        }
                    }
                    open_loops.push(ops.len());
                    // `end` は対応する `]` の IR 添字が確定した時点で置き換える。
                    ops.push(Op::new(OpKind::LoopStart { end: 0 }, start, 1));
                    i += 1;
                }
                Instruction::LoopEnd => {
                    let end_index = ops.len();
                    let start_index = open_loops.pop();
                    debug_assert!(start_index.is_some(), "unmatched `]` at {start}");
                    // 括弧は `Program` が検証済みのため到達しない。万一の場合も panic せず、
                    // 自分自身を指す(一般ループとして不正な対応を出力しない)。
                    let start_index = start_index.unwrap_or(end_index);
                    if let Some(open_source) =
                        ops.get(start_index).map(|op| op.source_range().start)
                    {
                        ops[start_index] =
                            Op::new(OpKind::LoopStart { end: end_index }, open_source, 1);
                    }
                    ops.push(Op::new(OpKind::LoopEnd { start: start_index }, start, 1));
                    i += 1;
                }
            }
        }

        debug_assert!(open_loops.is_empty(), "unmatched `[`");
        OptimizedProgram::from_parts(program.clone(), ops)
    }
}

/// ループ本体(`[` と `]` の間の命令列)から代表的ループの形を認識する。
///
/// 次の順で最初に該当した形を返す。どれにも該当しなければ `None`(一般ループのまま)。
/// 本体は括弧を含まないため、入れ子ループは一般ループとして外側で扱われる。
fn recognize_loop(body: &[Instruction]) -> Option<OpKind> {
    if matches!(body, [Instruction::Decrement]) {
        return Some(OpKind::Clear { increments: false });
    }
    if matches!(body, [Instruction::Increment]) {
        return Some(OpKind::Clear { increments: true });
    }
    if let Some(stride) = scan_stride(body) {
        return Some(OpKind::Scan { stride });
    }
    transfer(body)
}

/// 同方向の移動のみから成る本体の 1 反復あたりの移動量(符号が向き)を返す。
fn scan_stride(body: &[Instruction]) -> Option<isize> {
    let first = *body.first()?;
    let magnitude = isize::try_from(body.len()).ok()?;
    match first {
        Instruction::MoveRight if body.iter().all(|&i| i == Instruction::MoveRight) => {
            Some(magnitude)
        }
        Instruction::MoveLeft if body.iter().all(|&i| i == Instruction::MoveLeft) => {
            Some(-magnitude)
        }
        _ => None,
    }
}

/// `+ - > <` のみから成り、ポインタ正味移動 0・開始セルの正味変化 -1 の本体を移動/加算/乗算として表す。
///
/// 係数は相対位置ごとに mod 256 で集約し、位置 0 と係数 0 の項は除く。
/// 到達範囲は本体を辿る途中の相対位置の最小・最大(0 を含む)。
fn transfer(body: &[Instruction]) -> Option<OpKind> {
    let mut position: isize = 0;
    let mut reach_min: isize = 0;
    let mut reach_max: isize = 0;
    let mut factors: BTreeMap<isize, u8> = BTreeMap::new();
    for instruction in body {
        match instruction {
            Instruction::Increment => {
                let factor = factors.entry(position).or_insert(0);
                *factor = factor.wrapping_add(1);
            }
            Instruction::Decrement => {
                let factor = factors.entry(position).or_insert(0);
                *factor = factor.wrapping_sub(1);
            }
            Instruction::MoveRight => position = position.checked_add(1)?,
            Instruction::MoveLeft => position = position.checked_sub(1)?,
            _ => return None,
        }
        reach_min = reach_min.min(position);
        reach_max = reach_max.max(position);
    }
    if position != 0 || factors.get(&0).copied().unwrap_or(0) != u8::MAX {
        return None;
    }
    let terms: Box<[Term]> = factors
        .into_iter()
        .filter(|&(offset, factor)| offset != 0 && factor != 0)
        .map(|(offset, factor)| Term { offset, factor })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    Some(OpKind::Transfer {
        terms,
        body_len: body.len(),
        reach_min,
        reach_max,
    })
}
