//! `DebuggerService` の実行系(2.5)の振る舞い: バッチ実行・Step・リセット・状態取得・テープ窓。
//!
//! 確認する内容: 一連の操作列(開始 → バッチ反復 → ブレークポイント停止/再開 → Step → リセット)、
//! 停止理由の区別(`batchReached` と `error`)、現在の命令の範囲、不正・不明なセッション ID の拒否
//! (状態不変)、出力の差分窓、そして `brainfuck_core` の実行結果との一致。

mod common;

use brainfuck_gui_api::codec;
use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, BatchResultDto, BatchStopDto, PointerDirectionDto, RunBatchRequest,
    RuntimeErrorDto, SessionStateDto, SessionViewRequest, SourcePositionDto, SourceRangeDto,
    StepLimitDto, StepOutcomeDto, StopReasonDto, TapeWindowDto, TapeWindowRequest,
};
use brainfuck_gui_api::limits::{
    MAX_BATCH_SIZE, MAX_INPUT_BYTES, MAX_TAPE_RADIUS, OUTPUT_TAIL_LIMIT,
};
use brainfuck_gui_api::service::DebuggerService;

use common::{load_ok, run_core, run_until_stop, service, start_request, view};

/// 解析可能な実例(コメントを含む)。CLI と同じ結果を期待できる。
const HELLO: &str = include_str!("../../../examples/hello.bf");

fn unlimited() -> StepLimitDto {
    StepLimitDto::Unlimited
}

fn limited(max_steps: u64) -> StepLimitDto {
    StepLimitDto::Limited { max_steps }
}

/// コメントのない 1 行の ASCII ソースで、添字 `index` の命令の範囲。
fn instr_range(index: usize) -> SourceRangeDto {
    SourceRangeDto {
        start: SourcePositionDto {
            byte_offset: index,
            utf16_offset: index,
            line: 1,
            column: index + 1,
        },
        end: SourcePositionDto {
            byte_offset: index + 1,
            utf16_offset: index + 1,
            line: 1,
            column: index + 2,
        },
    }
}

/// 読み込みと開始をまとめて行い、セッション ID を返す。
fn start(service: &DebuggerService, source: &str, limit: StepLimitDto, offsets: Vec<usize>) -> u64 {
    let program = load_ok(service, source);
    service
        .start_session(start_request(program, b"", limit, offsets))
        .expect("session starts")
        .state
        .session_id
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

fn snapshot(service: &DebuggerService, session_id: u64) -> SessionStateDto {
    service
        .get_snapshot(SessionViewRequest {
            session_id,
            view: view(16, 0),
        })
        .expect("get_snapshot accepts the request")
}

fn step(service: &DebuggerService, session_id: u64) -> StepOutcomeDto {
    service
        .step(SessionViewRequest {
            session_id,
            view: view(16, 0),
        })
        .expect("step accepts the request")
        .outcome
}

fn decode(text: &str) -> Vec<u8> {
    codec::decode(text, MAX_INPUT_BYTES).expect("output is valid base64")
}

fn assert_invalid<T: std::fmt::Debug>(result: Result<T, ApiError>, field: &str) {
    let error = result.expect_err("the request is rejected");
    assert_eq!(error.kind, ApiErrorKind::InvalidArgument);
    assert_eq!(error.field.as_deref(), Some(field));
}

fn assert_session_not_found<T: std::fmt::Debug>(result: Result<T, ApiError>) {
    let error = result.expect_err("the request is rejected");
    assert_eq!(error.kind, ApiErrorKind::SessionNotFound);
    assert_eq!(error.field, None);
}

#[test]
fn default_batch_size_is_1000_and_batches_continue_until_halted() {
    let service = service();
    let session = start(&service, &"+".repeat(2_500), unlimited(), vec![]);

    let first = run(&service, session, None);
    assert_eq!(first.stop, BatchStopDto::BatchReached);
    assert_eq!(first.executed, 1_000);
    assert_eq!(first.state.snapshot.steps_executed, 1_000);
    assert_eq!(first.state.snapshot.pc, 1_000);
    assert_eq!(first.state.last_stop, Some(StopReasonDto::BatchReached));
    assert_eq!(first.state.current_range, Some(instr_range(1_000)));

    let second = run(&service, session, None);
    assert_eq!(second.stop, BatchStopDto::BatchReached);
    assert_eq!(second.executed, 1_000);

    let last = run(&service, session, None);
    assert_eq!(last.stop, BatchStopDto::Halted);
    assert_eq!(last.executed, 500);
    assert_eq!(last.state.snapshot.steps_executed, 2_500);
    assert_eq!(last.state.snapshot.pc, 2_500);
    assert_eq!(last.state.snapshot.current_cell, (2_500 % 256) as u8);
    assert!(last.state.snapshot.halted);
    assert_eq!(last.state.last_stop, Some(StopReasonDto::Halted));
    // 停止状態(正常終了)では現在の命令の範囲は null。
    assert_eq!(last.state.current_range, None);
}

#[test]
fn explicit_batch_size_bounds_are_accepted_and_out_of_range_values_are_rejected_without_change() {
    let service = service();
    let session = start(&service, "+++", unlimited(), vec![]);

    let one = run(&service, session, Some(1));
    assert_eq!(one.stop, BatchStopDto::BatchReached);
    assert_eq!(one.executed, 1);

    for bad in [0, MAX_BATCH_SIZE + 1, u64::MAX] {
        assert_invalid(
            service.run_batch(RunBatchRequest {
                session_id: session,
                batch_size: Some(bad),
                view: view(16, 0),
            }),
            "batchSize",
        );
    }
    // 拒否された要求は状態を変えない。
    let state = snapshot(&service, session);
    assert_eq!(state.snapshot.steps_executed, 1);
    assert_eq!(state.snapshot.pc, 1);

    // 上限ちょうどは受理される。
    let rest = run(&service, session, Some(MAX_BATCH_SIZE));
    assert_eq!(rest.stop, BatchStopDto::Halted);
    assert_eq!(rest.executed, 2);
}

#[test]
fn unknown_or_discarded_session_ids_are_rejected_without_changing_state() {
    let service = service();
    let live = start(&service, "+++", unlimited(), vec![]);
    run(&service, live, Some(1));

    for id in [0, live + 1_000] {
        assert_session_not_found(service.run_batch(RunBatchRequest {
            session_id: id,
            batch_size: None,
            view: view(16, 0),
        }));
        assert_session_not_found(service.step(SessionViewRequest {
            session_id: id,
            view: view(16, 0),
        }));
        assert_session_not_found(service.reset(SessionViewRequest {
            session_id: id,
            view: view(16, 0),
        }));
        assert_session_not_found(service.get_snapshot(SessionViewRequest {
            session_id: id,
            view: view(16, 0),
        }));
        assert_session_not_found(service.get_tape_window(TapeWindowRequest {
            session_id: id,
            radius: 4,
        }));
    }
    // 不明な ID の要求の後も、生きているセッションは従前のまま。
    assert_eq!(snapshot(&service, live).snapshot.steps_executed, 1);

    // 読み込みは既存のセッションを破棄する。古い ID は以後 sessionNotFound。
    load_ok(&service, "+++");
    assert_session_not_found(service.get_snapshot(SessionViewRequest {
        session_id: live,
        view: view(16, 0),
    }));
    assert_session_not_found(service.run_batch(RunBatchRequest {
        session_id: live,
        batch_size: None,
        view: view(16, 0),
    }));
}

#[test]
fn rejected_start_session_keeps_the_running_session_usable() {
    let service = service();
    let program = load_ok(&service, "+++");
    let live = service
        .start_session(start_request(program, b"", unlimited(), vec![]))
        .expect("session starts")
        .state
        .session_id;
    run(&service, live, Some(1));

    let mut bad_view = start_request(program, b"", unlimited(), vec![]);
    bad_view.view = view(65, 0);
    assert_invalid(service.start_session(bad_view), "tapeRadius");

    assert_invalid(
        service.start_session(start_request(program, b"", limited(0), vec![])),
        "maxSteps",
    );

    // 拒否の後も生きているセッションは同じ ID・同じ状態で動く。
    let state = snapshot(&service, live);
    assert_eq!(state.session_id, live);
    assert_eq!(state.snapshot.steps_executed, 1);
    let rest = run(&service, live, None);
    assert_eq!(rest.stop, BatchStopDto::Halted);
    assert_eq!(rest.state.session_id, live);
}

#[test]
fn breakpoint_stops_before_the_instruction_and_the_next_run_passes_it() {
    let service = service();
    let session = start(&service, "+++", unlimited(), vec![1]);

    let stopped = run(&service, session, None);
    assert_eq!(stopped.stop, BatchStopDto::Breakpoint);
    assert_eq!(stopped.executed, 1);
    assert_eq!(stopped.state.snapshot.pc, 1);
    assert_eq!(stopped.state.snapshot.steps_executed, 1);
    assert_eq!(stopped.state.last_stop, Some(StopReasonDto::Breakpoint));
    // 停止した命令(ブレークポイントの位置)が現在の命令として強調される。
    assert_eq!(stopped.state.current_range, Some(instr_range(1)));

    // 同じ位置で止まり続けず、その命令を実行して先へ進む。
    let finished = run(&service, session, None);
    assert_eq!(finished.stop, BatchStopDto::Halted);
    assert_eq!(finished.executed, 2);
    assert_eq!(finished.state.snapshot.steps_executed, 3);
    assert_eq!(finished.state.snapshot.current_cell, 3);
}

#[test]
fn breakpoint_inside_a_loop_stops_on_every_iteration_and_matches_the_core_total() {
    let service = service();
    // 添字 3 の `-` にブレークポイント。ループは 2 回回る。
    let session = start(&service, "++[-]", unlimited(), vec![3]);

    let mut stops = Vec::new();
    loop {
        let result = run(&service, session, None);
        stops.push(result.stop.clone());
        if result.stop == BatchStopDto::Halted {
            break;
        }
        assert_eq!(result.stop, BatchStopDto::Breakpoint);
        assert_eq!(result.state.snapshot.pc, 3);
        assert!(stops.len() < 10, "the loop must end");
    }
    assert_eq!(
        stops,
        vec![
            BatchStopDto::Breakpoint,
            BatchStopDto::Breakpoint,
            BatchStopDto::Halted
        ]
    );

    let core = run_core("++[-]", b"", None);
    let state = snapshot(&service, session);
    assert_eq!(state.snapshot.steps_executed, core.steps);
    assert_eq!(state.snapshot.pc, core.pc);
}

#[test]
fn step_ignores_breakpoints_and_executes_exactly_one_instruction() {
    let service = service();
    let session = start(&service, "+++", unlimited(), vec![0, 1]);

    // ブレークポイントが次の命令にあっても Step はそれを実行する。
    let first = step(&service, session);
    assert_eq!(first, StepOutcomeDto::Executed);
    let state = snapshot(&service, session);
    assert_eq!(state.snapshot.pc, 1);
    assert_eq!(state.snapshot.steps_executed, 1);

    let second = step(&service, session);
    assert_eq!(second, StepOutcomeDto::Executed);
    let state = snapshot(&service, session);
    assert_eq!(state.snapshot.pc, 2);
    assert_eq!(state.snapshot.steps_executed, 2);
    assert_eq!(state.snapshot.current_cell, 2);
}

#[test]
fn step_onto_the_end_reports_halted_and_clears_the_current_range() {
    let service = service();
    let session = start(&service, "+", unlimited(), vec![]);

    assert_eq!(step(&service, session), StepOutcomeDto::Executed);
    let state = snapshot(&service, session);
    assert!(state.snapshot.halted);
    assert_eq!(state.last_stop, Some(StopReasonDto::Halted));
    assert_eq!(state.current_range, None);

    // 停止済みの Step は機械を変えず Halted を返す。
    assert_eq!(step(&service, session), StepOutcomeDto::Halted);
    let after = snapshot(&service, session);
    assert_eq!(after.snapshot.steps_executed, 1);
}

#[test]
fn run_batch_on_a_halted_session_is_a_no_op() {
    let service = service();
    let session = start(&service, "+", unlimited(), vec![]);
    assert_eq!(run(&service, session, None).stop, BatchStopDto::Halted);

    let again = run(&service, session, None);
    assert_eq!(again.stop, BatchStopDto::Halted);
    assert_eq!(again.executed, 0);
    assert_eq!(again.state.snapshot.steps_executed, 1);
}

#[test]
fn execution_error_reports_the_failing_instruction_and_keeps_the_state_until_reset() {
    let service = service();
    // 3 番目の `<` で範囲外へ移動する。
    let session = start(&service, "+<", unlimited(), vec![]);

    let failed = run(&service, session, None);
    assert_eq!(
        failed.stop,
        BatchStopDto::Error {
            error: RuntimeErrorDto::PointerOutOfBounds {
                pc: 1,
                dp: 0,
                direction: PointerDirectionDto::Left,
            },
        }
    );
    assert_eq!(failed.executed, 1);
    assert_eq!(failed.state.snapshot.pc, 1);
    assert_eq!(failed.state.snapshot.dp, 0);
    assert_eq!(failed.state.snapshot.current_cell, 1);
    assert_eq!(failed.state.snapshot.steps_executed, 1);
    assert!(!failed.state.snapshot.halted);
    assert_eq!(failed.state.last_stop, None);
    // 実行エラーでは失敗した命令(`<`)が現在の命令の範囲になる。
    assert_eq!(failed.state.current_range, Some(instr_range(1)));

    // 状態は保持されるので、もう一度 Run しても同じエラー。Reset で初期状態へ戻る。
    let again = run(&service, session, None);
    assert_eq!(again.executed, 0);
    assert!(matches!(again.stop, BatchStopDto::Error { .. }));

    let reset = service
        .reset(SessionViewRequest {
            session_id: session,
            view: view(16, 0),
        })
        .expect("reset accepts the session");
    assert_eq!(reset.snapshot.steps_executed, 0);
    assert_eq!(reset.snapshot.pc, 0);
    let rerun = run(&service, session, None);
    assert_eq!(rerun.stop, failed.stop);
    assert_eq!(rerun.executed, 1);
}

#[test]
fn step_limit_error_is_distinct_from_batch_reached_and_keeps_the_machine() {
    let service = service();
    let session = start(&service, "+++++", limited(3), vec![]);

    let first = run(&service, session, Some(2));
    assert_eq!(first.stop, BatchStopDto::BatchReached);
    assert_eq!(first.executed, 2);

    // 2 回目は 1 命令実行した後、上限超過で止まる。バッチ到達とは別のラベル。
    let second = run(&service, session, Some(2));
    assert_eq!(
        second.stop,
        BatchStopDto::Error {
            error: RuntimeErrorDto::StepLimitExceeded { limit: 3, pc: 3 },
        }
    );
    assert_eq!(second.executed, 1);
    assert_eq!(second.state.snapshot.steps_executed, 3);
    assert_eq!(second.state.snapshot.pc, 3);

    // Step も同じエラーになり、状態は変わらない。
    assert_eq!(
        step(&service, session),
        StepOutcomeDto::Error {
            error: RuntimeErrorDto::StepLimitExceeded { limit: 3, pc: 3 },
        }
    );
    assert_eq!(snapshot(&service, session).snapshot.steps_executed, 3);

    // Reset で復帰できる。
    service
        .reset(SessionViewRequest {
            session_id: session,
            view: view(16, 0),
        })
        .expect("reset accepts the session");
    let rerun = run(&service, session, None);
    assert_eq!(rerun.executed, 3);
    assert!(matches!(rerun.stop, BatchStopDto::Error { .. }));
}

#[test]
fn reset_returns_to_the_initial_state_clears_output_and_keeps_breakpoints() {
    let service = service();
    // `.` で 1 バイト出力し、添字 2 にブレークポイント。
    let session = start(&service, "+.+", unlimited(), vec![2]);

    let stopped = run(&service, session, None);
    assert_eq!(stopped.stop, BatchStopDto::Breakpoint);
    assert_eq!(stopped.state.output.total_len, 1);
    assert_eq!(decode(&stopped.state.output.data_base64), vec![1]);

    let reset = service
        .reset(SessionViewRequest {
            session_id: session,
            view: view(16, 0),
        })
        .expect("reset accepts the session");
    assert_eq!(reset.session_id, session);
    assert_eq!(reset.snapshot.pc, 0);
    assert_eq!(reset.snapshot.dp, 0);
    assert_eq!(reset.snapshot.current_cell, 0);
    assert_eq!(reset.snapshot.steps_executed, 0);
    assert!(!reset.snapshot.halted);
    assert_eq!(reset.output.total_len, 0);
    assert_eq!(reset.output.data_base64, "");
    assert_eq!(reset.last_stop, None);
    assert_eq!(reset.current_range, Some(instr_range(0)));
    assert_eq!(reset.tape_window.cells, vec![0; 17]);

    // ブレークポイントは保持される。再び同じ位置で止まる。
    let again = run(&service, session, None);
    assert_eq!(again.stop, BatchStopDto::Breakpoint);
    assert_eq!(again.executed, 2);
    assert_eq!(again.state.snapshot.pc, 2);
    assert_eq!(again.state.output.total_len, 1);
}

#[test]
fn tape_window_follows_the_pointer_clips_at_the_edge_and_rejects_bad_radius() {
    let service = service();
    // +>++ : dp = 1、セル 0 = 1、セル 1 = 2。
    let session = start(&service, "+>++", unlimited(), vec![]);
    run(&service, session, None);

    let radius_one = service
        .get_tape_window(TapeWindowRequest {
            session_id: session,
            radius: 1,
        })
        .expect("radius 1 is accepted");
    assert_eq!(
        radius_one,
        TapeWindowDto {
            start: 0,
            cells: vec![1, 2, 0],
            pointer_offset: 1,
        }
    );

    // 半径 0 は指すセルだけ。
    let radius_zero = service
        .get_tape_window(TapeWindowRequest {
            session_id: session,
            radius: 0,
        })
        .expect("radius 0 is accepted");
    assert_eq!(radius_zero.start, 1);
    assert_eq!(radius_zero.cells, vec![2]);
    assert_eq!(radius_zero.pointer_offset, 0);

    // 半径 64 は上限ちょうどで受理され、左端は 0 で切り詰める(dp 1 → 窓は 0..=65 の 66 セル)。
    let radius_max = service
        .get_tape_window(TapeWindowRequest {
            session_id: session,
            radius: 64,
        })
        .expect("radius 64 is accepted");
    assert_eq!(radius_max.start, 0);
    assert_eq!(radius_max.cells.len(), 66);
    assert_eq!(radius_max.pointer_offset, 1);

    assert_invalid(
        service.get_tape_window(TapeWindowRequest {
            session_id: session,
            radius: 65,
        }),
        "radius",
    );
}

#[test]
fn output_window_returns_the_unread_suffix_and_clips_a_stale_position() {
    let service = service();
    let session = start(&service, "+++++.", unlimited(), vec![]);
    run(&service, session, None);

    let from_start = service
        .get_snapshot(SessionViewRequest {
            session_id: session,
            view: view(16, 0),
        })
        .expect("snapshot accepts the request");
    assert_eq!(from_start.output.total_len, 1);
    assert_eq!(from_start.output.data_start, 0);
    assert_eq!(decode(&from_start.output.data_base64), vec![5]);

    // 既読の位置 1 以降は差分なし。
    let caught_up = service
        .get_snapshot(SessionViewRequest {
            session_id: session,
            view: view(16, 1),
        })
        .expect("snapshot accepts the request");
    assert_eq!(caught_up.output.data_start, 1);
    assert_eq!(caught_up.output.data_base64, "");

    // 総数より大きい位置(リセット後の古い位置)は末尾の窓を先頭から返す。
    let stale = service
        .get_snapshot(SessionViewRequest {
            session_id: session,
            view: view(16, 99),
        })
        .expect("snapshot accepts the request");
    assert_eq!(stale.output.data_start, 0);
    assert_eq!(decode(&stale.output.data_base64), vec![5]);
}

#[test]
fn output_chunk_keeps_only_the_last_64_kib_and_reset_empties_it() {
    let service = service();
    // `+` の後に 70,000 個の `.`。すべて 0x01 を出力する。
    let source = format!("+{}", ".".repeat(70_000));
    let session = start(&service, &source, unlimited(), vec![]);

    let result = run(&service, session, Some(1_000_000));
    assert_eq!(result.stop, BatchStopDto::Halted);
    assert_eq!(result.state.output.total_len, 70_000);
    assert_eq!(result.state.output.data_start, 70_000 - OUTPUT_TAIL_LIMIT);
    let bytes = decode(&result.state.output.data_base64);
    assert_eq!(bytes.len(), OUTPUT_TAIL_LIMIT);
    assert!(bytes.iter().all(|&b| b == 1));

    let reset = service
        .reset(SessionViewRequest {
            session_id: session,
            view: view(16, 0),
        })
        .expect("reset accepts the session");
    assert_eq!(reset.output.total_len, 0);
    assert_eq!(reset.output.data_base64, "");
}

#[test]
fn non_utf8_output_bytes_are_carried_unchanged_in_the_base64_chunk() {
    let service = service();
    // `+` を 255 個してから `.` で 0xFF を出力する(UTF-8 として不正なバイト)。
    let source = format!("{}.", "+".repeat(255));
    let session = start(&service, &source, unlimited(), vec![]);

    let result = run(&service, session, None);
    assert_eq!(result.stop, BatchStopDto::Halted);
    assert_eq!(result.state.output.total_len, 1);
    assert_eq!(result.state.output.data_start, 0);
    assert_eq!(decode(&result.state.output.data_base64), vec![0xFF]);
    // 0xFF は置換されず、そのまま 1 バイトとして返る。
    assert_eq!(result.state.output.data_base64, codec::encode(&[0xFF]));
}

#[test]
fn every_batch_width_reaches_the_same_final_state_as_the_core() {
    let core = run_core(HELLO, b"", None);
    assert!(core.result.is_ok(), "hello.bf halts normally");

    for width in [1, 7, 1_000, 1_000_000] {
        let service = service();
        let session = start(&service, HELLO, unlimited(), vec![]);
        let finished = run_until_stop(&service, session, Some(width), 1_000_000);
        assert_eq!(finished.last.stop, BatchStopDto::Halted, "width {width}");
        assert_eq!(finished.executed_total as u64, core.steps, "width {width}");

        let state = &finished.last.state;
        assert_eq!(state.snapshot.pc, core.pc, "width {width}");
        assert_eq!(state.snapshot.dp, core.dp, "width {width}");
        assert_eq!(state.snapshot.steps_executed, core.steps, "width {width}");
        assert_eq!(
            decode(&state.output.data_base64),
            core.output,
            "width {width}"
        );

        // テープ全体: 表示窓の各セルは core のテープの同じ位置と一致する(範囲外は 0)。
        let window = service
            .get_tape_window(TapeWindowRequest {
                session_id: session,
                radius: MAX_TAPE_RADIUS,
            })
            .expect("radius 64 is accepted");
        for (offset, &cell) in window.cells.iter().enumerate() {
            let expected = core.tape.get(window.start + offset).copied().unwrap_or(0);
            assert_eq!(
                cell,
                expected,
                "width {width}, tape cell {}",
                window.start + offset
            );
        }
        assert_eq!(
            window.cells[window.pointer_offset],
            core.tape.get(core.dp).copied().unwrap_or(0),
            "width {width}"
        );
    }
    assert_eq!(core.output, b"Hello World!\n");
    assert_eq!((core.pc, core.dp, core.steps), (106, 6, 906));
}

#[test]
fn execution_error_matches_the_core_for_every_batch_width() {
    let core = run_core("++>+<<", b"", None);
    for width in [1, 2, 1_000] {
        let service = service();
        let session = start(&service, "++>+<<", unlimited(), vec![]);
        let finished = run_until_stop(&service, session, Some(width), 1_000);
        assert_eq!(
            finished.last.stop,
            BatchStopDto::Error {
                error: RuntimeErrorDto::PointerOutOfBounds {
                    pc: 5,
                    dp: 0,
                    direction: PointerDirectionDto::Left,
                },
            },
            "width {width}"
        );
        assert_eq!(
            finished.last.state.snapshot.steps_executed, core.steps,
            "width {width}"
        );
        assert_eq!(finished.last.state.snapshot.pc, core.pc, "width {width}");
    }
}

#[test]
fn step_limit_matches_the_core_for_every_batch_width() {
    let core = run_core("+++++", b"", Some(3));
    for width in [1, 2, 1_000] {
        let service = service();
        let session = start(&service, "+++++", limited(3), vec![]);
        let finished = run_until_stop(&service, session, Some(width), 1_000);
        assert_eq!(
            finished.last.stop,
            BatchStopDto::Error {
                error: RuntimeErrorDto::StepLimitExceeded { limit: 3, pc: 3 },
            },
            "width {width}"
        );
        assert_eq!(finished.executed_total as u64, core.steps, "width {width}");
    }
}

#[test]
fn stopped_state_has_a_null_range_and_a_running_one_has_the_current_instruction() {
    let service = service();
    let session = start(&service, "+.", unlimited(), vec![]);

    let started = snapshot(&service, session);
    assert_eq!(started.current_range, Some(instr_range(0)));
    assert_eq!(started.last_stop, None);

    run(&service, session, Some(1));
    let mid = snapshot(&service, session);
    assert_eq!(mid.current_range, Some(instr_range(1)));

    run(&service, session, None);
    let done = snapshot(&service, session);
    assert!(done.snapshot.halted);
    assert_eq!(done.current_range, None);
    assert_eq!(done.last_stop, Some(StopReasonDto::Halted));
}

#[test]
fn an_over_wide_tape_radius_rejects_every_session_call_without_moving_the_machine() {
    let service = service();
    let session = start(&service, "+++++", unlimited(), vec![]);
    run(&service, session, Some(2));

    // 4 つの呼び出しとも tapeRadius(65 > 64)で拒否される。
    assert_invalid(
        service.run_batch(RunBatchRequest {
            session_id: session,
            batch_size: Some(1),
            view: view(65, 0),
        }),
        "tapeRadius",
    );
    assert_invalid(
        service.step(SessionViewRequest {
            session_id: session,
            view: view(65, 0),
        }),
        "tapeRadius",
    );
    assert_invalid(
        service.reset(SessionViewRequest {
            session_id: session,
            view: view(65, 0),
        }),
        "tapeRadius",
    );
    assert_invalid(
        service.get_snapshot(SessionViewRequest {
            session_id: session,
            view: view(65, 0),
        }),
        "tapeRadius",
    );

    // 拒否の後も機械は進んでおらず、リセットもされていない(実行を進めた状態のまま)。
    let state = snapshot(&service, session);
    assert_eq!(state.snapshot.pc, 2);
    assert_eq!(state.snapshot.steps_executed, 2);
    assert_eq!(state.snapshot.current_cell, 2);
    assert_eq!(state.last_stop, Some(StopReasonDto::BatchReached));

    let rest = run(&service, session, None);
    assert_eq!(rest.stop, BatchStopDto::Halted);
    assert_eq!(rest.executed, 3);
    assert_eq!(rest.state.snapshot.steps_executed, 5);
}

#[test]
fn reset_after_input_restores_the_input_position_and_reads_the_same_bytes_again() {
    let service = service();
    // `,` で読み、`.` で出力する。入力は "AB"。
    let program = load_ok(&service, ",.,.");
    let session = service
        .start_session(start_request(program, b"AB", unlimited(), vec![]))
        .expect("session starts")
        .state
        .session_id;

    // 途中まで実行: 入力を 1 バイト読んで出力している。
    let partial = run(&service, session, Some(2));
    assert_eq!(partial.state.input_consumed, 1);
    assert_eq!(partial.state.input_total, 2);
    assert_eq!(decode(&partial.state.output.data_base64), vec![b'A']);

    let reset = service
        .reset(SessionViewRequest {
            session_id: session,
            view: view(16, 0),
        })
        .expect("reset accepts the session");
    assert_eq!(reset.input_consumed, 0);
    assert_eq!(reset.input_total, 2);
    assert_eq!(reset.snapshot.steps_executed, 0);
    assert_eq!(reset.output.total_len, 0);

    // 再実行は入力を先頭から読み直す。
    let again = run(&service, session, None);
    assert_eq!(again.stop, BatchStopDto::Halted);
    assert_eq!(again.state.input_consumed, 2);
    assert_eq!(again.state.input_total, 2);
    assert_eq!(decode(&again.state.output.data_base64), b"AB".to_vec());
}
