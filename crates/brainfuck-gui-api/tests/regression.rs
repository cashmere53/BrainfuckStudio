//! 回帰テスト(タスク 9.4、要件 15.7): 発見した不具合を再発から守るための最小再現プログラムを、
//! `DebuggerService` 経由で検証する。
//!
//! core(`brainfuck-core/tests/regression.rs`)と debugger-state(`debug_regression.rs`)の規約を踏襲する。
//! 既存の回帰ファイルは変更しない。
//!
//! # 追加の規約
//!
//! 不具合を修正するたびに、次の手順を行う。
//!
//! 1. `tests/regression/` 配下に、その不具合を再現する最小の `.bf` ファイルを置く。
//!    ファイル名は事象が分かる英小文字とアンダースコアで付ける。
//! 2. 本ファイルに、そのフィクスチャを `include_str!` で読み込み、サービス経由で期待する結果を
//!    検証するテスト関数を追加する。テスト関数名は事象が分かる名前にする。
//!    読み込みは `common::load_ok`、開始は `common::start_request`、バッチ実行は
//!    `DebuggerService::run_batch` を使う。
//!    停止理由・実行数・`pc`・出力などは、必要に応じて `brainfuck_core` の結果(`common::run_core`)とも比べる。
//! 3. フィクスチャ(`.bf`)の先頭に、事象・原因・期待する振る舞いを説明するコメントを書く。
//!    コメントは命令文字(`+`・`-`・`<`・`>`・`.`・`,`・`[`・`]`)を含まない日本語で書く。
//!    命令文字はコメント内に入れると命令として数えられてしまうため、コメント行には命令文字を書かない。
//!
//! 本ファイルの各テスト関数の上に、対応する不具合の概要をコメントで記す。
//! 各テストは有限のステップ上限を設定し、修正前の不具合で無限ループしても有限時間で失敗するようにする。
//!
//! 初期ケースは、バッチ幅ちょうどで正常終了する最小プログラム `batch_boundary_halts.bf` 1 件である。

mod common;

use brainfuck_gui_api::dto::{BatchStopDto, RunBatchRequest, StepLimitDto};
use brainfuck_gui_api::service::DebuggerService;

use common::{load_ok, service, start_request, view};

/// バッチ幅ちょうどで正常終了することを確認するフィクスチャ(`++`)。
const BATCH_BOUNDARY_HALTS_SOURCE: &str = include_str!("regression/batch_boundary_halts.bf");

/// 回帰テストの命令数上限。フィクスチャは 2 命令で停止するため十分な余裕があり、
/// 不具合で停止しなくなった場合も有限時間で失敗する。
const STEP_LIMIT: u64 = 100;

/// 回帰テストで使うバッチ幅。フィクスチャの命令数とちょうど一致させる。
const BOUNDARY_BATCH: u64 = 2;

/// 規約の実証用の初期ケース(不具合は未発見)。バッチ幅がプログラムの命令数にちょうど一致する場合、
/// 末尾で正常終了したことを `BatchReached` ではなく `Halted` として報告することを確認する。
/// 期待: 幅 2 の最初のバッチは実行数 2 で `Halted` を返し、以後の呼び出しは
/// 実行数 0 の `Halted`(何もしない)になる。最終状態は `pc = 2`・実行数 2 で、
/// `brainfuck_core` の `Machine::run` の結果と一致する。
#[test]
fn batch_width_equal_to_program_length_reports_halted_on_that_batch() {
    let service = service();
    let program = load_ok(&service, BATCH_BOUNDARY_HALTS_SOURCE);
    let session = service
        .start_session(start_request(
            program,
            b"",
            StepLimitDto::Limited {
                max_steps: STEP_LIMIT,
            },
            vec![],
        ))
        .expect("session starts")
        .state
        .session_id;

    let first = run_with_width(&service, session, BOUNDARY_BATCH);
    assert_eq!(
        first.stop,
        BatchStopDto::Halted,
        "バッチ幅ちょうどの末尾は BatchReached ではなく Halted"
    );
    assert_eq!(first.executed, 2, "実行数は 2");
    assert_eq!(first.state.snapshot.pc, 2);
    assert_eq!(first.state.snapshot.steps_executed, 2);
    assert!(first.state.snapshot.halted);
    assert_eq!(first.state.current_range, None);

    let second = run_with_width(&service, session, BOUNDARY_BATCH);
    assert_eq!(
        second.stop,
        BatchStopDto::Halted,
        "停止後の呼び出しは Halted のまま"
    );
    assert_eq!(second.executed, 0, "停止後は何も実行しない");
    assert_eq!(second.state.snapshot.steps_executed, 2);

    let core = common::run_core(BATCH_BOUNDARY_HALTS_SOURCE, b"", Some(STEP_LIMIT));
    assert_eq!(core.pc, first.state.snapshot.pc);
    assert_eq!(core.steps, first.state.snapshot.steps_executed);
    assert_eq!(core.dp, first.state.snapshot.dp);
    assert_eq!(core.output, b"");
}

/// 指定したバッチ幅で 1 回 `run_batch` を呼ぶ。
fn run_with_width(
    service: &DebuggerService,
    session_id: u64,
    width: u64,
) -> brainfuck_gui_api::dto::BatchResultDto {
    service
        .run_batch(RunBatchRequest {
            session_id,
            batch_size: Some(width),
            view: view(16, 0),
        })
        .expect("run_batch accepts the request")
}
