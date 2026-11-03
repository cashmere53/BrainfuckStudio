//! 回帰テスト: 発見した不具合を再発から守るための最小再現プログラムを置く。
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
//! 初期ケースは、規約を示すための `loop_end_halts.bf` 1 件である。
//!
//! 各テストは有限のステップ上限を設定し、修正前の不具合で無限ループしても
//! 有限時間で失敗するようにする。

mod common;

use brainfuck_core::{MachineConfig, StopReason};
use common::machine_from;

/// 末尾の閉じ括弧で停止することを確認するフィクスチャ(`+[-]`)。
const LOOP_END_HALTS_SOURCE: &str = include_str!("regression/loop_end_halts.bf");

/// 回帰テストの命令数上限。フィクスチャは数命令で停止するため十分な余裕があり、
/// 不具合で停止しなくなった場合も有限時間で失敗する。
const STEP_LIMIT: u64 = 100;

/// 最後の命令がループ終了 `]` のプログラムは、`]` の後に `pc` が命令数(4)へ到達して停止する。
///
/// 規約の実証用の初期ケース。`+` で現在セルを 1 にし、`[` で非 0 のため本体へ入り、
/// `-` で 0 に戻し、`]` でセルが 0 のためループを抜ける。実行数は 4、出力は空である。
#[test]
fn loop_end_as_last_instruction_halts_with_pc_at_program_length() {
    let mut machine = machine_from(
        LOOP_END_HALTS_SOURCE,
        b"",
        MachineConfig::with_max_steps(STEP_LIMIT),
    );

    assert_eq!(machine.run(), Ok(StopReason::Halted));

    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 4, "pc は命令数(4)に到達している");
    assert_eq!(snapshot.steps_executed, 4, "実行数は 4");
    assert_eq!(snapshot.current_cell, 0);
    assert!(snapshot.halted);
    assert_eq!(machine.output(), b"");
}
