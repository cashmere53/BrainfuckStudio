//! 再利用する公開項目がクレート外から使えることの最小のコンパイル確認(タスク 4.1)。
//! 欠けている項目があればここがコンパイルエラーになる(デバッガ API クレートは編集しない)。

use brainfuck_compare::limits::{DEFAULT_REPETITIONS, MAX_COMPARISON_STEPS, MAX_REPETITIONS};
use brainfuck_compare::{
    CompareError, Comparison, ComparisonRequest, Method, MethodReport, Observation, Overall, Units,
    Verdict,
};
use brainfuck_core::{tape_window_of, Program, DEFAULT_WINDOW_RADIUS};
use brainfuck_gui_api::codec;
use brainfuck_gui_api::convert::{
    output_chunk, parse_error_dto, runtime_error_dto, tape_window_dto,
};
use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, OutputChunkDto, ParseErrorDto, RuntimeErrorDto, TapeWindowDto,
};
use brainfuck_gui_api::limits::{MAX_INPUT_BYTES, MAX_SOURCE_BYTES, OUTPUT_TAIL_LIMIT};
use brainfuck_gui_api::service::DebuggerService;

#[test]
fn shared_items_are_reachable_from_outside() {
    // 型名・関数名の存在を参照で確かめる(値は使わない)。
    let _ = std::mem::size_of::<ApiError>();
    let _ = ApiError::new(ApiErrorKind::InvalidArgument, Some("source"));
    let _ = std::mem::size_of::<ParseErrorDto>();
    let _ = std::mem::size_of::<RuntimeErrorDto>();
    let _ = std::mem::size_of::<OutputChunkDto>();
    let _ = std::mem::size_of::<TapeWindowDto>();
    let _: fn(&str, &brainfuck_core::ParseError) -> Result<ParseErrorDto, ApiError> =
        parse_error_dto;
    let _: fn(&brainfuck_core::RuntimeError) -> Result<RuntimeErrorDto, ApiError> =
        runtime_error_dto;
    let _: fn(&[u8], usize) -> OutputChunkDto = output_chunk;
    let _ = tape_window_dto;
    let _ = codec::encode(&[0u8, 1, 2]);
    let _ = codec::decode("AAEC", MAX_INPUT_BYTES);
    let _ = (MAX_SOURCE_BYTES, OUTPUT_TAIL_LIMIT, DEFAULT_WINDOW_RADIUS);
    let _ = DebuggerService::new();
    let _ = tape_window_of(&[0u8; 4], 0, DEFAULT_WINDOW_RADIUS);
    let _ = Program::parse("+");
    let _ = (MAX_COMPARISON_STEPS, MAX_REPETITIONS, DEFAULT_REPETITIONS);
    let _ = std::mem::size_of::<Observation>();
    let _ = std::mem::size_of::<Comparison>();
    let _ = std::mem::size_of::<ComparisonRequest<'static>>();
    let _ = std::mem::size_of::<MethodReport>();
    let _ = std::mem::size_of::<Method>();
    let _ = std::mem::size_of::<Overall>();
    let _ = std::mem::size_of::<Units>();
    let _ = std::mem::size_of::<Verdict>();
    let _ = std::mem::size_of::<CompareError>();
}
