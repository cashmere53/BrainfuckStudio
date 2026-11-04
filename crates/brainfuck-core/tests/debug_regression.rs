//! デバッグ層の回帰テスト(タスク 4.3、要件 11.8)。
//!
//! core の回帰テスト(`tests/regression.rs`)の規約を踏襲する。core の既存回帰ファイルは変更しない。
//!
//! # 追加の規約
//!
//! 不具合を修正するたびに、次の 3 点を追加する。
//!
//! 1. `tests/regression/debug_*.bf` に、その不具合を再現する最小の `.bf` ファイルを置く。
//!    ファイル名は必ず `debug_` 接頭辞で始める。core の `tests/regression/` には接頭辞のない
//!    既存フィクスチャが同居するため、この接頭辞で名前の衝突を避ける。
//! 2. 本ファイルに、そのフィクスチャを `include_str!` で読み込み、期待する結果を検証する
//!    テスト関数を追加する。テスト関数名は事象が分かる名前にする。
//! 3. フィクスチャ(`.bf`)の先頭に、事象・原因・期待する振る舞いを説明するコメントを書く。
//!    コメントは命令文字(`+`・`-`・`<`・`>`・`.`・`,`・`[`・`]`)を含まない日本語で書く。
//!    命令文字はコメント内に入れると命令として数えられてしまうため、コメント行には命令文字を書かない。
//!
//! 本ファイルの各テスト関数の上に、対応する不具合の概要をコメントで記す。
//! 各テストは有限のステップ上限を設定し、修正前の不具合で無限ループしても有限時間で失敗するようにする。
//!
//! 初期ケースは、バッチ境界ちょうどのブレークポイントを再現する
//! `debug_batch_boundary_breakpoint.bf` 1 件である。

mod debug_common;

use brainfuck_core::{BatchSize, MachineConfig, NeverCancel, StopReason};
use debug_common::session_from;

/// バッチ境界ちょうどのブレークポイントを再現するフィクスチャ(`+++.`)。
const BATCH_BOUNDARY_BREAKPOINT_SOURCE: &str =
    include_str!("regression/debug_batch_boundary_breakpoint.bf");

/// 回帰テストの命令数上限。フィクスチャは 4 命令で停止するため十分な余裕があり、
/// 不具合で停止しなくなった場合も有限時間で失敗する。
const STEP_LIMIT: u64 = 100;

/// 最初のバッチ命令数。ブレークポイントの命令添字 2 にちょうど一致させる。
const BOUNDARY_BATCH: u64 = 2;

/// バッチ境界ちょうど(`pc = 2`)のブレークポイントを見逃さず `Breakpoint` で停止する。
///
/// 事象: バッチ命令数に到達した位置へ設定したブレークポイントが、停止として報告されず
/// 次のバッチへ持ち越される。原因はブレークポイント照合がバッチ到達判定より後に置かれていたこと。
/// 期待: 幅 2 のバッチは `pc = 2` で実行数 2 のまま `Breakpoint` で停止し、停止時の状態は
/// 出力なし・セル値 2 である。再開後の次のバッチは停止位置の命令を実行して `Halted` になる。
#[test]
fn breakpoint_on_batch_boundary_is_reported_as_breakpoint_not_missed() {
    let mut session = session_from(
        BATCH_BOUNDARY_BREAKPOINT_SOURCE,
        b"",
        MachineConfig::with_max_steps(STEP_LIMIT),
    );
    session
        .breakpoints_mut()
        .insert(2)
        .expect("命令添字 2 はフィクスチャの範囲内");
    let size = BatchSize::new(BOUNDARY_BATCH).expect("バッチ命令数は 1 以上");

    let first = session.run_batch(size, &NeverCancel);
    assert_eq!(
        first.stop,
        Ok(StopReason::Breakpoint),
        "境界の停止は Breakpoint"
    );
    assert_eq!(first.executed, 2, "実行数は 2 のまま");
    assert_eq!(first.snapshot.pc, 2);
    assert_eq!(first.snapshot.current_cell, 2);
    assert_eq!(
        session.machine().output(),
        b"",
        "停止時点では出力はまだ無い"
    );

    let second = session.run_batch(size, &NeverCancel);
    assert_eq!(second.stop, Ok(StopReason::Halted));
    assert_eq!(second.executed, 2, "停止位置の命令を実行して前進する");
    assert_eq!(second.snapshot.current_cell, 3);
    assert_eq!(session.machine().output(), b"\x03");
}
