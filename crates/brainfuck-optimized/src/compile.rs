//! 解析済みプログラムから最適化後の IR への変換。
//!
//! 変換は純粋かつ全域であり、失敗しない。入力の命令列のみに依存する。
//! 2.2 の範囲では、加減算・同方向移動の統合と、すべてのループを一般ループとして出力する。
//! 代表的ループの認識は 2.3 で行う。

use crate::ir::{Op, OpKind, OptimizedProgram};
use brainfuck_core::{Instruction, Program};

impl OptimizedProgram {
    /// 解析済みプログラムを 1 回の線形走査で最適化後の IR へ変換する。
    ///
    /// - `+`/`-` の連続は正味量(mod 256)の `Add` 1 操作へ統合する(正味 0 でも操作は残す)。
    /// - 同方向の `>`/`<` の連続は `Move` 1 操作へ統合する。逆向きの隣接は統合しない。
    /// - `.`/`,` は 1 命令 1 操作とし、統合・削除・並べ替えをしない。
    /// - すべてのループは `LoopStart`/`LoopEnd` の一般ループとし、対応する IR 添字は
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
