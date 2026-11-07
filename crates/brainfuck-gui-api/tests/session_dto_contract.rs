//! セッション・実行・ブレークポイント・ファイル操作の DTO の JSON 形と、要求の受理/拒否の契約。
//!
//! 要求 DTO の方針(`dto` モジュールの先頭コメントと一致させる):
//! - 未知のキーは拒否する(`deny_unknown_fields`)。
//! - 必須項目(`Option` 以外)の欠落は拒否する。`Option` 項目は省略と `null` を同じ `None` として受理する。
//! - `u64` は JSON の非負整数として受理する(小数・負数・文字列は拒否)。上限 2^53−1 の検査は
//!   サービス層が `invalidArgument` で行う(DTO は形式のみを検査する)。

use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, BatchResultDto, BatchStopDto, BreakpointDto, BreakpointsDto,
    IoOperation, LoadSourceRequest, LoadSourceResult, OpenFileResult, OutputChunkDto,
    ParseErrorDto, ParseErrorKind, PauseRequest, PointerDirectionDto, RunBatchRequest,
    RuntimeErrorDto, SaveFileRequest, SaveFileResult, SessionStartedDto, SessionStateDto,
    SessionViewRequest, SetBreakpointsRequest, SetDocumentDirtyRequest, SnapshotDto,
    SourcePositionDto, SourceRangeDto, StartSessionRequest, StepLimitDto, StepOutcomeDto,
    StepResultDto, StopReasonDto, TapeWindowDto, TapeWindowRequest, ViewRequestDto,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

const MAX_STEP_LIMIT: u64 = 9_007_199_254_740_991;

fn position(byte: usize, utf16: usize, line: usize, column: usize) -> SourcePositionDto {
    SourcePositionDto {
        byte_offset: byte,
        utf16_offset: utf16,
        line,
        column,
    }
}

fn range() -> SourceRangeDto {
    SourceRangeDto {
        start: position(0, 0, 1, 1),
        end: position(1, 1, 1, 2),
    }
}

fn view() -> ViewRequestDto {
    ViewRequestDto {
        tape_radius: 16,
        output_from: 0,
    }
}

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serialize")
}

fn from_json<T: DeserializeOwned>(text: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(text)
}

fn round_trip<T>(value: &T) -> T
where
    T: Serialize + DeserializeOwned,
{
    let text = serde_json::to_string(value).expect("serialize");
    serde_json::from_str(&text).expect("deserialize")
}

fn sample_state(
    last_stop: Option<StopReasonDto>,
    current: Option<SourceRangeDto>,
) -> SessionStateDto {
    SessionStateDto {
        session_id: 7,
        snapshot: SnapshotDto {
            pc: 3,
            dp: 1,
            current_cell: 65,
            steps_executed: 12,
            halted: false,
        },
        tape_window: TapeWindowDto {
            start: 0,
            cells: vec![0, 65, 0],
            pointer_offset: 1,
        },
        output: OutputChunkDto {
            data_start: 0,
            total_len: 2,
            data_base64: "SGk=".to_owned(),
        },
        input_consumed: 1,
        input_total: 4,
        last_stop,
        current_range: current,
    }
}

// ---- 要求 DTO: 形式の受理・拒否 ----

#[test]
fn step_limit_is_tagged_and_max_steps_is_a_number() {
    let limited = StepLimitDto::Limited {
        max_steps: 10_000_000,
    };
    assert_eq!(
        to_value(&limited),
        json!({ "kind": "limited", "maxSteps": 10_000_000 })
    );
    assert_eq!(
        to_value(&StepLimitDto::Unlimited),
        json!({ "kind": "unlimited" })
    );
    assert_eq!(round_trip(&limited), limited);
    assert_eq!(
        round_trip(&StepLimitDto::Unlimited),
        StepLimitDto::Unlimited
    );
}

#[test]
fn step_limit_accepts_the_json_safe_maximum_as_a_plain_number() {
    let max = StepLimitDto::Limited {
        max_steps: MAX_STEP_LIMIT,
    };
    let text = serde_json::to_string(&max).expect("serialize");
    assert_eq!(text, r#"{"kind":"limited","maxSteps":9007199254740991}"#);
    assert_eq!(round_trip(&max), max);
}

#[test]
fn step_limit_accepts_values_above_the_json_safe_maximum_and_leaves_range_to_the_service() {
    // 形式は受理する(u64 の範囲内)。1..=2^53−1 の検査はサービス層が invalidArgument で行う。
    let parsed: StepLimitDto =
        from_json(r#"{"kind":"limited","maxSteps":9007199254740992}"#).expect("shape is valid");
    assert_eq!(
        parsed,
        StepLimitDto::Limited {
            max_steps: MAX_STEP_LIMIT + 1
        }
    );
}

#[test]
fn step_limit_rejects_negative_fractional_and_string_values() {
    for bad in [
        r#"{"kind":"limited","maxSteps":-1}"#,
        r#"{"kind":"limited","maxSteps":1.5}"#,
        r#"{"kind":"limited","maxSteps":"10"}"#,
        r#"{"kind":"limited"}"#,
    ] {
        assert!(from_json::<StepLimitDto>(bad).is_err(), "accepted: {bad}");
    }
}

#[test]
fn step_limit_rejects_unknown_keys_in_every_variant() {
    assert!(from_json::<StepLimitDto>(r#"{"kind":"unlimited","maxSteps":5}"#).is_err());
    assert!(from_json::<StepLimitDto>(r#"{"kind":"limited","maxSteps":5,"extra":1}"#).is_err());
    assert!(from_json::<StepLimitDto>(r#"{"kind":"bogus"}"#).is_err());
}

#[test]
fn requests_reject_unknown_top_level_and_nested_keys() {
    let top = r#"{"sessionId":1,"view":{"tapeRadius":16,"outputFrom":0},"extra":true}"#;
    assert!(from_json::<SessionViewRequest>(top).is_err());

    let nested = r#"{"sessionId":1,"view":{"tapeRadius":16,"outputFrom":0,"extra":true}}"#;
    assert!(from_json::<SessionViewRequest>(nested).is_err());
}

#[test]
fn requests_reject_missing_required_keys() {
    assert!(
        from_json::<SessionViewRequest>(r#"{"view":{"tapeRadius":16,"outputFrom":0}}"#).is_err()
    );
    assert!(from_json::<SessionViewRequest>(r#"{"sessionId":1}"#).is_err());
    assert!(from_json::<PauseRequest>(r#"{}"#).is_err());
    assert!(from_json::<LoadSourceRequest>(r#"{}"#).is_err());
    assert!(from_json::<StartSessionRequest>(
        r#"{"programId":1,"inputBase64":"","limit":{"kind":"unlimited"},"view":{"tapeRadius":0,"outputFrom":0}}"#
    )
    .is_err());
}

#[test]
fn optional_batch_size_accepts_absent_and_null_as_none() {
    let absent: RunBatchRequest =
        from_json(r#"{"sessionId":1,"view":{"tapeRadius":16,"outputFrom":0}}"#).expect("absent");
    assert_eq!(absent.batch_size, None);

    let null: RunBatchRequest =
        from_json(r#"{"sessionId":1,"batchSize":null,"view":{"tapeRadius":16,"outputFrom":0}}"#)
            .expect("null");
    assert_eq!(null.batch_size, None);

    let explicit: RunBatchRequest =
        from_json(r#"{"sessionId":1,"batchSize":1000,"view":{"tapeRadius":16,"outputFrom":0}}"#)
            .expect("number");
    assert_eq!(explicit.batch_size, Some(1000));
}

#[test]
fn batch_size_rejects_negative_and_fractional_values_but_leaves_range_to_the_service() {
    // 0 は形式として有効(1..=1,000,000 の検査はサービス層が invalidArgument で行う)。
    let zero: RunBatchRequest =
        from_json(r#"{"sessionId":1,"batchSize":0,"view":{"tapeRadius":16,"outputFrom":0}}"#)
            .expect("zero is a valid shape");
    assert_eq!(zero.batch_size, Some(0));

    for bad in [
        r#"{"sessionId":1,"batchSize":-1,"view":{"tapeRadius":16,"outputFrom":0}}"#,
        r#"{"sessionId":1,"batchSize":1.5,"view":{"tapeRadius":16,"outputFrom":0}}"#,
    ] {
        assert!(
            from_json::<RunBatchRequest>(bad).is_err(),
            "accepted: {bad}"
        );
    }
}

#[test]
fn start_session_request_uses_camel_case_and_nested_objects() {
    let text = r#"{
        "programId": 2,
        "inputBase64": "AQI=",
        "limit": { "kind": "limited", "maxSteps": 1000 },
        "breakpointOffsets": [0, 3],
        "view": { "tapeRadius": 8, "outputFrom": 4 }
    }"#;
    let parsed: StartSessionRequest = from_json(text).expect("valid request");
    assert_eq!(parsed.program_id, 2);
    assert_eq!(parsed.input_base64, "AQI=");
    assert_eq!(parsed.limit, StepLimitDto::Limited { max_steps: 1000 });
    assert_eq!(parsed.breakpoint_offsets, vec![0, 3]);
    assert_eq!(
        parsed.view,
        ViewRequestDto {
            tape_radius: 8,
            output_from: 4
        }
    );
    assert_eq!(round_trip(&parsed), parsed);
}

#[test]
fn request_dtos_round_trip_with_camel_case_keys() {
    let batch = RunBatchRequest {
        session_id: 9,
        batch_size: None,
        view: view(),
    };
    assert_eq!(
        to_value(&batch),
        json!({
            "sessionId": 9,
            "batchSize": null,
            "view": { "tapeRadius": 16, "outputFrom": 0 }
        })
    );
    assert_eq!(round_trip(&batch), batch);

    let tape = TapeWindowRequest {
        session_id: 9,
        radius: 64,
    };
    assert_eq!(to_value(&tape), json!({ "sessionId": 9, "radius": 64 }));

    let breakpoints = SetBreakpointsRequest {
        session_id: 9,
        offsets: vec![1, 2],
    };
    assert_eq!(
        to_value(&breakpoints),
        json!({ "sessionId": 9, "offsets": [1, 2] })
    );
    assert_eq!(round_trip(&breakpoints), breakpoints);

    let pause = PauseRequest { session_id: 9 };
    assert_eq!(to_value(&pause), json!({ "sessionId": 9 }));
}

// ---- 応答 DTO: JSON 形 ----

#[test]
fn load_source_result_is_tagged_ok_or_parse_error() {
    let ok = LoadSourceResult::Ok {
        program_id: 1,
        instruction_count: 4,
        line_count: 2,
    };
    assert_eq!(
        to_value(&ok),
        json!({ "kind": "ok", "programId": 1, "instructionCount": 4, "lineCount": 2 })
    );
    assert_eq!(round_trip(&ok), ok);

    let error = LoadSourceResult::ParseError {
        error: ParseErrorDto {
            kind: ParseErrorKind::UnmatchedOpeningBracket,
            instruction_index: 0,
            position: position(0, 0, 1, 1),
        },
    };
    let value = to_value(&error);
    assert_eq!(value["kind"], json!("parseError"));
    assert_eq!(value["error"]["kind"], json!("unmatchedOpeningBracket"));
    assert_eq!(round_trip(&error), error);
}

#[test]
fn stop_reason_uses_the_four_camel_case_strings() {
    let cases = [
        (StopReasonDto::Halted, "halted"),
        (StopReasonDto::Breakpoint, "breakpoint"),
        (StopReasonDto::Cancelled, "cancelled"),
        (StopReasonDto::BatchReached, "batchReached"),
    ];
    for (reason, text) in cases {
        assert_eq!(to_value(&reason), json!(text));
        assert_eq!(round_trip(&reason), reason);
    }
}

#[test]
fn session_state_has_all_fields_and_nullable_stop_and_range() {
    let state = sample_state(None, None);
    assert_eq!(
        to_value(&state),
        json!({
            "sessionId": 7,
            "snapshot": { "pc": 3, "dp": 1, "currentCell": 65, "stepsExecuted": 12, "halted": false },
            "tapeWindow": { "start": 0, "cells": [0, 65, 0], "pointerOffset": 1 },
            "output": { "dataStart": 0, "totalLen": 2, "dataBase64": "SGk=" },
            "inputConsumed": 1,
            "inputTotal": 4,
            "lastStop": null,
            "currentRange": null
        })
    );
    assert_eq!(round_trip(&state), state);

    let stopped = sample_state(Some(StopReasonDto::Breakpoint), Some(range()));
    let value = to_value(&stopped);
    assert_eq!(value["lastStop"], json!("breakpoint"));
    assert_eq!(value["currentRange"]["start"]["utf16Offset"], json!(0));
    assert_eq!(round_trip(&stopped), stopped);
}

#[test]
fn batch_stop_distinguishes_all_outcomes_and_nests_runtime_errors() {
    let cases = [
        (BatchStopDto::Halted, json!({ "kind": "halted" })),
        (BatchStopDto::Breakpoint, json!({ "kind": "breakpoint" })),
        (BatchStopDto::Cancelled, json!({ "kind": "cancelled" })),
        (
            BatchStopDto::BatchReached,
            json!({ "kind": "batchReached" }),
        ),
    ];
    for (stop, expected) in cases {
        assert_eq!(to_value(&stop), expected);
        assert_eq!(round_trip(&stop), stop);
    }

    let error = BatchStopDto::Error {
        error: RuntimeErrorDto::StepLimitExceeded {
            limit: MAX_STEP_LIMIT,
            pc: 5,
        },
    };
    assert_eq!(
        to_value(&error),
        json!({
            "kind": "error",
            "error": { "kind": "stepLimitExceeded", "limit": 9007199254740991u64, "pc": 5 }
        })
    );
    assert_eq!(round_trip(&error), error);

    let pointer = BatchStopDto::Error {
        error: RuntimeErrorDto::PointerOutOfBounds {
            pc: 1,
            dp: 0,
            direction: PointerDirectionDto::Left,
        },
    };
    assert_eq!(
        to_value(&pointer)["error"]["kind"],
        json!("pointerOutOfBounds")
    );
}

#[test]
fn batch_result_and_step_result_wrap_the_state() {
    let state = sample_state(Some(StopReasonDto::BatchReached), None);
    let batch = BatchResultDto {
        stop: BatchStopDto::BatchReached,
        executed: 1000,
        state: state.clone(),
    };
    let value = to_value(&batch);
    assert_eq!(value["stop"], json!({ "kind": "batchReached" }));
    assert_eq!(value["executed"], json!(1000));
    assert_eq!(value["state"]["lastStop"], json!("batchReached"));
    assert_eq!(round_trip(&batch), batch);

    let step = StepResultDto {
        outcome: StepOutcomeDto::Error {
            error: RuntimeErrorDto::PointerOutOfBounds {
                pc: 0,
                dp: 0,
                direction: PointerDirectionDto::Left,
            },
        },
        state,
    };
    assert_eq!(to_value(&step)["outcome"]["kind"], json!("error"));
    assert_eq!(round_trip(&step), step);

    for (outcome, text) in [
        (StepOutcomeDto::Executed, "executed"),
        (StepOutcomeDto::Halted, "halted"),
    ] {
        assert_eq!(to_value(&outcome), json!({ "kind": text }));
        assert_eq!(round_trip(&outcome), outcome);
    }
}

#[test]
fn breakpoints_list_resolved_and_unresolved_offsets() {
    let resolved = BreakpointDto {
        utf16_offset: 4,
        instruction_index: 2,
        range: range(),
    };
    let set = BreakpointsDto {
        breakpoints: vec![resolved],
        unresolved: vec![9],
    };
    assert_eq!(
        to_value(&set),
        json!({
            "breakpoints": [{
                "utf16Offset": 4,
                "instructionIndex": 2,
                "range": {
                    "start": { "byteOffset": 0, "utf16Offset": 0, "line": 1, "column": 1 },
                    "end": { "byteOffset": 1, "utf16Offset": 1, "line": 1, "column": 2 }
                }
            }],
            "unresolved": [9]
        })
    );
    assert_eq!(round_trip(&set), set);

    let started = SessionStartedDto {
        state: sample_state(None, None),
        breakpoints: set,
    };
    assert_eq!(to_value(&started)["breakpoints"]["unresolved"], json!([9]));
    assert_eq!(round_trip(&started), started);
}

#[test]
fn tape_window_dto_uses_start_cells_and_pointer_offset() {
    let window = TapeWindowDto {
        start: 3,
        cells: vec![1, 2],
        pointer_offset: 0,
    };
    assert_eq!(
        to_value(&window),
        json!({ "start": 3, "cells": [1, 2], "pointerOffset": 0 })
    );
    assert_eq!(round_trip(&window), window);
}

// ---- ファイル操作 DTO ----

#[test]
fn open_file_result_is_opened_or_cancelled() {
    let opened = OpenFileResult::Opened {
        source: "+.".to_owned(),
        file_name: "hello.bf".to_owned(),
        lossy: true,
    };
    assert_eq!(
        to_value(&opened),
        json!({ "kind": "opened", "source": "+.", "fileName": "hello.bf", "lossy": true })
    );
    assert_eq!(round_trip(&opened), opened);

    assert_eq!(
        to_value(&OpenFileResult::Cancelled),
        json!({ "kind": "cancelled" })
    );
}

#[test]
fn save_file_request_and_result_use_camel_case() {
    let request = SaveFileRequest {
        source: "+".to_owned(),
        save_as: true,
    };
    assert_eq!(to_value(&request), json!({ "source": "+", "saveAs": true }));
    assert_eq!(round_trip(&request), request);

    let saved = SaveFileResult::Saved {
        file_name: "out.bf".to_owned(),
    };
    assert_eq!(
        to_value(&saved),
        json!({ "kind": "saved", "fileName": "out.bf" })
    );
    assert_eq!(round_trip(&saved), saved);
    assert_eq!(
        to_value(&SaveFileResult::Cancelled),
        json!({ "kind": "cancelled" })
    );
}

#[test]
fn set_document_dirty_request_is_a_flag() {
    let request = SetDocumentDirtyRequest { dirty: true };
    assert_eq!(to_value(&request), json!({ "dirty": true }));
    assert_eq!(round_trip(&request), request);
    assert!(from_json::<SetDocumentDirtyRequest>(r#"{"dirty":"yes"}"#).is_err());
}

// ---- API エラーの固定文面 ----

#[test]
fn api_error_constructor_uses_the_fixed_message_for_each_kind() {
    let kinds = [
        ApiErrorKind::InvalidArgument,
        ApiErrorKind::SessionNotFound,
        ApiErrorKind::ProgramNotLoaded,
        ApiErrorKind::StaleProgram,
        ApiErrorKind::SourceTooLarge,
        ApiErrorKind::InputTooLarge,
        ApiErrorKind::FileTooLarge,
        ApiErrorKind::Io,
        ApiErrorKind::Internal,
    ];
    let mut messages = Vec::new();
    for kind in kinds {
        let error = ApiError::new(kind, None);
        assert!(!error.message.is_empty());
        assert!(error.message.is_ascii(), "message must be fixed ASCII text");
        // 同じ種類は常に同じ文面(入力や内容が混ざらない)。
        assert_eq!(ApiError::new(kind, Some("field")).message, error.message);
        messages.push(error.message);
    }
    messages.sort();
    messages.dedup();
    assert_eq!(messages.len(), 9, "each kind has its own fixed message");
}

#[test]
fn api_error_constructor_carries_the_field_name_only() {
    let error = ApiError::new(ApiErrorKind::InvalidArgument, Some("batchSize"));
    assert_eq!(
        to_value(&error),
        json!({
            "kind": "invalidArgument",
            "message": ApiError::new(ApiErrorKind::InvalidArgument, None).message,
            "field": "batchSize"
        })
    );
    assert_eq!(error.field.as_deref(), Some("batchSize"));
}

/// 有効な例は受理され、余分なキーを 1 つ足すと拒否され、必須項目を 1 つずつ欠くと拒否されることを検査する。
/// 失敗時のメッセージに型名と項目名を含める(どの型で破れたかが分かるように)。
fn assert_strict_request<T: DeserializeOwned>(name: &str, valid: &str, required: &[&str]) {
    let valid_value: Value = serde_json::from_str(valid).expect("valid sample must be JSON");
    assert!(
        from_json::<T>(valid).is_ok(),
        "{name}: valid sample was rejected: {valid}"
    );

    let mut with_extra = valid_value.clone();
    with_extra
        .as_object_mut()
        .expect("object sample")
        .insert("unexpectedKey".to_owned(), json!(true));
    assert!(
        from_json::<T>(&with_extra.to_string()).is_err(),
        "{name}: an unknown key was accepted"
    );

    for key in required {
        let mut without = valid_value.clone();
        without.as_object_mut().expect("object sample").remove(*key);
        assert!(
            from_json::<T>(&without.to_string()).is_err(),
            "{name}: a request without required `{key}` was accepted"
        );
    }
}

#[test]
fn request_dtos_reject_unknown_keys_and_missing_required_keys() {
    assert_strict_request::<RunBatchRequest>(
        "RunBatchRequest",
        r#"{"sessionId":1,"view":{"tapeRadius":16,"outputFrom":0}}"#,
        &["sessionId", "view"],
    );
    assert_strict_request::<StartSessionRequest>(
        "StartSessionRequest",
        r#"{"programId":1,"inputBase64":"","limit":{"kind":"unlimited"},"breakpointOffsets":[],"view":{"tapeRadius":16,"outputFrom":0}}"#,
        &[
            "programId",
            "inputBase64",
            "limit",
            "breakpointOffsets",
            "view",
        ],
    );
    assert_strict_request::<TapeWindowRequest>(
        "TapeWindowRequest",
        r#"{"sessionId":1,"radius":16}"#,
        &["sessionId", "radius"],
    );
    assert_strict_request::<SetBreakpointsRequest>(
        "SetBreakpointsRequest",
        r#"{"sessionId":1,"offsets":[]}"#,
        &["sessionId", "offsets"],
    );
    assert_strict_request::<PauseRequest>("PauseRequest", r#"{"sessionId":1}"#, &["sessionId"]);
    assert_strict_request::<LoadSourceRequest>(
        "LoadSourceRequest",
        r#"{"source":""}"#,
        &["source"],
    );
    assert_strict_request::<SaveFileRequest>(
        "SaveFileRequest",
        r#"{"source":"","saveAs":false}"#,
        &["source", "saveAs"],
    );
    assert_strict_request::<SetDocumentDirtyRequest>(
        "SetDocumentDirtyRequest",
        r#"{"dirty":true}"#,
        &["dirty"],
    );
}

// ---- io エラーの文面(要求 2.11、design の Error Handling) ----

#[test]
fn io_error_message_names_the_operation_and_carries_the_os_text() {
    let os_text = "os says no";
    let read = ApiError::io(
        IoOperation::Read,
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, os_text),
    );
    assert_eq!(read.kind, ApiErrorKind::Io);
    assert_eq!(read.message, "failed to read file: os says no");
    assert_eq!(read.field, None);

    let write = ApiError::io(IoOperation::Write, std::io::Error::other("disk full"));
    assert_eq!(write.message, "failed to write file: disk full");
    assert_eq!(
        to_value(&write),
        json!({ "kind": "io", "message": "failed to write file: disk full", "field": null })
    );
}

#[test]
fn io_error_message_from_a_real_failure_contains_no_path_or_contents() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dir_text = dir.path().to_string_lossy().into_owned();

    let read_err = std::fs::read(dir.path().join("secret-missing-name.bf"))
        .expect_err("the file does not exist");
    let read = ApiError::io(IoOperation::Read, &read_err);
    assert!(
        read.message.starts_with("failed to read file: "),
        "{}",
        read.message
    );
    assert!(
        read.message.ends_with(&read_err.to_string()),
        "{}",
        read.message
    );
    assert!(
        !read.message.contains("secret-missing-name"),
        "{}",
        read.message
    );
    assert!(!read.message.contains(&dir_text), "{}", read.message);

    let write_err = std::fs::write(
        dir.path().join("no-such-dir").join("secret-output.bf"),
        b"secret source",
    )
    .expect_err("the parent directory does not exist");
    let write = ApiError::io(IoOperation::Write, &write_err);
    assert!(
        write.message.starts_with("failed to write file: "),
        "{}",
        write.message
    );
    assert!(
        write.message.ends_with(&write_err.to_string()),
        "{}",
        write.message
    );
    assert!(
        !write.message.contains("secret-output"),
        "{}",
        write.message
    );
    assert!(
        !write.message.contains("secret source"),
        "{}",
        write.message
    );
    assert!(!write.message.contains(&dir_text), "{}", write.message);
}

// ---- 出力チャンクの共有型(サービス応答と同じ形) ----

#[test]
fn output_chunk_inside_state_keeps_the_base64_payload() {
    let chunk = OutputChunkDto {
        data_start: 0,
        total_len: 1,
        data_base64: "AA==".to_owned(),
    };
    assert_eq!(
        to_value(&chunk),
        json!({ "dataStart": 0, "totalLen": 1, "dataBase64": "AA==" })
    );
}

// ---- 要求 DTO の検査方針: LoadSourceRequest はソースを文字列で運ぶ ----

#[test]
fn load_source_request_carries_the_source_text() {
    let request: LoadSourceRequest = from_json(r#"{"source":"+[]"}"#).expect("valid");
    assert_eq!(request.source, "+[]");
    assert_eq!(to_value(&request), json!({ "source": "+[]" }));
}
