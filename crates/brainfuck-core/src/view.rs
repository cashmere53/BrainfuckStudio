//! 状態ビュー・テープ表示窓。
//!
//! テープ窓の計算は純粋関数 [`tape_window_of`] の 1 か所だけに置く。機械版 [`tape_window`] は
//! 機械のテープと `dp` を渡して委譲するだけで、独自の計算を持たない。
//! 窓はテープのスライスの借用であり、コピーも状態の変更もしない。
//!
//! 窓は `dp - 半径` から `dp + 半径` までを有効なテープ範囲に切り詰め、昇順で返す。
//! 列挙は昇順であり、端では切り詰める(パディングもずらしもしない)。
//! 窓の大きさは半径とテープ長のみで決まり、`dp - 半径` と `dp + 半径` の計算は飽和演算で行う。

use crate::machine::{Machine, MachineSnapshot, StopReason};

/// テープ表示窓の既定の半径(現在セルの両側に含めるセル数)。
pub const DEFAULT_WINDOW_RADIUS: usize = 16;

/// テープ表示窓。
///
/// `cells` はテープ上の `start` から昇順に連続するセル値で、テープ全体の同じ添字の現在値と一致する。
/// `pointer_offset` は窓内での `dp` の位置であり、`cells[pointer_offset]` が現在セルを指す。
/// 窓が空の場合(`dp` がテープ範囲外)は `pointer_offset` は 0 である。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TapeWindow<'a> {
    /// 窓の先頭セルのテープ上の添字。
    pub start: usize,
    /// `start` から昇順の連続セル値。
    pub cells: &'a [u8],
    /// 窓内での `dp` の位置。
    pub pointer_offset: usize,
}

/// 状態ビュー。
///
/// 生成はセッション側が行う(`last_stop` はセッションが所有するため)。
/// `output` は UTF-8 への変換や置換をせず、バイト列のまま返す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct StateView<'a> {
    /// 機械の状態の観測結果。
    pub snapshot: MachineSnapshot,
    /// 現在のポインタ周辺のテープ窓。
    pub tape_window: TapeWindow<'a>,
    /// 出力バイト列(バイト列のまま)。
    pub output: &'a [u8],
    /// 直近の停止理由。
    pub last_stop: Option<StopReason>,
}

/// 機械を持たない呼び出し元向けのテープ表示窓(純粋関数)。
///
/// `tape` の `dp` を中心に `dp - radius` から `dp + radius` までを `0..tape.len()` に切り詰めて借用する。
/// 窓の先頭は `dp.saturating_sub(radius)`、末尾は `dp.saturating_add(radius)` を `tape.len() - 1` で
/// 抑えた位置であり、半径が極端に大きくても異常終了しない。
///
/// `dp` がテープ範囲外(空テープを含む)のときは異常終了せず、空の窓を返す。
/// その場合 `start` はテープ長、`cells` は空、`pointer_offset` は 0 である。
/// この場合に限り「`start + pointer_offset == dp`」は成り立たない。
/// 引数は変更せず、窓の大きさは半径とテープ長のみで決まる。
pub fn tape_window_of(tape: &[u8], dp: usize, radius: usize) -> TapeWindow<'_> {
    let len = tape.len();
    if dp >= len {
        // 範囲外または空テープ。空の窓を返す(この場合のみ `start + pointer_offset == dp` は成り立たない)。
        return TapeWindow {
            start: len,
            cells: &tape[len..],
            pointer_offset: 0,
        };
    }
    // `dp < len` のため `len - 1` は非負で、`start <= dp <= end_inclusive` が成り立つ。
    let start = dp.saturating_sub(radius);
    let end_inclusive = dp.saturating_add(radius).min(len - 1);
    TapeWindow {
        start,
        cells: &tape[start..=end_inclusive],
        pointer_offset: dp - start,
    }
}

/// 機械版のテープ表示窓。
///
/// `tape_window_of(machine.tape(), machine.snapshot().data_pointer, radius)` と同一の結果を返し、
/// 機械の状態は変更しない。
pub fn tape_window(machine: &Machine, radius: usize) -> TapeWindow<'_> {
    tape_window_of(machine.tape(), machine.snapshot().data_pointer, radius)
}
