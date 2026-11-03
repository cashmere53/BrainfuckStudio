//! 機械のテスト共通ヘルパ。
//!
//! 各テストファイルから `mod common;` で読み込む。テストファイルごとに使うヘルパが
//! 異なるため、未使用コードによる警告はこのモジュール全体で抑止する。

#![allow(dead_code)]

use brainfuck_core::{Machine, MachineConfig, Program};

/// ソース・入力・設定から機械を生成する。
///
/// ソースが解析に失敗する場合はテストを失敗させる。
pub fn machine_from(source: &str, input: &[u8], config: MachineConfig) -> Machine {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    Machine::new(program, input.to_vec(), config)
}

/// 2 つの機械の実行状態が完全に一致することを確認する。
///
/// 観測 API の各項目を個別に比較し、失敗時に差分の項目が分かるようにした上で、
/// `Machine` の完全一致比較でも一致することを確認する。
pub fn assert_same_state(actual: &Machine, expected: &Machine) {
    assert_eq!(actual.snapshot().pc, expected.snapshot().pc, "pc");
    assert_eq!(
        actual.snapshot().data_pointer,
        expected.snapshot().data_pointer,
        "data pointer"
    );
    assert_eq!(
        actual.snapshot().current_cell,
        expected.snapshot().current_cell,
        "current cell"
    );
    assert_eq!(
        actual.snapshot().steps_executed,
        expected.snapshot().steps_executed,
        "steps executed"
    );
    assert_eq!(
        actual.snapshot().halted,
        expected.snapshot().halted,
        "halted"
    );
    assert_eq!(actual.tape(), expected.tape(), "tape");
    assert_eq!(actual.output(), expected.output(), "output");
    assert_eq!(
        actual.input_position(),
        expected.input_position(),
        "input position"
    );
    assert_eq!(actual.program(), expected.program(), "program");
    assert_eq!(actual, expected, "machine state");
}
