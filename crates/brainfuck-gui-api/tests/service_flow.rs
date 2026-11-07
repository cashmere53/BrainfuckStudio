//! `DebuggerService` の読み込みとセッション開始(2.4)の振る舞い。
//!
//! 各ケース: 解析エラー・成功・古いプログラム ID・過大入力・命令数上限の有無・不正な値の拒否。
//! 不正な要求が状態を変えないことは、拒否後も従前のプログラム/セッションで要求が通ることで確認する。

mod common;

use brainfuck_gui_api::dto::{
    ApiErrorKind, BreakpointDto, LoadSourceRequest, LoadSourceResult, ParseErrorKind,
    SourcePositionDto, SourceRangeDto, StartSessionRequest, StepLimitDto, TapeWindowDto,
};
use brainfuck_gui_api::limits::{
    MAX_BREAKPOINT_OFFSETS, MAX_INPUT_BYTES, MAX_SOURCE_BYTES, MAX_STEP_LIMIT,
};
use brainfuck_gui_api::service::DebuggerService;

use common::{load_ok, service, start_request, view};

fn pos(byte_offset: usize, utf16_offset: usize, line: usize, column: usize) -> SourcePositionDto {
    SourcePositionDto {
        byte_offset,
        utf16_offset,
        line,
        column,
    }
}

fn range(start: SourcePositionDto, end: SourcePositionDto) -> SourceRangeDto {
    SourceRangeDto { start, end }
}

fn limited(max_steps: u64) -> StepLimitDto {
    StepLimitDto::Limited { max_steps }
}

#[test]
fn service_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DebuggerService>();
}

#[test]
fn load_source_success_reports_counts_and_a_new_program_id() {
    let service = service();
    // `+` と `.` が命令。` comment` と改行はそれ以外の文字なので命令ではない。
    let result = service
        .load_source(LoadSourceRequest {
            source: "++ comment\n+.".to_owned(),
        })
        .expect("valid source");
    match result {
        LoadSourceResult::Ok {
            program_id,
            instruction_count,
            line_count,
        } => {
            assert_ne!(program_id, 0);
            assert_eq!(instruction_count, 4);
            assert_eq!(line_count, 2);
        }
        other => panic!("expected ok, got {other:?}"),
    }
}

#[test]
fn each_successful_load_gets_a_different_program_id() {
    let service = service();
    let first = load_ok(&service, "+");
    let second = load_ok(&service, "+");
    assert_ne!(first, second);
}

#[test]
fn parse_error_reports_kind_instruction_index_and_position_with_multibyte_prefix() {
    let service = service();
    // 先頭の `あ`(3 バイト・UTF-16 1 単位)と改行の後の `]` が余分な閉じ括弧。
    let result = service
        .load_source(LoadSourceRequest {
            source: "あ\n+]".to_owned(),
        })
        .expect("parse errors are results, not API errors");
    match result {
        LoadSourceResult::ParseError { error } => {
            assert_eq!(error.kind, ParseErrorKind::UnmatchedClosingBracket);
            assert_eq!(error.instruction_index, 1);
            // `あ`(3 バイト) + `\n`(1 バイト) + `+`(1 バイト) の後が `]` の位置。
            assert_eq!(error.position, pos(5, 3, 2, 2));
        }
        other => panic!("expected parse error, got {other:?}"),
    }
}

#[test]
fn unmatched_opening_bracket_is_reported_at_its_own_position() {
    let service = service();
    let result = service
        .load_source(LoadSourceRequest {
            source: "+[".to_owned(),
        })
        .expect("parse errors are results");
    match result {
        LoadSourceResult::ParseError { error } => {
            assert_eq!(error.kind, ParseErrorKind::UnmatchedOpeningBracket);
            assert_eq!(error.instruction_index, 1);
            assert_eq!(error.position, pos(1, 1, 1, 2));
        }
        other => panic!("expected parse error, got {other:?}"),
    }
}

#[test]
fn failed_load_keeps_the_previous_program_and_session_usable() {
    let service = service();
    let program = load_ok(&service, "+.");
    service
        .start_session(start_request(program, b"", limited(1_000), vec![]))
        .expect("session starts");

    let result = service
        .load_source(LoadSourceRequest {
            source: "]".to_owned(),
        })
        .expect("parse error is a result");
    assert!(matches!(result, LoadSourceResult::ParseError { .. }));

    // 解析に失敗した読み込みは既存のプログラムを置き換えない。
    service
        .start_session(start_request(program, b"", limited(1_000), vec![]))
        .expect("the previous program is still loaded");
}

#[test]
fn source_over_one_mib_is_rejected_with_the_field_name_only() {
    let service = service();
    let error = service
        .load_source(LoadSourceRequest {
            source: "+".repeat(MAX_SOURCE_BYTES + 1),
        })
        .expect_err("source over the limit");
    assert_eq!(error.kind, ApiErrorKind::SourceTooLarge);
    assert_eq!(error.field.as_deref(), Some("source"));
    assert_eq!(error.message, "source is too large");
}

#[test]
fn source_of_exactly_one_mib_is_accepted() {
    let service = service();
    let result = service
        .load_source(LoadSourceRequest {
            source: "+".repeat(MAX_SOURCE_BYTES),
        })
        .expect("exactly the limit is accepted");
    match result {
        LoadSourceResult::Ok {
            instruction_count, ..
        } => assert_eq!(instruction_count, MAX_SOURCE_BYTES),
        other => panic!("expected ok, got {other:?}"),
    }
}

#[test]
fn start_session_without_a_loaded_program_is_program_not_loaded() {
    let service = service();
    let error = service
        .start_session(start_request(1, b"", limited(1_000), vec![]))
        .expect_err("no program is loaded");
    assert_eq!(error.kind, ApiErrorKind::ProgramNotLoaded);
}

#[test]
fn start_session_after_a_parse_error_without_a_program_is_program_not_loaded() {
    let service = service();
    service
        .load_source(LoadSourceRequest {
            source: "[".to_owned(),
        })
        .expect("parse error is a result");
    let error = service
        .start_session(start_request(1, b"", limited(1_000), vec![]))
        .expect_err("a failed load registers no program");
    assert_eq!(error.kind, ApiErrorKind::ProgramNotLoaded);
}

#[test]
fn start_session_with_a_superseded_program_id_is_stale_program() {
    let service = service();
    let old = load_ok(&service, "+");
    let current = load_ok(&service, "+.");
    assert_ne!(old, current);

    let error = service
        .start_session(start_request(old, b"", limited(1_000), vec![]))
        .expect_err("the old program id is stale");
    assert_eq!(error.kind, ApiErrorKind::StaleProgram);
    assert_eq!(error.field, None);

    // 拒否された要求は状態を変えず、現在のプログラムでは開始できる。
    service
        .start_session(start_request(current, b"", limited(1_000), vec![]))
        .expect("the current program starts");
}

#[test]
fn successful_load_discards_the_running_session_by_superseding_its_program() {
    let service = service();
    let old = load_ok(&service, "+.");
    service
        .start_session(start_request(old, b"", limited(1_000), vec![]))
        .expect("session starts");

    load_ok(&service, "+.");

    let error = service
        .start_session(start_request(old, b"", limited(1_000), vec![]))
        .expect_err("the old program is gone");
    assert_eq!(error.kind, ApiErrorKind::StaleProgram);
}

#[test]
fn start_session_returns_the_initial_state_of_a_fresh_session() {
    let service = service();
    let program = load_ok(&service, "+.");
    let started = service
        .start_session(start_request(program, b"AB", limited(1_000), vec![]))
        .expect("session starts");

    let state = started.state;
    assert_ne!(state.session_id, 0);
    assert_ne!(state.session_id, program);
    assert_eq!(state.snapshot.pc, 0);
    assert_eq!(state.snapshot.dp, 0);
    assert_eq!(state.snapshot.current_cell, 0);
    assert_eq!(state.snapshot.steps_executed, 0);
    assert!(!state.snapshot.halted);
    // 既定の要求は半径 16。先頭側は 0 で切り詰め、窓は 0..=16 の 17 セル。
    assert_eq!(
        state.tape_window,
        TapeWindowDto {
            start: 0,
            cells: vec![0; 17],
            pointer_offset: 0,
        }
    );
    assert_eq!(state.output.data_start, 0);
    assert_eq!(state.output.total_len, 0);
    assert_eq!(state.output.data_base64, "");
    assert_eq!(state.input_consumed, 0);
    assert_eq!(state.input_total, 2);
    assert_eq!(state.last_stop, None);
    // 現在の命令は先頭の `+`。
    assert_eq!(
        state.current_range,
        Some(range(pos(0, 0, 1, 1), pos(1, 1, 1, 2)))
    );
    assert!(started.breakpoints.breakpoints.is_empty());
    assert!(started.breakpoints.unresolved.is_empty());
}

#[test]
fn start_session_reports_no_current_range_for_an_empty_program() {
    let service = service();
    let program = load_ok(&service, "");
    let started = service
        .start_session(start_request(program, b"", limited(1_000), vec![]))
        .expect("an empty program starts");
    assert_eq!(started.state.current_range, None);
}

#[test]
fn start_session_uses_the_tape_radius_of_the_view() {
    let service = service();
    let program = load_ok(&service, "+.");
    let mut request = start_request(program, b"", limited(1_000), vec![]);
    request.view = view(0, 0);
    let started = service.start_session(request).expect("session starts");
    assert_eq!(
        started.state.tape_window,
        TapeWindowDto {
            start: 0,
            cells: vec![0],
            pointer_offset: 0,
        }
    );
}

#[test]
fn breakpoints_resolve_to_the_next_instruction_and_collapse_duplicates() {
    let service = service();
    // 命令は `+`(添字 0)と、2 行目の `+`(添字 1)。空白と `x` はコメント。
    let program = load_ok(&service, "+ x\n+");
    let started = service
        .start_session(start_request(
            program,
            b"",
            limited(1_000),
            // 1・2 はコメント上で、次の命令(添字 1)へ解決される。4 は添字 1 そのもの。
            // 0 は添字 0。99 は命令が無い位置で未解決。
            vec![1, 2, 4, 0, 99, 99],
        ))
        .expect("session starts");

    assert_eq!(
        started.breakpoints.breakpoints,
        vec![
            BreakpointDto {
                utf16_offset: 0,
                instruction_index: 0,
                range: range(pos(0, 0, 1, 1), pos(1, 1, 1, 2)),
            },
            BreakpointDto {
                // 同じ命令へ解決した要求のうち最初の要求位置を写す。
                utf16_offset: 1,
                instruction_index: 1,
                range: range(pos(4, 4, 2, 1), pos(5, 5, 2, 2)),
            },
        ]
    );
    // 未解決の位置は昇順・重複なし。
    assert_eq!(started.breakpoints.unresolved, vec![99]);
}

#[test]
fn breakpoint_count_limit_is_enforced_at_the_boundary() {
    let service = service();
    let program = load_ok(&service, "+");

    let at_limit = vec![0usize; MAX_BREAKPOINT_OFFSETS];
    service
        .start_session(start_request(program, b"", limited(1_000), at_limit))
        .expect("exactly the limit is accepted");

    let over_limit = vec![0usize; MAX_BREAKPOINT_OFFSETS + 1];
    let error = service
        .start_session(start_request(program, b"", limited(1_000), over_limit))
        .expect_err("one over the limit");
    assert_eq!(error.kind, ApiErrorKind::InvalidArgument);
    assert_eq!(error.field.as_deref(), Some("breakpointOffsets"));
}

#[test]
fn oversized_input_is_rejected_and_one_mib_is_accepted() {
    let service = service();
    let program = load_ok(&service, "+");

    let at_limit = vec![0x20u8; MAX_INPUT_BYTES];
    let started = service
        .start_session(start_request(program, &at_limit, limited(1_000), vec![]))
        .expect("exactly 1 MiB is accepted");
    assert_eq!(started.state.input_total, MAX_INPUT_BYTES);

    let over_limit = vec![0x20u8; MAX_INPUT_BYTES + 1];
    let error = service
        .start_session(start_request(program, &over_limit, limited(1_000), vec![]))
        .expect_err("one byte over the limit");
    assert_eq!(error.kind, ApiErrorKind::InputTooLarge);
    assert_eq!(error.field.as_deref(), Some("inputBase64"));
}

#[test]
fn invalid_base64_input_is_an_invalid_argument() {
    let service = service();
    let program = load_ok(&service, "+");
    let mut request = start_request(program, b"", limited(1_000), vec![]);
    request.input_base64 = "@@@@".to_owned();
    let error = service.start_session(request).expect_err("not Base64");
    assert_eq!(error.kind, ApiErrorKind::InvalidArgument);
    assert_eq!(error.field.as_deref(), Some("inputBase64"));
}

#[test]
fn step_limit_must_be_between_one_and_the_json_safe_maximum() {
    let service = service();
    let program = load_ok(&service, "+");

    for bad in [0, MAX_STEP_LIMIT + 1] {
        let error = service
            .start_session(start_request(program, b"", limited(bad), vec![]))
            .expect_err("out of range");
        assert_eq!(error.kind, ApiErrorKind::InvalidArgument);
        assert_eq!(error.field.as_deref(), Some("maxSteps"));
    }

    for good in [1, MAX_STEP_LIMIT] {
        service
            .start_session(start_request(program, b"", limited(good), vec![]))
            .expect("in range");
    }

    service
        .start_session(start_request(program, b"", StepLimitDto::Unlimited, vec![]))
        .expect("unlimited is accepted");
}

#[test]
fn tape_radius_above_the_limit_is_an_invalid_argument() {
    let service = service();
    let program = load_ok(&service, "+");
    let mut request: StartSessionRequest = start_request(program, b"", limited(1_000), vec![]);
    request.view = view(65, 0);
    let error = service
        .start_session(request)
        .expect_err("radius 65 is over the limit");
    assert_eq!(error.kind, ApiErrorKind::InvalidArgument);
    assert_eq!(error.field.as_deref(), Some("tapeRadius"));
}

#[test]
fn tape_radius_limit_is_enforced_at_the_boundary() {
    let service = service();
    let program = load_ok(&service, "+");

    let mut at_limit = start_request(program, b"", limited(1_000), vec![]);
    at_limit.view = view(64, 0);
    let started = service
        .start_session(at_limit)
        .expect("radius 64 is accepted");
    assert_eq!(started.state.tape_window.cells.len(), 65);

    let mut over_limit = start_request(program, b"", limited(1_000), vec![]);
    over_limit.view = view(65, 0);
    let error = service
        .start_session(over_limit)
        .expect_err("radius 65 is over the limit");
    assert_eq!(error.kind, ApiErrorKind::InvalidArgument);
    assert_eq!(error.field.as_deref(), Some("tapeRadius"));
}

#[test]
fn rejected_start_leaves_a_running_session_untouched() {
    let service = service();
    let program = load_ok(&service, "+.");
    service
        .start_session(start_request(program, b"", limited(1_000), vec![]))
        .expect("session starts");

    let mut bad = start_request(program, b"", limited(1_000), vec![]);
    bad.view = view(65, 0);
    service
        .start_session(bad)
        .expect_err("rejected before any change");

    // 拒否の後も同じプログラムから新しいセッションを開始できる(プログラムは失われていない)。
    let started = service
        .start_session(start_request(program, b"x", limited(1_000), vec![]))
        .expect("program is still loaded");
    assert_eq!(started.state.input_total, 1);
}
