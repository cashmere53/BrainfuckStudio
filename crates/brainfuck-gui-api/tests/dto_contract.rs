//! 共通 DTO の JSON 形(camelCase、`kind` 判別、`u64` の number 表現)と往復の契約。

use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, OutputChunkDto, ParseErrorDto, ParseErrorKind, PointerDirectionDto,
    RuntimeErrorDto, SourcePositionDto, SourceRangeDto,
};
use serde_json::{json, Value};

fn position(byte: usize, utf16: usize, line: usize, column: usize) -> SourcePositionDto {
    SourcePositionDto {
        byte_offset: byte,
        utf16_offset: utf16,
        line,
        column,
    }
}

fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("object expected")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn round_trip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let text = serde_json::to_string(value).expect("serialize");
    serde_json::from_str(&text).expect("deserialize")
}

#[test]
fn source_position_uses_camel_case_fields() {
    let p = position(5, 4, 2, 3);
    let json = serde_json::to_value(p).unwrap();
    assert_eq!(
        json,
        json!({ "byteOffset": 5, "utf16Offset": 4, "line": 2, "column": 3 })
    );
    assert_eq!(
        keys(&json),
        vec!["byteOffset", "column", "line", "utf16Offset"]
    );
    assert_eq!(round_trip(&p), p);
}

#[test]
fn source_range_is_start_and_end_positions() {
    let r = SourceRangeDto {
        start: position(0, 0, 1, 1),
        end: position(1, 1, 1, 2),
    };
    let json = serde_json::to_value(r).unwrap();
    assert_eq!(
        json,
        json!({
            "start": { "byteOffset": 0, "utf16Offset": 0, "line": 1, "column": 1 },
            "end": { "byteOffset": 1, "utf16Offset": 1, "line": 1, "column": 2 }
        })
    );
    assert_eq!(round_trip(&r), r);
}

#[test]
fn parse_error_has_kind_instruction_index_and_position() {
    let e = ParseErrorDto {
        kind: ParseErrorKind::UnmatchedClosingBracket,
        instruction_index: 3,
        position: position(7, 7, 1, 8),
    };
    let json = serde_json::to_value(e).unwrap();
    assert_eq!(
        json,
        json!({
            "kind": "unmatchedClosingBracket",
            "instructionIndex": 3,
            "position": { "byteOffset": 7, "utf16Offset": 7, "line": 1, "column": 8 }
        })
    );
    assert_eq!(round_trip(&e), e);

    let opening = ParseErrorKind::UnmatchedOpeningBracket;
    assert_eq!(
        serde_json::to_value(opening).unwrap(),
        json!("unmatchedOpeningBracket")
    );
}

#[test]
fn runtime_error_pointer_out_of_bounds_is_tagged_by_kind() {
    let e = RuntimeErrorDto::PointerOutOfBounds {
        pc: 2,
        dp: 0,
        direction: PointerDirectionDto::Left,
    };
    let json = serde_json::to_value(e).unwrap();
    assert_eq!(
        json,
        json!({ "kind": "pointerOutOfBounds", "pc": 2, "dp": 0, "direction": "left" })
    );
    assert_eq!(round_trip(&e), e);

    let right = RuntimeErrorDto::PointerOutOfBounds {
        pc: 0,
        dp: 29999,
        direction: PointerDirectionDto::Right,
    };
    assert_eq!(
        serde_json::to_value(right).unwrap()["direction"],
        json!("right")
    );
}

#[test]
fn runtime_error_step_limit_keeps_u64_as_json_number_at_the_maximum() {
    let max = (1u64 << 53) - 1;
    let e = RuntimeErrorDto::StepLimitExceeded { limit: max, pc: 9 };
    let text = serde_json::to_string(&e).unwrap();
    assert_eq!(
        text,
        r#"{"kind":"stepLimitExceeded","limit":9007199254740991,"pc":9}"#
    );
    assert_eq!(round_trip(&e), e);
}

#[test]
fn output_chunk_uses_camel_case_fields() {
    let c = OutputChunkDto {
        data_start: 2,
        total_len: 5,
        data_base64: "AAE=".to_owned(),
    };
    let json = serde_json::to_value(&c).unwrap();
    assert_eq!(
        json,
        json!({ "dataStart": 2, "totalLen": 5, "dataBase64": "AAE=" })
    );
    assert_eq!(round_trip(&c), c);
}

#[test]
fn api_error_kinds_are_the_fixed_camel_case_set() {
    let kinds = [
        (ApiErrorKind::InvalidArgument, "invalidArgument"),
        (ApiErrorKind::SessionNotFound, "sessionNotFound"),
        (ApiErrorKind::ProgramNotLoaded, "programNotLoaded"),
        (ApiErrorKind::StaleProgram, "staleProgram"),
        (ApiErrorKind::SourceTooLarge, "sourceTooLarge"),
        (ApiErrorKind::InputTooLarge, "inputTooLarge"),
        (ApiErrorKind::FileTooLarge, "fileTooLarge"),
        (ApiErrorKind::Io, "io"),
        (ApiErrorKind::Internal, "internal"),
    ];
    for (kind, text) in kinds {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(text));
        assert_eq!(round_trip(&kind), kind);
    }
}

#[test]
fn api_error_has_kind_message_and_nullable_field() {
    let without_field = ApiError {
        kind: ApiErrorKind::Internal,
        message: "internal error".to_owned(),
        field: None,
    };
    assert_eq!(
        serde_json::to_value(&without_field).unwrap(),
        json!({ "kind": "internal", "message": "internal error", "field": null })
    );
    assert_eq!(round_trip(&without_field), without_field);

    let with_field = ApiError {
        kind: ApiErrorKind::InvalidArgument,
        message: "invalid argument".to_owned(),
        field: Some("batchSize".to_owned()),
    };
    assert_eq!(
        serde_json::to_value(&with_field).unwrap()["field"],
        json!("batchSize")
    );
    assert_eq!(round_trip(&with_field), with_field);
}
