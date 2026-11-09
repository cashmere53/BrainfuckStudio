//! 完全状態の観測値と、項目ごとの差分。

use brainfuck_core::{RuntimeError, StopReason};

/// 1 回の実行の完全な観測値。
///
/// `steps_executed` は素朴実装換算の実行数である。方式固有の単位(`Units`)は含まない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// 終了種別と、異常終了の場合はエラー値。
    pub result: Result<StopReason, RuntimeError>,
    /// 命令ポインタ(core の命令添字)。
    pub pc: usize,
    /// データポインタ。
    pub data_pointer: usize,
    /// 現在のセルの値。
    pub current_cell: u8,
    /// 素朴実装換算の実行数。
    pub steps_executed: u64,
    /// 入力の読取位置。
    pub input_position: usize,
    /// テープ全体(長さ `TAPE_SIZE`)。
    pub tape: Box<[u8]>,
    /// 出力バイト列。
    pub output: Vec<u8>,
}

/// 観測値の比較項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Field {
    /// 終了種別(正常停止とエラー、または異なる停止理由)。
    Termination,
    /// 同じ種類のエラー同士で、エラーの値が異なる。
    ErrorValue,
    /// 命令ポインタ。
    Pc,
    /// データポインタ。
    DataPointer,
    /// 現在のセルの値。
    CurrentCell,
    /// 素朴換算の実行数。
    StepsExecuted,
    /// 入力の読取位置。
    InputPosition,
    /// 出力バイト列。
    Output,
    /// テープ全体。
    Tape,
}

impl Observation {
    /// 正常停止かどうか(`Ok(StopReason::Halted)`)。
    pub fn halted(&self) -> bool {
        self.result == Ok(StopReason::Halted)
    }

    /// 2 つの観測値の差分を項目ごとに返す。完全一致なら空。
    ///
    /// 各項目は独立に比較する。終了種別の不一致は `Termination`、両方がエラーで種別(判別子)が同じ、
    /// かつ値だけが異なる場合は `ErrorValue` とする。
    pub fn diff(&self, other: &Observation) -> Vec<Field> {
        let mut fields = Vec::new();
        if self.result != other.result {
            match (self.result, other.result) {
                (Err(a), Err(b)) if std::mem::discriminant(&a) == std::mem::discriminant(&b) => {
                    fields.push(Field::ErrorValue)
                }
                _ => fields.push(Field::Termination),
            }
        }
        if self.pc != other.pc {
            fields.push(Field::Pc);
        }
        if self.data_pointer != other.data_pointer {
            fields.push(Field::DataPointer);
        }
        if self.current_cell != other.current_cell {
            fields.push(Field::CurrentCell);
        }
        if self.steps_executed != other.steps_executed {
            fields.push(Field::StepsExecuted);
        }
        if self.input_position != other.input_position {
            fields.push(Field::InputPosition);
        }
        if self.output != other.output {
            fields.push(Field::Output);
        }
        if self.tape != other.tape {
            fields.push(Field::Tape);
        }
        fields
    }

    /// テープ全体の指紋(FNV-1a 64)。表示用であり、暗号用途ではない。
    pub fn tape_digest(&self) -> u64 {
        const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        self.tape.iter().fold(OFFSET_BASIS, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(PRIME)
        })
    }
}
