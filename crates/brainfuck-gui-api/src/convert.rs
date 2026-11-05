//! 上流(`brainfuck-core` / `debugger-state`)の値から DTO への写像のみを行う。
//!
//! 位置の計算(行・列・UTF-16・逆引き)はここに置かず、上流の `SourceMap` の結果を写すだけにする。

use brainfuck_core::{
    MachineSnapshot, ParseError, PointerDirection, RuntimeError, SourceMap, SourcePosition,
    SourceRange, StateView, StepResult, StopReason, TapeWindow,
};

use crate::codec;
use crate::dto::{
    ApiError, ApiErrorKind, BatchResultDto, BatchStopDto, OutputChunkDto, ParseErrorDto,
    ParseErrorKind, PointerDirectionDto, RuntimeErrorDto, SessionStateDto, SnapshotDto,
    SourcePositionDto, SourceRangeDto, StepOutcomeDto, StopReasonDto, TapeWindowDto,
};
use crate::limits::OUTPUT_TAIL_LIMIT;

/// 上流の非網羅列挙(`#[non_exhaustive]`)に未知の値が現れたときの内部エラー。
fn internal() -> ApiError {
    ApiError::new(ApiErrorKind::Internal, None)
}

/// 上流の位置の 4 フィールドをそのまま DTO へ写す(値の再計算はしない)。
pub fn source_position_dto(p: &SourcePosition) -> SourcePositionDto {
    SourcePositionDto {
        byte_offset: p.byte_offset,
        utf16_offset: p.utf16_offset,
        line: p.line,
        column: p.column,
    }
}

/// 上流の範囲の始点・終点をそのまま DTO へ写す。
pub fn source_range_dto(r: &SourceRange) -> SourceRangeDto {
    SourceRangeDto {
        start: source_position_dto(&r.start),
        end: source_position_dto(&r.end),
    }
}

/// 解析エラーを DTO へ写す。位置は `SourceMap::position_at(source, 違反のバイトオフセット)` の結果を使う。
///
/// 位置が取得できない(通常は起こらない、ソースと解析結果が食い違う)場合と、上流の
/// 非網羅の列挙に未知の種類が現れた場合は `internal` を返す。
pub fn parse_error_dto(source: &str, e: &ParseError) -> Result<ParseErrorDto, ApiError> {
    let (kind, instruction_index, source_offset) = match *e {
        ParseError::UnmatchedClosingBracket {
            instruction_index,
            source_offset,
        } => (
            ParseErrorKind::UnmatchedClosingBracket,
            instruction_index,
            source_offset,
        ),
        ParseError::UnmatchedOpeningBracket {
            instruction_index,
            source_offset,
        } => (
            ParseErrorKind::UnmatchedOpeningBracket,
            instruction_index,
            source_offset,
        ),
        _ => return Err(ApiError::new(ApiErrorKind::Internal, None)),
    };
    let position = SourceMap::position_at(source, source_offset)
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, None))?;
    Ok(ParseErrorDto {
        kind,
        instruction_index,
        position: source_position_dto(&position),
    })
}

/// 機械の観測結果を DTO へ写す(値の再計算はしない)。
pub fn snapshot_dto(s: &MachineSnapshot) -> SnapshotDto {
    SnapshotDto {
        pc: s.pc,
        dp: s.data_pointer,
        current_cell: s.current_cell,
        steps_executed: s.steps_executed,
        halted: s.halted,
    }
}

/// テープ窓を DTO へ写す。窓の切り詰めは上流(`tape_window_of` / `DebugSession::tape_window`)の結果をそのまま使う。
pub fn tape_window_dto(w: &TapeWindow<'_>) -> TapeWindowDto {
    TapeWindowDto {
        start: w.start,
        cells: w.cells.to_vec(),
        pointer_offset: w.pointer_offset,
    }
}

/// 実行エラーを DTO へ写す。上流の非網羅列挙に未知の値があれば `internal`。
///
/// 戻り値が `Result` なのは、`RuntimeErrorDto` に内部エラーの表現がないため(未知の値を別の種類へ黙って写さない)。
pub fn runtime_error_dto(e: &RuntimeError) -> Result<RuntimeErrorDto, ApiError> {
    match *e {
        RuntimeError::PointerOutOfBounds { pc, dp, direction } => {
            Ok(RuntimeErrorDto::PointerOutOfBounds {
                pc,
                dp,
                direction: pointer_direction_dto(direction),
            })
        }
        RuntimeError::StepLimitExceeded { limit, pc } => {
            Ok(RuntimeErrorDto::StepLimitExceeded { limit, pc })
        }
        _ => Err(internal()),
    }
}

fn pointer_direction_dto(direction: PointerDirection) -> PointerDirectionDto {
    match direction {
        PointerDirection::Left => PointerDirectionDto::Left,
        PointerDirection::Right => PointerDirectionDto::Right,
    }
}

/// 停止理由を状態応答の `lastStop` 用の DTO へ写す。上流の `StepLimitReached` はバッチ到達(`batchReached`)とする。
pub fn stop_reason_dto(r: StopReason) -> Result<StopReasonDto, ApiError> {
    match r {
        StopReason::Halted => Ok(StopReasonDto::Halted),
        StopReason::Breakpoint => Ok(StopReasonDto::Breakpoint),
        StopReason::Cancelled => Ok(StopReasonDto::Cancelled),
        StopReason::StepLimitReached => Ok(StopReasonDto::BatchReached),
        _ => Err(internal()),
    }
}

/// 直近の停止理由(未停止は `None`)を DTO へ写す。
pub fn last_stop_dto(r: Option<StopReason>) -> Result<Option<StopReasonDto>, ApiError> {
    r.map(stop_reason_dto).transpose()
}

/// バッチ実行の停止理由を DTO へ写す。
///
/// 上流の `Ok(StepLimitReached)`(バッチ幅への到達)は `batchReached`、機械の命令数上限超過は
/// `Err(RuntimeError::StepLimitExceeded)` として `error` に載り、両者は別のラベルになる。
pub fn batch_stop_dto(r: Result<StopReason, RuntimeError>) -> Result<BatchStopDto, ApiError> {
    match r {
        Ok(StopReason::Halted) => Ok(BatchStopDto::Halted),
        Ok(StopReason::Breakpoint) => Ok(BatchStopDto::Breakpoint),
        Ok(StopReason::Cancelled) => Ok(BatchStopDto::Cancelled),
        Ok(StopReason::StepLimitReached) => Ok(BatchStopDto::BatchReached),
        Ok(_) => Err(internal()),
        Err(e) => Ok(BatchStopDto::Error {
            error: runtime_error_dto(&e)?,
        }),
    }
}

/// 1 命令の実行結果を DTO へ写す。
pub fn step_outcome_dto(r: Result<StepResult, RuntimeError>) -> Result<StepOutcomeDto, ApiError> {
    match r {
        Ok(StepResult::Executed) => Ok(StepOutcomeDto::Executed),
        Ok(StepResult::Halted) => Ok(StepOutcomeDto::Halted),
        Err(e) => Ok(StepOutcomeDto::Error {
            error: runtime_error_dto(&e)?,
        }),
    }
}

/// 上流の命令数(`u64`)を DTO の `usize` へ写す。通常の 64 ビット環境では失敗しない。
pub fn executed_dto(executed: u64) -> Result<usize, ApiError> {
    usize::try_from(executed).map_err(|_| internal())
}

/// 出力差分を DTO へ写す。開始位置の規則はこの関数の 1 か所にだけ置く。
///
/// - `output_from <= total` のとき `dataStart = max(output_from, total - 65,536)`(0 未満に下がらない)
/// - `output_from > total` のとき `dataStart = max(0, total - 65,536)`(出力が縮んだ後は末尾の窓を先頭から返す)
///
/// 返す `data_base64` は `dataStart` から末尾までのバイト列を標準 Base64 にしたもの(UTF-8 への変換はしない)。
pub fn output_chunk(output: &[u8], output_from: usize) -> OutputChunkDto {
    let total = output.len();
    let tail_start = total.saturating_sub(OUTPUT_TAIL_LIMIT);
    let requested = if output_from > total { 0 } else { output_from };
    let data_start = requested.max(tail_start);
    OutputChunkDto {
        data_start,
        total_len: total,
        data_base64: codec::encode(&output[data_start..]),
    }
}

/// 状態応答の共通部分を組み立てる。
///
/// スナップショット・窓・出力・直近の停止理由は上流の `StateView`(同一の借用から作られた値)を写す。
/// 現在命令の範囲(`current_range`)と入力の進捗は呼び出し元(サービス層)が求めて渡す。
pub fn session_state_dto(
    session_id: u64,
    view: &StateView<'_>,
    current_range: Option<&SourceRange>,
    input_consumed: usize,
    input_total: usize,
    output_from: usize,
) -> Result<SessionStateDto, ApiError> {
    Ok(SessionStateDto {
        session_id,
        snapshot: snapshot_dto(&view.snapshot),
        tape_window: tape_window_dto(&view.tape_window),
        output: output_chunk(view.output, output_from),
        input_consumed,
        input_total,
        last_stop: last_stop_dto(view.last_stop)?,
        current_range: current_range.map(source_range_dto),
    })
}

/// バッチ実行の結果を DTO へ写す(停止理由・実行数・停止時点の状態)。
pub fn batch_result_dto(
    executed: u64,
    stop: Result<StopReason, RuntimeError>,
    state: SessionStateDto,
) -> Result<BatchResultDto, ApiError> {
    Ok(BatchResultDto {
        stop: batch_stop_dto(stop)?,
        executed: executed_dto(executed)?,
        state,
    })
}
