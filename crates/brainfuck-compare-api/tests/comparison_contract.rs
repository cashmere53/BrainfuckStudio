//! 比較用 DTO の JSON 形(camelCase、`kind` 判別、`u64` の number 表現)・判別子の値・往復の契約(タスク 4.2)。
//! 共通部品(解析エラー・実行エラー・出力チャンク・テープ窓)の形は gui-api 側の契約テストが持つ。

use brainfuck_compare_api::dto::{
    ComparisonInfoDto, ComparisonMethodDto, ComparisonResultDto, ExitStateDto, FailureKindDto,
    MethodResultDto, MetricsDto, MismatchFieldDto, OverallDto, RunComparisonRequest,
    TerminationDto, UnitDto, UnsupportedReasonDto, VerdictDto,
};
use brainfuck_gui_api::dto::{
    OutputChunkDto, ParseErrorDto, ParseErrorKind, PointerDirectionDto, RuntimeErrorDto,
    SourcePositionDto, TapeWindowDto,
};
use serde_json::{json, Value};

fn round_trip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let text = serde_json::to_string(value).expect("serialize");
    serde_json::from_str(&text).expect("deserialize")
}

fn exit_state(termination: TerminationDto) -> ExitStateDto {
    ExitStateDto {
        termination,
        pc: 3,
        dp: 1,
        current_cell: 255,
        steps_executed: 42,
        input_consumed: 1,
        input_total: 2,
        output_tail: OutputChunkDto {
            data_start: 0,
            total_len: 2,
            data_base64: "AAE=".to_string(),
        },
        tape_window: TapeWindowDto {
            start: 0,
            cells: vec![0, 255],
            pointer_offset: 1,
        },
        tape_digest: "0123456789abcdef".to_string(),
    }
}

fn metrics(prepare: Option<u64>, unit: UnitDto) -> MetricsDto {
    MetricsDto {
        prepare_micros: prepare,
        run_micros: 17,
        repetitions: 3,
        unit,
        artifact_bytes: None,
        replayed_steps: None,
    }
}

#[test]
fn info_has_camel_case_fields_and_method_names() {
    let info = ComparisonInfoDto {
        max_steps: 50_000_000,
        max_repetitions: 5,
        default_repetitions: 3,
        methods: vec![
            ComparisonMethodDto::Naive,
            ComparisonMethodDto::Optimized,
            ComparisonMethodDto::Wasm,
        ],
    };
    assert_eq!(
        serde_json::to_value(&info).unwrap(),
        json!({
            "maxSteps": 50_000_000,
            "maxRepetitions": 5,
            "defaultRepetitions": 3,
            "methods": ["naive", "optimized", "wasm"],
        })
    );
    assert_eq!(round_trip(&info), info);
}

#[test]
fn request_parses_camel_case_and_rejects_unknown_keys() {
    let ok: RunComparisonRequest = serde_json::from_value(json!({
        "source": "+",
        "inputBase64": "AA==",
        "maxSteps": 1000,
        "methods": ["optimized", "wasm"],
        "repetitions": 3,
    }))
    .expect("valid request");
    assert_eq!(
        ok,
        RunComparisonRequest {
            source: "+".to_string(),
            input_base64: "AA==".to_string(),
            max_steps: 1000,
            methods: vec![ComparisonMethodDto::Optimized, ComparisonMethodDto::Wasm],
            repetitions: 3,
        }
    );

    let extra = serde_json::from_value::<RunComparisonRequest>(json!({
        "source": "+",
        "inputBase64": "",
        "maxSteps": 1,
        "methods": [],
        "repetitions": 1,
        "unexpected": true,
    }));
    assert!(extra.is_err(), "unknown keys must be rejected");

    let bad_method = serde_json::from_value::<RunComparisonRequest>(json!({
        "source": "+",
        "inputBase64": "",
        "maxSteps": 1,
        "methods": ["Wasm"],
        "repetitions": 1,
    }));
    assert!(bad_method.is_err(), "method names are camelCase literals");
}

#[test]
fn request_serializes_with_camel_case_keys() {
    let req = RunComparisonRequest {
        source: String::new(),
        input_base64: String::new(),
        max_steps: 7,
        methods: vec![],
        repetitions: 2,
    };
    let value = serde_json::to_value(&req).unwrap();
    let mut keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "inputBase64",
            "maxSteps",
            "methods",
            "repetitions",
            "source"
        ]
    );
    assert_eq!(round_trip(&req), req);
}

#[test]
fn method_names_are_fixed_literals() {
    for (method, name) in [
        (ComparisonMethodDto::Naive, "naive"),
        (ComparisonMethodDto::Optimized, "optimized"),
        (ComparisonMethodDto::Wasm, "wasm"),
    ] {
        assert_eq!(serde_json::to_value(method).unwrap(), json!(name));
        assert_eq!(round_trip(&method), method);
    }
}

#[test]
fn termination_discriminators() {
    assert_eq!(
        serde_json::to_value(TerminationDto::Halted).unwrap(),
        json!({"kind": "halted"})
    );
    let err = TerminationDto::Error {
        error: RuntimeErrorDto::PointerOutOfBounds {
            pc: 4,
            dp: 0,
            direction: PointerDirectionDto::Left,
        },
    };
    assert_eq!(
        serde_json::to_value(err).unwrap(),
        json!({
            "kind": "error",
            "error": {"kind": "pointerOutOfBounds", "pc": 4, "dp": 0, "direction": "left"},
        })
    );
    assert_eq!(round_trip(&err), err);
}

#[test]
fn exit_state_has_full_camel_case_shape() {
    let exit = exit_state(TerminationDto::Halted);
    assert_eq!(
        serde_json::to_value(&exit).unwrap(),
        json!({
            "termination": {"kind": "halted"},
            "pc": 3,
            "dp": 1,
            "currentCell": 255,
            "stepsExecuted": 42,
            "inputConsumed": 1,
            "inputTotal": 2,
            "outputTail": {"dataStart": 0, "totalLen": 2, "dataBase64": "AAE="},
            "tapeWindow": {"start": 0, "cells": [0, 255], "pointerOffset": 1},
            "tapeDigest": "0123456789abcdef",
        })
    );
    assert_eq!(round_trip(&exit), exit);
}

#[test]
fn unit_discriminators() {
    assert_eq!(
        serde_json::to_value(UnitDto::NaiveInstructions { count: 5 }).unwrap(),
        json!({"kind": "naiveInstructions", "count": 5})
    );
    assert_eq!(
        serde_json::to_value(UnitDto::IrOperations { count: 9 }).unwrap(),
        json!({"kind": "irOperations", "count": 9})
    );
    assert_eq!(
        serde_json::to_value(UnitDto::None).unwrap(),
        json!({"kind": "none"})
    );
    for unit in [
        UnitDto::NaiveInstructions { count: 5 },
        UnitDto::IrOperations { count: 9 },
        UnitDto::None,
    ] {
        assert_eq!(round_trip(&unit), unit);
    }
}

#[test]
fn metrics_use_null_for_absent_values() {
    let naive = metrics(None, UnitDto::NaiveInstructions { count: 2 });
    assert_eq!(
        serde_json::to_value(&naive).unwrap(),
        json!({
            "prepareMicros": null,
            "runMicros": 17,
            "repetitions": 3,
            "unit": {"kind": "naiveInstructions", "count": 2},
            "artifactBytes": null,
            "replayedSteps": null,
        })
    );
    let wasm = MetricsDto {
        prepare_micros: Some(8),
        run_micros: 17,
        repetitions: 3,
        unit: UnitDto::None,
        artifact_bytes: Some(120),
        replayed_steps: Some(0),
    };
    let value = serde_json::to_value(&wasm).unwrap();
    assert_eq!(value["prepareMicros"], json!(8));
    assert_eq!(value["unit"], json!({"kind": "none"}));
    assert_eq!(value["artifactBytes"], json!(120));
    assert_eq!(value["replayedSteps"], json!(0));
    assert_eq!(round_trip(&wasm), wasm);
}

#[test]
fn verdict_and_mismatch_field_names() {
    assert_eq!(
        serde_json::to_value(VerdictDto::Oracle).unwrap(),
        json!({"kind": "oracle"})
    );
    assert_eq!(
        serde_json::to_value(VerdictDto::Match).unwrap(),
        json!({"kind": "match"})
    );
    let mismatch = VerdictDto::Mismatch {
        fields: vec![
            MismatchFieldDto::Termination,
            MismatchFieldDto::ErrorValue,
            MismatchFieldDto::Pc,
            MismatchFieldDto::DataPointer,
            MismatchFieldDto::CurrentCell,
            MismatchFieldDto::StepsExecuted,
            MismatchFieldDto::InputPosition,
            MismatchFieldDto::Output,
            MismatchFieldDto::Tape,
        ],
    };
    assert_eq!(
        serde_json::to_value(&mismatch).unwrap(),
        json!({
            "kind": "mismatch",
            "fields": [
                "termination", "errorValue", "pc", "dataPointer", "currentCell",
                "stepsExecuted", "inputPosition", "output", "tape",
            ],
        })
    );
    assert_eq!(round_trip(&mismatch), mismatch);
}

#[test]
fn method_result_variants_have_fixed_discriminators() {
    let completed = MethodResultDto::Completed {
        method: ComparisonMethodDto::Naive,
        verdict: VerdictDto::Oracle,
        exit: Box::new(exit_state(TerminationDto::Halted)),
        metrics: metrics(None, UnitDto::NaiveInstructions { count: 2 }),
    };
    let value = serde_json::to_value(&completed).unwrap();
    assert_eq!(value["kind"], json!("completed"));
    assert_eq!(value["method"], json!("naive"));
    assert_eq!(value["verdict"], json!({"kind": "oracle"}));
    assert!(value["exit"].is_object());
    assert!(value["metrics"].is_object());
    assert_eq!(round_trip(&completed), completed);

    let unsupported = MethodResultDto::Unsupported {
        method: ComparisonMethodDto::Wasm,
        reason: UnsupportedReasonDto::ModuleTooLarge,
    };
    assert_eq!(
        serde_json::to_value(&unsupported).unwrap(),
        json!({"kind": "unsupported", "method": "wasm", "reason": "moduleTooLarge"})
    );
    let nesting = MethodResultDto::Unsupported {
        method: ComparisonMethodDto::Wasm,
        reason: UnsupportedReasonDto::NestingTooDeep,
    };
    assert_eq!(
        serde_json::to_value(&nesting).unwrap()["reason"],
        json!("nestingTooDeep")
    );
    assert_eq!(round_trip(&unsupported), unsupported);

    let failures = [
        (FailureKindDto::WatchdogTripped, "watchdogTripped"),
        (FailureKindDto::OutputLimitExceeded, "outputLimitExceeded"),
        (FailureKindDto::EngineError, "engineError"),
        (FailureKindDto::PolicyRejected, "policyRejected"),
        (
            FailureKindDto::InconsistentRepetitions,
            "inconsistentRepetitions",
        ),
    ];
    for (failure, name) in failures {
        let failed = MethodResultDto::Failed {
            method: ComparisonMethodDto::Optimized,
            failure,
        };
        assert_eq!(
            serde_json::to_value(&failed).unwrap(),
            json!({"kind": "failed", "method": "optimized", "failure": name})
        );
        assert_eq!(round_trip(&failed), failed);
    }
}

#[test]
fn overall_names() {
    for (overall, name) in [
        (OverallDto::AllMatch, "allMatch"),
        (OverallDto::Mismatch, "mismatch"),
        (OverallDto::Incomplete, "incomplete"),
    ] {
        assert_eq!(serde_json::to_value(overall).unwrap(), json!(name));
        assert_eq!(round_trip(&overall), overall);
    }
}

#[test]
fn comparison_result_parse_error_and_completed() {
    let parse = ComparisonResultDto::ParseError {
        error: ParseErrorDto {
            kind: ParseErrorKind::UnmatchedOpeningBracket,
            instruction_index: 0,
            position: SourcePositionDto {
                byte_offset: 0,
                utf16_offset: 0,
                line: 1,
                column: 1,
            },
        },
    };
    let value = serde_json::to_value(&parse).unwrap();
    assert_eq!(value["kind"], json!("parseError"));
    assert_eq!(value["error"]["kind"], json!("unmatchedOpeningBracket"));
    assert_eq!(round_trip(&parse), parse);

    let completed = ComparisonResultDto::Completed {
        limit: 1000,
        overall: OverallDto::AllMatch,
        methods: vec![MethodResultDto::Completed {
            method: ComparisonMethodDto::Naive,
            verdict: VerdictDto::Oracle,
            exit: Box::new(exit_state(TerminationDto::Halted)),
            metrics: metrics(None, UnitDto::NaiveInstructions { count: 2 }),
        }],
    };
    let value = serde_json::to_value(&completed).unwrap();
    assert_eq!(value["kind"], json!("completed"));
    assert_eq!(value["limit"], json!(1000));
    assert_eq!(value["overall"], json!("allMatch"));
    assert_eq!(value["methods"][0]["kind"], json!("completed"));
    assert_eq!(round_trip(&completed), completed);
}

#[test]
fn json_numbers_are_plain_numbers_not_strings() {
    let info = ComparisonInfoDto {
        max_steps: 50_000_000,
        max_repetitions: 5,
        default_repetitions: 3,
        methods: vec![],
    };
    let value: Value = serde_json::to_value(&info).unwrap();
    assert!(value["maxSteps"].is_u64());
    assert!(value["maxRepetitions"].is_u64());
}
