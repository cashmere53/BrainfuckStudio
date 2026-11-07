//! 上流の値 → DTO の変換層(`convert` の状態・停止・エラー・テープ窓・出力差分)の契約。
//!
//! 上流の計算規則(窓の切り詰め・位置規則)は検査しない。ここでは次だけを確認する:
//! - 停止理由・実行エラー・Step 結果を、取り違えなく別のラベルへ写すこと
//!   (バッチ到達と命令数上限超過のエラーは別ラベル)
//! - 出力差分の開始位置の規則(連続 / 要求位置超過 / 末尾 65,536 バイト)
//! - 状態応答の各フィールドが上流の値から写されること

use brainfuck_core::{
    tape_window_of, BatchSize, DebugSession, Machine, MachineConfig, MachineSnapshot, NeverCancel,
    PointerDirection, Program, RuntimeError, SourceMap, StateView, StepResult, StopReason,
    TAPE_SIZE,
};
use brainfuck_gui_api::codec;
use brainfuck_gui_api::convert::{
    batch_result_dto, batch_stop_dto, executed_dto, last_stop_dto, output_chunk, runtime_error_dto,
    session_state_dto, snapshot_dto, source_range_dto, step_outcome_dto, tape_window_dto,
};
use brainfuck_gui_api::dto::{
    BatchStopDto, PointerDirectionDto, RuntimeErrorDto, SnapshotDto, StepOutcomeDto, StopReasonDto,
    TapeWindowDto,
};
const OUTPUT_TAIL: usize = 65_536;

fn decoded(base64: &str) -> Vec<u8> {
    codec::decode(base64, 10_000_000).expect("valid base64")
}

// ---- 停止理由・バッチ結果 ----

#[test]
fn stop_reasons_map_to_distinct_labels() {
    assert_eq!(last_stop_dto(None), Ok(None));
    assert_eq!(
        last_stop_dto(Some(StopReason::Halted)),
        Ok(Some(StopReasonDto::Halted))
    );
    assert_eq!(
        last_stop_dto(Some(StopReason::Breakpoint)),
        Ok(Some(StopReasonDto::Breakpoint))
    );
    assert_eq!(
        last_stop_dto(Some(StopReason::Cancelled)),
        Ok(Some(StopReasonDto::Cancelled))
    );
    // 上流の「バッチ幅に到達」は batchReached として写す。
    assert_eq!(
        last_stop_dto(Some(StopReason::StepLimitReached)),
        Ok(Some(StopReasonDto::BatchReached))
    );
}

#[test]
fn batch_ok_stops_map_to_batch_stop_labels() {
    assert_eq!(
        batch_stop_dto(Ok(StopReason::Halted)),
        Ok(BatchStopDto::Halted)
    );
    assert_eq!(
        batch_stop_dto(Ok(StopReason::Breakpoint)),
        Ok(BatchStopDto::Breakpoint)
    );
    assert_eq!(
        batch_stop_dto(Ok(StopReason::Cancelled)),
        Ok(BatchStopDto::Cancelled)
    );
    assert_eq!(
        batch_stop_dto(Ok(StopReason::StepLimitReached)),
        Ok(BatchStopDto::BatchReached)
    );
}

#[test]
fn runtime_errors_map_field_by_field() {
    assert_eq!(
        runtime_error_dto(&RuntimeError::PointerOutOfBounds {
            pc: 4,
            dp: 0,
            direction: PointerDirection::Left,
        }),
        Ok(RuntimeErrorDto::PointerOutOfBounds {
            pc: 4,
            dp: 0,
            direction: PointerDirectionDto::Left,
        })
    );
    assert_eq!(
        runtime_error_dto(&RuntimeError::PointerOutOfBounds {
            pc: 1,
            dp: TAPE_SIZE - 1,
            direction: PointerDirection::Right,
        }),
        Ok(RuntimeErrorDto::PointerOutOfBounds {
            pc: 1,
            dp: TAPE_SIZE - 1,
            direction: PointerDirectionDto::Right,
        })
    );
    assert_eq!(
        runtime_error_dto(&RuntimeError::StepLimitExceeded { limit: 3, pc: 3 }),
        Ok(RuntimeErrorDto::StepLimitExceeded { limit: 3, pc: 3 })
    );
}

#[test]
fn batch_error_stop_wraps_runtime_error_and_is_not_batch_reached() {
    let stop =
        batch_stop_dto(Err(RuntimeError::StepLimitExceeded { limit: 3, pc: 3 })).expect("mapped");
    assert_eq!(
        stop,
        BatchStopDto::Error {
            error: RuntimeErrorDto::StepLimitExceeded { limit: 3, pc: 3 },
        }
    );
    assert_ne!(stop, BatchStopDto::BatchReached);
}

#[test]
fn step_outcomes_map_to_executed_halted_and_error() {
    assert_eq!(
        step_outcome_dto(Ok(StepResult::Executed)),
        Ok(StepOutcomeDto::Executed)
    );
    assert_eq!(
        step_outcome_dto(Ok(StepResult::Halted)),
        Ok(StepOutcomeDto::Halted)
    );
    assert_eq!(
        step_outcome_dto(Err(RuntimeError::PointerOutOfBounds {
            pc: 0,
            dp: 0,
            direction: PointerDirection::Left,
        })),
        Ok(StepOutcomeDto::Error {
            error: RuntimeErrorDto::PointerOutOfBounds {
                pc: 0,
                dp: 0,
                direction: PointerDirectionDto::Left,
            },
        })
    );
}

#[test]
fn executed_count_is_copied_as_usize() {
    assert_eq!(executed_dto(0), Ok(0));
    assert_eq!(executed_dto(1_000_000), Ok(1_000_000));
}

// ---- スナップショット・テープ窓 ----

#[test]
fn snapshot_copies_every_field() {
    // `++` の 1 命令目を実行した直後: pc 1、dp 0、現在セル 1、実行数 1、未停止(pc が命令数に達していない)。
    let program = Program::parse("++").expect("valid source");
    let mut machine = Machine::new(program, Vec::new(), MachineConfig::unlimited());
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    let snapshot: MachineSnapshot = machine.snapshot();
    assert_eq!(
        snapshot_dto(&snapshot),
        SnapshotDto {
            pc: 1,
            dp: 0,
            current_cell: 1,
            steps_executed: 1,
            halted: false,
        }
    );
}

#[test]
fn tape_window_at_the_left_edge_is_copied() {
    let tape: Vec<u8> = (0..TAPE_SIZE).map(|i| (i % 256) as u8).collect();
    let window = tape_window_of(&tape, 0, 2);
    assert_eq!(
        tape_window_dto(&window),
        TapeWindowDto {
            start: 0,
            cells: vec![0, 1, 2],
            pointer_offset: 0,
        }
    );
}

#[test]
fn tape_window_at_the_right_edge_is_copied() {
    let tape: Vec<u8> = (0..TAPE_SIZE).map(|i| (i % 256) as u8).collect();
    let dp = TAPE_SIZE - 1;
    let window = tape_window_of(&tape, dp, 2);
    let dto = tape_window_dto(&window);
    assert_eq!(dto.start, TAPE_SIZE - 3);
    assert_eq!(dto.cells.len(), 3);
    assert_eq!(dto.pointer_offset, 2);
    assert_eq!(dto.cells[dto.pointer_offset], tape[dp]);
}

// ---- 出力差分(末尾 65,536 バイトの規則) ----

#[test]
fn output_chunk_from_the_start_returns_everything() {
    let chunk = output_chunk(b"abc", 0);
    assert_eq!(chunk.data_start, 0);
    assert_eq!(chunk.total_len, 3);
    assert_eq!(chunk.data_base64, "YWJj");
}

#[test]
fn output_chunk_continuous_returns_only_the_new_tail() {
    let chunk = output_chunk(b"abc", 1);
    assert_eq!(chunk.data_start, 1);
    assert_eq!(chunk.total_len, 3);
    assert_eq!(decoded(&chunk.data_base64), b"bc");
}

#[test]
fn output_chunk_at_the_end_is_empty() {
    let chunk = output_chunk(b"abc", 3);
    assert_eq!(chunk.data_start, 3);
    assert_eq!(chunk.total_len, 3);
    assert_eq!(chunk.data_base64, "");
}

#[test]
fn output_chunk_beyond_the_total_returns_everything_from_zero() {
    // 要求位置が総バイト数より大きい(出力が縮んだ後など)ときは、末尾の窓を先頭から返す。
    let chunk = output_chunk(b"abc", 10);
    assert_eq!(chunk.data_start, 0);
    assert_eq!(chunk.total_len, 3);
    assert_eq!(decoded(&chunk.data_base64), b"abc");
}

#[test]
fn output_chunk_clamps_to_the_tail_limit_when_output_is_large() {
    let output: Vec<u8> = (0..OUTPUT_TAIL + 4_000).map(|i| (i % 251) as u8).collect();
    let total = output.len();
    let floor = total - OUTPUT_TAIL;

    // 要求位置 0 は末尾 65,536 バイトへ切り詰める。
    let chunk = output_chunk(&output, 0);
    assert_eq!(chunk.data_start, floor);
    assert_eq!(chunk.total_len, total);
    assert_eq!(decoded(&chunk.data_base64), output[floor..].to_vec());
    assert_eq!(decoded(&chunk.data_base64).len(), OUTPUT_TAIL);

    // 要求位置が切り詰め境界より後なら、その位置から返す。
    let chunk = output_chunk(&output, floor + 10);
    assert_eq!(chunk.data_start, floor + 10);
    assert_eq!(decoded(&chunk.data_base64), output[floor + 10..].to_vec());

    // 要求位置が切り詰め境界より前なら、境界から返す。
    let chunk = output_chunk(&output, floor - 1);
    assert_eq!(chunk.data_start, floor);
}

#[test]
fn output_chunk_at_exactly_the_tail_limit_keeps_all_bytes() {
    let output: Vec<u8> = vec![7; OUTPUT_TAIL];
    let chunk = output_chunk(&output, 0);
    assert_eq!(chunk.data_start, 0);
    assert_eq!(decoded(&chunk.data_base64).len(), OUTPUT_TAIL);
}

#[test]
fn output_chunk_preserves_non_utf8_bytes() {
    let output = [0xff_u8, 0x00, 0x80, b'A'];
    let chunk = output_chunk(&output, 0);
    assert_eq!(decoded(&chunk.data_base64), output.to_vec());
}

// ---- 状態応答 ----

#[test]
fn session_state_copies_view_and_counters() {
    let source = "+.";
    let program = Program::parse(source).expect("valid source");
    let source_map = SourceMap::new(&program, source).expect("matching source");
    let mut session = DebugSession::new(program, Vec::new(), MachineConfig::with_max_steps(1_000));
    assert_eq!(
        session.run_batch(BatchSize::DEFAULT, &NeverCancel).stop,
        Ok(StopReason::Halted)
    );
    let view: StateView<'_> = session.state_view(2);
    let current = source_map.range_of(0);

    let dto = session_state_dto(7, &view, None, 3, 9, 0).expect("state");

    assert_eq!(dto.session_id, 7);
    assert!(dto.snapshot.halted);
    assert_eq!(dto.snapshot.steps_executed, 2);
    // dp = 0 の半径 2 の窓は先頭セル 0 から 3 セル(テープ左端で切り詰め)。
    assert_eq!(dto.tape_window.start, 0);
    assert_eq!(dto.tape_window.cells, vec![1, 0, 0]);
    assert_eq!(dto.tape_window.pointer_offset, 0);
    assert_eq!(dto.output.total_len, 1);
    assert_eq!(decoded(&dto.output.data_base64), vec![1]);
    assert_eq!(dto.input_consumed, 3);
    assert_eq!(dto.input_total, 9);
    assert_eq!(dto.last_stop, Some(StopReasonDto::Halted));
    assert_eq!(dto.current_range, None);

    // 現在命令の範囲は渡された値をそのまま写す。
    let with_range = session_state_dto(7, &view, current.as_ref(), 0, 0, 0).expect("state");
    assert_eq!(
        with_range.current_range,
        current.as_ref().map(source_range_dto)
    );
}

#[test]
fn runtime_error_from_a_real_session_maps_to_error_stop() {
    // 先頭で `<` を実行すると範囲外へ移動する。状態は変わらず、エラーとして報告される。
    let program = Program::parse("<").expect("valid source");
    let mut session = DebugSession::new(program, Vec::new(), MachineConfig::with_max_steps(1_000));
    let report = session.run_batch(BatchSize::DEFAULT, &NeverCancel);
    assert_eq!(
        batch_stop_dto(report.stop),
        Ok(BatchStopDto::Error {
            error: RuntimeErrorDto::PointerOutOfBounds {
                pc: 0,
                dp: 0,
                direction: PointerDirectionDto::Left,
            },
        })
    );
    let view = session.state_view(2);
    let dto = session_state_dto(1, &view, None, 0, 0, 0).expect("state");
    assert_eq!(dto.snapshot.pc, 0);
    assert_eq!(dto.snapshot.dp, 0);
    assert!(!dto.snapshot.halted);
}

#[test]
fn batch_result_carries_stop_count_and_state() {
    let program = Program::parse("+").expect("valid source");
    let mut session = DebugSession::new(program, Vec::new(), MachineConfig::with_max_steps(1_000));
    let report = session.run_batch(BatchSize::DEFAULT, &NeverCancel);
    let state = session_state_dto(4, &session.state_view(1), None, 0, 0, 0).expect("state");
    let result = batch_result_dto(report.executed, report.stop, state).expect("batch result");
    assert_eq!(result.stop, BatchStopDto::Halted);
    assert_eq!(result.executed, 1);
    assert_eq!(result.state.session_id, 4);
    assert!(result.state.snapshot.halted);
    assert_eq!(result.state.snapshot.steps_executed, 1);
}

#[test]
fn batch_limit_and_step_limit_are_distinct_labels_in_a_real_session() {
    let source = "+++++";

    // バッチ幅 2 は batchReached(通常の停止)。
    let program = Program::parse(source).expect("valid source");
    let mut session = DebugSession::new(program, Vec::new(), MachineConfig::with_max_steps(3));
    let batch = BatchSize::new(2).expect("non-zero");
    let report = session.run_batch(batch, &NeverCancel);
    assert_eq!(batch_stop_dto(report.stop), Ok(BatchStopDto::BatchReached));

    // 機械の命令数上限 3 に達すると、実行エラー(stepLimitExceeded)として報告される。
    let report = session.run_batch(BatchSize::DEFAULT, &NeverCancel);
    assert_eq!(
        batch_stop_dto(report.stop),
        Ok(BatchStopDto::Error {
            error: RuntimeErrorDto::StepLimitExceeded { limit: 3, pc: 3 },
        })
    );
}
