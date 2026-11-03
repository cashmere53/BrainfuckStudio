//! デバッグ拡張のテスト共通ヘルパ。
//!
//! 各テストファイルから `mod debug_common;` で読み込む。テストファイルごとに使うヘルパが
//! 異なるため、未使用コードによる警告はこのモジュール全体で抑止する。
//! セッション型に依存しないヘルパだけを置く(セッション用ヘルパは後続タスクで追加する)。

#![allow(dead_code)]

use brainfuck_core::{Machine, MachineConfig, Program};

/// ソース・入力・設定から機械を生成する。
///
/// ソースが解析に失敗する場合はテストを失敗させる。
pub fn machine_from(source: &str, input: &[u8], config: MachineConfig) -> Machine {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    Machine::new(program, input.to_vec(), config)
}
