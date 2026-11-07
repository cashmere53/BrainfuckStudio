//! `DebuggerService` のブレークポイント置換(`set_breakpoints`)と一時停止(`pause`)の振る舞い(2.6)。
//!
//! 確認する内容: 置換の解決結果・未解決・冪等性、要求の拒否(状態不変)、上限の境界、
//! 実行中バッチの `cancelled` での停止(`Mutex` 保持中でも `pause` がブロックしないこと)、
//! バッチ間に届いた旗が次の `run_batch` 入口で解除されること、未知/古い `sessionId` の `pause` が無視されること。
//!
//! 別スレッドは本テストコード内でのみ使う(本番コードではスレッドを生成しない)。

mod common;

use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, BatchResultDto, BatchStopDto, BreakpointsDto, PauseRequest,
    RunBatchRequest, SetBreakpointsRequest, StepLimitDto,
};
use brainfuck_gui_api::limits::{MAX_BATCH_SIZE, MAX_BREAKPOINT_OFFSETS};
use brainfuck_gui_api::service::DebuggerService;

use common::{load_ok, service, start_request, view};

/// 無限ループ。命令数上限なしで実行すると止まらない。
const FOREVER: &str = "+[]";

/// 命令 0 `+`(オフセット 0)、命令 1 `+`(オフセット 4)。`x` はコメント(オフセット 2)。
const TWO_PLUS: &str = "+ x\n+";

fn start(service: &DebuggerService, program_id: u64, breakpoints: Vec<usize>) -> u64 {
    service
        .start_session(start_request(
            program_id,
            b"",
            StepLimitDto::Unlimited,
            breakpoints,
        ))
        .expect("session starts")
        .state
        .session_id
}

fn set(
    service: &DebuggerService,
    session_id: u64,
    offsets: Vec<usize>,
) -> Result<BreakpointsDto, ApiError> {
    service.set_breakpoints(SetBreakpointsRequest {
        session_id,
        offsets,
    })
}

fn run(service: &DebuggerService, session_id: u64, batch_size: Option<u64>) -> BatchResultDto {
    service
        .run_batch(RunBatchRequest {
            session_id,
            batch_size,
            view: view(16, 0),
        })
        .expect("run_batch accepts the request")
}

#[test]
fn set_breakpoints_resolves_to_instructions_and_reports_unresolved_offsets() {
    let service = service();
    let program = load_ok(&service, TWO_PLUS);
    let session = start(&service, program, vec![]);

    // オフセット 2 はコメントなので次の命令(添字 1)へ解決する。99 は命令が無い。
    let result = set(&service, session, vec![2, 99, 0]).expect("the request is accepted");

    let resolved: Vec<(usize, usize)> = result
        .breakpoints
        .iter()
        .map(|bp| (bp.instruction_index, bp.utf16_offset))
        .collect();
    assert_eq!(resolved, vec![(0, 0), (1, 2)]);
    assert_eq!(result.unresolved, vec![99]);
}

#[test]
fn offsets_resolving_to_one_instruction_collapse_and_keep_the_first_request_offset() {
    let service = service();
    let program = load_ok(&service, TWO_PLUS);
    let session = start(&service, program, vec![]);

    // オフセット 2(`x`)と 3(`\n`)はどちらも命令 1 へ解決する。要求順で最初の値を採る。
    let first_is_three = set(&service, session, vec![3, 2]).expect("accepted");
    assert_eq!(first_is_three.breakpoints.len(), 1);
    assert_eq!(first_is_three.breakpoints[0].instruction_index, 1);
    assert_eq!(first_is_three.breakpoints[0].utf16_offset, 3);

    let first_is_two = set(&service, session, vec![2, 3]).expect("accepted");
    assert_eq!(first_is_two.breakpoints.len(), 1);
    assert_eq!(first_is_two.breakpoints[0].utf16_offset, 2);
}

#[test]
fn resending_the_same_offsets_returns_the_same_result() {
    let service = service();
    let program = load_ok(&service, TWO_PLUS);
    let session = start(&service, program, vec![]);

    let first = set(&service, session, vec![2, 99, 0]).expect("accepted");
    let second = set(&service, session, vec![2, 99, 0]).expect("accepted");
    assert_eq!(first, second);
}

#[test]
fn replacing_the_set_removes_the_old_breakpoints_and_keeps_the_new_ones() {
    let service = service();
    // 命令 0〜2 は `+`、命令 3 は `.`(オフセット 0, 1, 2, 3)。
    let program = load_ok(&service, "+++.");
    let session = start(&service, program, vec![2]);

    set(&service, session, vec![1]).expect("accepted");
    let stopped = run(&service, session, None);
    assert_eq!(stopped.stop, BatchStopDto::Breakpoint);
    assert_eq!(
        stopped.state.snapshot.pc, 1,
        "the new breakpoint at 1 is hit first"
    );

    // 空の置換で残りのブレークポイントがすべて消える(旧 2 も新 1 も残らない)。
    set(&service, session, vec![]).expect("accepted");
    let finished = run(&service, session, None);
    assert_eq!(finished.stop, BatchStopDto::Halted);
    assert_eq!(finished.state.snapshot.steps_executed, 4);
}

#[test]
fn set_breakpoints_is_rejected_for_unknown_discarded_and_stale_sessions_without_changing_state() {
    let service = service();
    let program = load_ok(&service, "+++");
    let session = start(&service, program, vec![2]);

    for bad in [0, session + 1_000] {
        let error = set(&service, bad, vec![]).expect_err("unknown session is rejected");
        assert_eq!(error.kind, ApiErrorKind::SessionNotFound);
    }
    // 拒否された置換は既存のブレークポイント(2)を変えない。
    let stopped = run(&service, session, None);
    assert_eq!(stopped.stop, BatchStopDto::Breakpoint);
    assert_eq!(stopped.state.snapshot.pc, 2);

    // 読み込み直しで旧セッションは破棄される。
    load_ok(&service, "+++");
    let error = set(&service, session, vec![]).expect_err("discarded session is rejected");
    assert_eq!(error.kind, ApiErrorKind::SessionNotFound);
    assert_eq!(error.field, None);
}

#[test]
fn offset_list_over_the_limit_is_rejected_and_the_limit_itself_is_accepted() {
    let service = service();
    let program = load_ok(&service, "+++");
    let session = start(&service, program, vec![2]);

    let too_many = vec![0; MAX_BREAKPOINT_OFFSETS + 1];
    let error = set(&service, session, too_many).expect_err("over the limit");
    assert_eq!(error.kind, ApiErrorKind::InvalidArgument);
    assert_eq!(error.field.as_deref(), Some("offsets"));
    // 拒否の後も元の置換(2)が残っている。
    assert_eq!(run(&service, session, None).stop, BatchStopDto::Breakpoint);

    // 上限ちょうどは受け付ける(重複は 1 件に畳まれる)。
    let at_limit = set(&service, session, vec![0; MAX_BREAKPOINT_OFFSETS]).expect("accepted");
    assert_eq!(at_limit.breakpoints.len(), 1);
}

#[test]
fn pause_stops_a_running_batch_with_cancelled_while_the_batch_holds_the_lock() {
    let service = service();
    let program = load_ok(&service, FOREVER);
    let session = start(&service, program, vec![]);

    // バッチが `Mutex` を保持したまま走っている間に、別スレッドの `pause` が止めること。
    // `pause` がロックを取るなら、このテストは完了しない(期限で失敗させる)。
    // ランナーは `batchReached` の間バッチを繰り返し、`cancelled` 以外で止まったら失敗とする。
    // 一時停止の到着時刻に依存せず、一時停止が届いたバッチが必ず `cancelled` で終わる。
    let result = thread::scope(|scope| {
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let shared: &DebuggerService = &service;
        let runner = scope.spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            let result = loop {
                let result = run(shared, session, Some(MAX_BATCH_SIZE));
                if result.stop != BatchStopDto::BatchReached {
                    break result;
                }
                assert!(
                    Instant::now() < deadline,
                    "no pause reached a running batch within 60 seconds"
                );
            };
            done_tx.send(()).ok();
            result
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        // 完了(送信またはランナーの破棄)まで送り続ける。
        while let Err(TryRecvError::Empty) = done_rx.try_recv() {
            assert!(
                Instant::now() < deadline,
                "pause never reached the running batch"
            );
            service.pause(PauseRequest {
                session_id: session,
            });
            thread::sleep(Duration::from_millis(1));
        }
        runner.join().expect("the runner thread does not panic")
    });

    assert_eq!(result.stop, BatchStopDto::Cancelled);
    assert!(
        (result.executed as u64) < MAX_BATCH_SIZE,
        "the batch stopped before the full width"
    );
    assert_eq!(result.state.session_id, session);
}

#[test]
fn a_pause_before_the_next_run_is_cleared_at_the_run_entry() {
    let service = service();
    let program = load_ok(&service, "+.");
    let session = start(&service, program, vec![]);

    // バッチの外に届いた旗は、次の `run_batch` の入口で解除される(前回の Pause の残りで即中断しない)。
    service.pause(PauseRequest {
        session_id: session,
    });
    let finished = run(&service, session, None);
    assert_eq!(finished.stop, BatchStopDto::Halted);
    assert_eq!(finished.executed, 2);
}

#[test]
fn pause_with_an_unknown_or_stale_session_id_does_not_stop_the_live_session() {
    let service = service();
    let old_program = load_ok(&service, FOREVER);
    let stale = start(&service, old_program, vec![]);
    let new_program = load_ok(&service, FOREVER);
    let live = start(&service, new_program, vec![]);
    assert_ne!(stale, live);

    // 古い ID・未知の ID・未開始(0)の Pause を、実行中のバッチの間じゅう送り続ける。
    // 戻り値はなく、エラーも返さない。新しいセッションのバッチは最後まで走る。
    let result = thread::scope(|scope| {
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let shared: &DebuggerService = &service;
        let runner = scope.spawn(move || {
            let result = run(shared, live, Some(MAX_BATCH_SIZE));
            done_tx.send(()).ok();
            result
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        while let Err(TryRecvError::Empty) = done_rx.try_recv() {
            assert!(Instant::now() < deadline, "the runner never finished");
            for session_id in [stale, live + 1_000, 0] {
                service.pause(PauseRequest { session_id });
            }
            thread::sleep(Duration::from_millis(1));
        }
        runner.join().expect("the runner thread does not panic")
    });

    assert_eq!(result.stop, BatchStopDto::BatchReached);
    assert_eq!(result.executed as u64, MAX_BATCH_SIZE);
    assert_eq!(result.state.session_id, live);
}

#[test]
fn default_batch_width_is_unchanged_by_the_pause_path() {
    // 既定幅の実行が、旗を立てない限り batchReached で止まることの確認(Pause の経路の回帰防止)。
    let service = service();
    let program = load_ok(&service, FOREVER);
    let session = start(&service, program, vec![]);
    let result = run(&service, session, None);
    assert_eq!(result.stop, BatchStopDto::BatchReached);
    assert_eq!(result.executed, 1_000);
}
