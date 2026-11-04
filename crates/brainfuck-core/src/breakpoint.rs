//! ブレークポイント集合(命令添字)。
//!
//! 保持するのは命令添字のみで、ソース位置は持たない(コメント位置は表現できない)。
//! 有効な添字は `0..instruction_count` で、範囲外の追加・トグルは集合を変更せずエラーとなる。
//! 問い合わせ(`contains`)と削除(`remove`)は範囲外でも異常終了せず偽を返す。
//!
//! 内部は「命令数と同じ長さの真偽値配列」と「件数」で持つ。照合は添字による O(1) である。

use std::error::Error;
use std::fmt;

/// ブレークポイント操作のエラー。
///
/// エラーを返した操作は集合を変更しない。診断文は固定英語文面で、数値のみを含む。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BreakpointError {
    /// 命令添字が命令数以上である。
    OutOfRange {
        /// 指定された命令添字。
        instruction_index: usize,
        /// プログラムの命令数。
        instruction_count: usize,
    },
}

impl fmt::Display for BreakpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            BreakpointError::OutOfRange {
                instruction_index,
                instruction_count,
            } => write!(
                f,
                "breakpoint instruction index out of range: instruction #{instruction_index}, instruction count {instruction_count}"
            ),
        }
    }
}

impl Error for BreakpointError {}

/// 命令添字のブレークポイント集合。
///
/// 有効な添字は `0..instruction_count` である。
/// 反復は昇順で重複を含まない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakpointSet {
    /// 命令ごとの設定有無。長さは命令数と等しい。
    flags: Vec<bool>,
    /// 設定されている添字の件数。`flags` の真の要素数と常に一致する。
    count: usize,
}

impl BreakpointSet {
    /// 命令数 `instruction_count` の空の集合を作る。
    pub fn new(instruction_count: usize) -> BreakpointSet {
        BreakpointSet {
            flags: vec![false; instruction_count],
            count: 0,
        }
    }

    /// 対象プログラムの命令数を返す。
    pub fn instruction_count(&self) -> usize {
        self.flags.len()
    }

    /// 命令添字を追加する。新規に追加したら `Ok(true)`、既に存在したら `Ok(false)`。
    ///
    /// 範囲外なら `Err(BreakpointError::OutOfRange)` を返し、集合は変更しない。
    pub fn insert(&mut self, instruction_index: usize) -> Result<bool, BreakpointError> {
        self.check_range(instruction_index)?;
        if self.flags[instruction_index] {
            Ok(false)
        } else {
            self.flags[instruction_index] = true;
            self.count += 1;
            Ok(true)
        }
    }

    /// 命令添字を削除する。存在して削除したら `true`、存在しなければ `false`。
    ///
    /// 範囲外でも異常終了せず `false` を返す。
    pub fn remove(&mut self, instruction_index: usize) -> bool {
        match self.flags.get_mut(instruction_index) {
            Some(flag) if *flag => {
                *flag = false;
                self.count -= 1;
                true
            }
            _ => false,
        }
    }

    /// 命令添字の設定をトグルする。操作後に設定されていれば `Ok(true)`、削除されていれば `Ok(false)`。
    ///
    /// 範囲外なら `Err(BreakpointError::OutOfRange)` を返し、集合は変更しない。
    pub fn toggle(&mut self, instruction_index: usize) -> Result<bool, BreakpointError> {
        self.check_range(instruction_index)?;
        if self.flags[instruction_index] {
            self.flags[instruction_index] = false;
            self.count -= 1;
            Ok(false)
        } else {
            self.flags[instruction_index] = true;
            self.count += 1;
            Ok(true)
        }
    }

    /// すべての添字を削除する。
    pub fn clear(&mut self) {
        self.flags.iter_mut().for_each(|flag| *flag = false);
        self.count = 0;
    }

    /// 命令添字が設定されているかを返す。範囲外は `false`。
    pub fn contains(&self, instruction_index: usize) -> bool {
        self.flags.get(instruction_index).copied().unwrap_or(false)
    }

    /// 集合が空かを返す。
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// 設定されている添字の件数を返す。
    pub fn len(&self) -> usize {
        self.count
    }

    /// 設定されている命令添字を昇順・重複なしで列挙する。
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.flags
            .iter()
            .enumerate()
            .filter_map(|(index, &set)| set.then_some(index))
    }

    /// 範囲外なら `OutOfRange` を返す。
    fn check_range(&self, instruction_index: usize) -> Result<(), BreakpointError> {
        if instruction_index < self.flags.len() {
            Ok(())
        } else {
            Err(BreakpointError::OutOfRange {
                instruction_index,
                instruction_count: self.flags.len(),
            })
        }
    }
}
