//! 回帰テスト: 発見した不具合を再発から守るための最小再現プログラムを置く(最適化機械側)。
//!
//! # 追加の規約
//!
//! 不具合を修正するたびに、次の 3 点を追加する。
//!
//! 1. `tests/regression/` 配下に、その不具合を再現する最小の `.bf` ファイルを置く。
//! 2. 本ファイルに、そのフィクスチャを `include_str!` で読み込み、期待する結果を検証する
//!    テスト関数を追加する。テスト関数名は事象が分かる名前にする。
//! 3. フィクスチャ(`.bf`)の先頭に、事象・原因・期待する振る舞いを説明するコメントを書く。
//!    コメントは命令文字(`+`・`-`・`<`・`>`・`.`・`,`・`[`・`]`)を含まない日本語で書く。
//!    命令文字はコメント内に入れると命令として数えられてしまうため、
//!    コメント行には命令文字を書かない。
//!
//! 本ファイルの各テスト関数の上に、対応する不具合の概要をコメントで記す。
//! 比較は素朴実装(core の `Machine`)をオラクルとして行い、期待値も明示的に検証する。
//!
//! 各テストは有限のステップ上限、または停止が確実なプログラムのみを用い、
//! 修正前の不具合で無限ループしても有限時間で失敗するようにする。

mod common;

use brainfuck_core::{MachineConfig, StopReason};
use common::{assert_equivalent, observe_core, observe_optimized};

/// 回帰テストの命令数上限。フィクスチャは数命令で停止するため十分な余裕があり、
/// 不具合で停止しなくなった場合も有限時間で失敗する。
const STEP_LIMIT: u64 = 100;

/// セル 0 で到達した範囲外に触れうる乗算ループ `[<+>-]` のフィクスチャ。
const CELL0_MULTIPLY_BODY_SOURCE: &str =
    include_str!("regression/cell0_multiply_body_out_of_range.bf");

/// セル 0 で到達したループは本体を評価せず、範囲外に触れうるプログラムでも
/// エラーにならず、素朴実装と同じく実行数 1 で停止することを確認する。
///
/// `[` でセルが 0 のため本体を飛ばして末尾へ進み、停止する。本体の `<` で
/// 範囲外へ出る可能性はあるが、実行されないためエラーは発生しない。
#[test]
fn cell0_loop_with_out_of_range_body_halts_without_evaluating_body() {
    let config = MachineConfig::with_max_steps(STEP_LIMIT);

    let oracle = observe_core(CELL0_MULTIPLY_BODY_SOURCE, b"", config);
    assert_eq!(oracle.result, Ok(StopReason::Halted));
    assert_eq!(oracle.steps_executed, 1, "素朴実装の実行数は 1");
    assert_eq!(oracle.data_pointer, 0);
    assert_eq!(oracle.current_cell, 0);
    assert!(oracle.halted);
    assert!(oracle.output.is_empty());

    let (optimized, ir_ops_executed) = observe_optimized(CELL0_MULTIPLY_BODY_SOURCE, b"", config);
    assert_eq!(optimized.result, Ok(StopReason::Halted));
    assert_eq!(optimized.steps_executed, 1, "最適化機械の実行数も 1");
    assert!(ir_ops_executed <= optimized.steps_executed);

    assert_equivalent(CELL0_MULTIPLY_BODY_SOURCE, b"", config);
}
