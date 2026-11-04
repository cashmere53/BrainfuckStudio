//! デバッグ拡張のテスト共通ヘルパ。
//!
//! 各テストファイルから `mod debug_common;` で読み込む。テストファイルごとに使うヘルパが
//! 異なるため、未使用コードによる警告はこのモジュール全体で抑止する。
//! 機械・セッション・キャンセル指示の生成ヘルパを置く。

#![allow(dead_code)]

use brainfuck_core::{CancelFn, DebugSession, Machine, MachineConfig, Program};
use std::cell::Cell;

/// ソース・入力・設定から機械を生成する。
///
/// ソースが解析に失敗する場合はテストを失敗させる。
pub fn machine_from(source: &str, input: &[u8], config: MachineConfig) -> Machine {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    Machine::new(program, input.to_vec(), config)
}

/// ソース・入力・設定からデバッグセッションを生成する。
///
/// ソースが解析に失敗する場合はテストを失敗させる。
pub fn session_from(source: &str, input: &[u8], config: MachineConfig) -> DebugSession {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    DebugSession::new(program, input.to_vec(), config)
}

/// 最終状態(スナップショット・テープ・出力・入力読取位置)が一致することを確認する。
///
/// 機械の内部状態の比較のうち、観測可能な項目を検証メッセージ付きで比べる。
pub fn assert_same_final_state(actual: &Machine, expected: &Machine) {
    assert_eq!(actual.snapshot(), expected.snapshot(), "snapshot");
    assert_eq!(actual.tape(), expected.tape(), "tape");
    assert_eq!(actual.output(), expected.output(), "output");
    assert_eq!(
        actual.input_position(),
        expected.input_position(),
        "input position"
    );
}

/// 「`n` 回の確認まで供給せず、`n + 1` 回目の確認から有効になる」キャンセル指示を作る。
///
/// 確認回数は `Cell` で数える。別スレッドを使わずに、キャンセルの発生位置を決定的に再現するために用いる。
/// `n` 回の確認はいずれも偽を返し、`n + 1` 回目以降は真を返す。
pub fn cancel_after(n: usize) -> CancelFn<impl Fn() -> bool> {
    let calls = Cell::new(0usize);
    CancelFn(move || {
        let seen = calls.get();
        calls.set(seen + 1);
        seen >= n
    })
}
