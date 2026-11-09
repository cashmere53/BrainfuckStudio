//! 比較サービス(`ComparisonService`、タスク 4.3)の契約テスト。
//!
//! 検査する範囲: 要求検証の境界と固定文面、解析エラーで方式を実行しないこと、素朴行が
//! デバッガ経路(複数バッチ幅)の最終状態・エラー値・窓・出力と一致すること、出力末尾の
//! 切り詰め、テープ窓の端のクランプ、総合結果、計測値の形、スレッド間共有と直列化。

use std::sync::Arc;
use std::thread;

use brainfuck_compare::limits::{DEFAULT_REPETITIONS, MAX_COMPARISON_STEPS, MAX_REPETITIONS};
use brainfuck_compare_api::dto::{
    ComparisonInfoDto, ComparisonMethodDto, ComparisonResultDto, ExitStateDto, MethodResultDto,
    OverallDto, RunComparisonRequest, TerminationDto, UnitDto, UnsupportedReasonDto, VerdictDto,
};
use brainfuck_compare_api::service::ComparisonService;
use brainfuck_gui_api::codec;
use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, BatchStopDto, LoadSourceRequest, LoadSourceResult, ParseErrorKind,
    RunBatchRequest, SessionStateDto, StartSessionRequest, StepLimitDto, ViewRequestDto,
};
use brainfuck_gui_api::limits::{
    DEFAULT_TAPE_RADIUS, MAX_INPUT_BYTES, MAX_SOURCE_BYTES, OUTPUT_TAIL_LIMIT,
};
use brainfuck_gui_api::service::DebuggerService;

/// ワークスペース共通のサンプル(コピーを置かない)。
const HELLO: &str = include_str!("../../../examples/hello.bf");

/// デバッガのバッチ実行の打ち切り上限(無限ループ防止の安全弁)。
const MAX_BATCHES: usize = 2_000_000;

fn request(
    source: &str,
    input: &[u8],
    max_steps: u64,
    methods: Vec<ComparisonMethodDto>,
    repetitions: u32,
) -> RunComparisonRequest {
    RunComparisonRequest {
        source: source.to_string(),
        input_base64: codec::encode(input),
        max_steps,
        methods,
        repetitions,
    }
}

fn run_ok(service: &ComparisonService, req: RunComparisonRequest) -> ComparisonResultDto {
    match service.run(req) {
        Ok(result) => result,
        Err(error) => panic!("run should succeed: {error:?}"),
    }
}

fn run_err(service: &ComparisonService, req: RunComparisonRequest) -> ApiError {
    match service.run(req) {
        Ok(result) => panic!("run should be rejected: {result:?}"),
        Err(error) => error,
    }
}

fn assert_rejected(error: &ApiError, kind: ApiErrorKind, field: Option<&str>) {
    assert_eq!(error.kind, kind, "error kind: {error:?}");
    assert_eq!(error.field.as_deref(), field, "field: {error:?}");
    assert_eq!(
        error.message,
        kind.fixed_message(),
        "fixed message: {error:?}"
    );
}

/// 完了した結果の総合結果と方式の一覧を取り出す。
fn completed(result: ComparisonResultDto) -> (OverallDto, Vec<MethodResultDto>) {
    match result {
        ComparisonResultDto::Completed {
            overall, methods, ..
        } => (overall, methods),
        other => panic!("expected completed result: {other:?}"),
    }
}

/// 先頭(素朴)の終了状態を取り出す。
fn naive_exit(result: &ComparisonResultDto) -> ExitStateDto {
    let ComparisonResultDto::Completed { methods, .. } = result else {
        panic!("expected completed result: {result:?}");
    };
    match &methods[0] {
        MethodResultDto::Completed { exit, .. } => (**exit).clone(),
        other => panic!("naive must complete: {other:?}"),
    }
}

/// デバッガ経路の最終地点(停止理由と停止時点の状態)。
struct DebuggerEnd {
    stop: BatchStopDto,
    state: SessionStateDto,
}

/// デバッガ API を、指定のバッチ幅で停止まで回して最終地点を返す(CLI と同じ経路)。
fn debugger_end(source: &str, input: &[u8], limit: u64, width: u64) -> DebuggerEnd {
    let service = DebuggerService::new();
    let program_id = match service
        .load_source(LoadSourceRequest {
            source: source.to_string(),
        })
        .expect("load_source")
    {
        LoadSourceResult::Ok { program_id, .. } => program_id,
        other => panic!("debugger parse failed: {other:?}"),
    };
    let view = ViewRequestDto {
        tape_radius: DEFAULT_TAPE_RADIUS,
        output_from: 0,
    };
    let started = service
        .start_session(StartSessionRequest {
            program_id,
            input_base64: codec::encode(input),
            limit: StepLimitDto::Limited { max_steps: limit },
            breakpoint_offsets: Vec::new(),
            view: view.clone(),
        })
        .expect("start_session");
    let session_id = started.state.session_id;
    for _ in 0..MAX_BATCHES {
        let result = service
            .run_batch(RunBatchRequest {
                session_id,
                batch_size: Some(width),
                view: view.clone(),
            })
            .expect("run_batch");
        match result.stop {
            BatchStopDto::BatchReached => continue,
            stop => {
                return DebuggerEnd {
                    stop,
                    state: result.state,
                }
            }
        }
    }
    panic!("debugger did not stop within {MAX_BATCHES} batches");
}

/// 素朴行の終了状態が、デバッガ経路の最終地点と一致することを確かめる。
fn assert_exit_matches(exit: &ExitStateDto, end: &DebuggerEnd, label: &str) {
    let snapshot = &end.state.snapshot;
    assert_eq!(exit.pc, snapshot.pc, "pc: {label}");
    assert_eq!(exit.dp, snapshot.dp, "dp: {label}");
    assert_eq!(exit.current_cell, snapshot.current_cell, "cell: {label}");
    assert_eq!(
        exit.steps_executed, snapshot.steps_executed,
        "steps: {label}"
    );
    assert_eq!(
        exit.input_consumed, end.state.input_consumed,
        "input: {label}"
    );
    assert_eq!(
        exit.input_total, end.state.input_total,
        "input total: {label}"
    );
    assert_eq!(exit.output_tail, end.state.output, "output: {label}");
    assert_eq!(exit.tape_window, end.state.tape_window, "window: {label}");
    match (&exit.termination, &end.stop) {
        (TerminationDto::Halted, BatchStopDto::Halted) => {}
        (TerminationDto::Error { error: naive }, BatchStopDto::Error { error: debugger }) => {
            assert_eq!(naive, debugger, "error value: {label}");
        }
        (naive, debugger) => panic!("termination differs ({label}): {naive:?} vs {debugger:?}"),
    }
}

/// 素朴行とデバッガ経路(複数のバッチ幅)の一致を確かめる。
fn assert_parity(source: &str, input: &[u8], limit: u64) {
    let service = ComparisonService::new();
    let result = run_ok(&service, request(source, input, limit, Vec::new(), 1));
    let exit = naive_exit(&result);
    for width in [1u64, 7, 1_000, 1_000_000] {
        let end = debugger_end(source, input, limit, width);
        assert_exit_matches(&exit, &end, &format!("limit={limit} width={width}"));
    }
}

/// 解析エラーの DTO を取り出す。
fn parse_error_of(result: ComparisonResultDto) -> brainfuck_gui_api::dto::ParseErrorDto {
    match result {
        ComparisonResultDto::ParseError { error } => error,
        other => panic!("expected parse error: {other:?}"),
    }
}

#[test]
fn info_reports_the_comparison_constants() {
    let info: ComparisonInfoDto = ComparisonService::new().info();
    assert_eq!(info.max_steps, MAX_COMPARISON_STEPS);
    assert_eq!(info.max_steps, 50_000_000);
    assert_eq!(info.max_repetitions, MAX_REPETITIONS);
    assert_eq!(info.max_repetitions, 5);
    assert_eq!(info.default_repetitions, DEFAULT_REPETITIONS);
    assert_eq!(info.default_repetitions, 3);
    assert_eq!(
        info.methods,
        vec![
            ComparisonMethodDto::Naive,
            ComparisonMethodDto::Optimized,
            ComparisonMethodDto::Wasm,
        ]
    );
}

#[test]
fn step_limit_is_checked_at_its_boundaries() {
    let service = ComparisonService::new();
    let zero = run_err(&service, request(HELLO, b"", 0, Vec::new(), 1));
    assert_rejected(&zero, ApiErrorKind::InvalidArgument, Some("maxSteps"));
    let over = run_err(
        &service,
        request(HELLO, b"", MAX_COMPARISON_STEPS + 1, Vec::new(), 1),
    );
    assert_rejected(&over, ApiErrorKind::InvalidArgument, Some("maxSteps"));

    // 1 と最大値は受け付ける(hello は十分な上限で停止する)。
    let min = run_ok(&service, request(HELLO, b"", 1, Vec::new(), 1));
    let (_, methods) = completed(min);
    assert!(matches!(methods[0], MethodResultDto::Completed { .. }));
    let max = run_ok(
        &service,
        request(HELLO, b"", MAX_COMPARISON_STEPS, Vec::new(), 1),
    );
    completed(max);
}

#[test]
fn repetitions_are_checked_at_their_boundaries() {
    let service = ComparisonService::new();
    for bad in [0u32, MAX_REPETITIONS + 1] {
        let error = run_err(&service, request(HELLO, b"", 1_000, Vec::new(), bad));
        assert_rejected(&error, ApiErrorKind::InvalidArgument, Some("repetitions"));
    }
    for good in [1u32, MAX_REPETITIONS] {
        completed(run_ok(
            &service,
            request(HELLO, b"", 1_000_000, Vec::new(), good),
        ));
    }
}

#[test]
fn duplicate_or_naive_methods_are_rejected_and_the_service_stays_usable() {
    let service = ComparisonService::new();
    let before = completed(run_ok(
        &service,
        request(
            HELLO,
            b"",
            1_000_000,
            vec![ComparisonMethodDto::Optimized],
            1,
        ),
    ));

    let duplicate = run_err(
        &service,
        request(
            HELLO,
            b"",
            1_000_000,
            vec![
                ComparisonMethodDto::Optimized,
                ComparisonMethodDto::Optimized,
            ],
            1,
        ),
    );
    assert_rejected(&duplicate, ApiErrorKind::InvalidArgument, Some("methods"));

    let naive = run_err(
        &service,
        request(HELLO, b"", 1_000_000, vec![ComparisonMethodDto::Naive], 1),
    );
    assert_rejected(&naive, ApiErrorKind::InvalidArgument, Some("methods"));

    // 拒否の後も、同じ要求は同じ結果になる(状態を変えない)。
    let after = completed(run_ok(
        &service,
        request(
            HELLO,
            b"",
            1_000_000,
            vec![ComparisonMethodDto::Optimized],
            1,
        ),
    ));
    assert_eq!(before.0, after.0);
    assert_eq!(before.1.len(), after.1.len());
}

#[test]
fn invalid_base64_and_oversize_input_are_rejected_with_the_field_name() {
    let service = ComparisonService::new();
    let mut bad = request(HELLO, b"", 1_000, Vec::new(), 1);
    bad.input_base64 = "!!not-base64!!".to_string();
    let error = run_err(&service, bad);
    assert_rejected(&error, ApiErrorKind::InvalidArgument, Some("inputBase64"));

    // 復号後 1 MiB + 1 バイトは入力超過(テキスト長では区別できないため復号後で判定する)。
    let oversize = vec![0u8; MAX_INPUT_BYTES + 1];
    let error = run_err(&service, request(HELLO, &oversize, 1_000, Vec::new(), 1));
    assert_rejected(&error, ApiErrorKind::InputTooLarge, Some("inputBase64"));

    // 上限ちょうどは受け付ける(入力は使われないため停止する)。
    let exact = vec![0u8; MAX_INPUT_BYTES];
    completed(run_ok(
        &service,
        request(HELLO, &exact, 1_000_000, Vec::new(), 1),
    ));
}

#[test]
fn oversize_source_is_rejected_and_the_boundary_is_accepted() {
    let service = ComparisonService::new();
    let too_large = "a".repeat(MAX_SOURCE_BYTES + 1);
    let error = run_err(&service, request(&too_large, b"", 1_000, Vec::new(), 1));
    assert_rejected(&error, ApiErrorKind::SourceTooLarge, Some("source"));

    // 上限ちょうど(命令のない文字列)は受け付け、素朴は直ちに停止する。
    let exact = "a".repeat(MAX_SOURCE_BYTES);
    let (_, methods) = completed(run_ok(&service, request(&exact, b"", 1_000, Vec::new(), 1)));
    match &methods[0] {
        MethodResultDto::Completed { exit, .. } => {
            assert_eq!(exit.termination, TerminationDto::Halted);
            assert_eq!(exit.steps_executed, 0);
        }
        other => panic!("naive must complete: {other:?}"),
    }
}

#[test]
fn rejection_messages_are_fixed_and_carry_no_source_or_input() {
    let service = ComparisonService::new();
    let secret_source = "SECRET-SOURCE-7f3a";
    let mut bad = request(secret_source, b"", 1_000, Vec::new(), 1);
    bad.input_base64 = "SECRET-INPUT-!!".to_string();
    let error = run_err(&service, bad);
    let text = format!("{error:?}");
    assert!(!text.contains("SECRET"), "error leaks input: {text}");
    assert_rejected(&error, ApiErrorKind::InvalidArgument, Some("inputBase64"));
}

#[test]
fn parse_error_is_reported_without_running_any_method() {
    let service = ComparisonService::new();
    // 方式を要求しても解析エラーで止まり、完了の結果は作られない。
    let result = run_ok(
        &service,
        request(
            "+]",
            b"",
            1_000,
            vec![ComparisonMethodDto::Optimized, ComparisonMethodDto::Wasm],
            1,
        ),
    );
    let error = parse_error_of(result);
    assert_eq!(error.kind, ParseErrorKind::UnmatchedClosingBracket);
    assert_eq!(error.instruction_index, 1);
    assert_eq!(error.position.utf16_offset, 1);

    let unmatched_open = parse_error_of(run_ok(&service, request("[", b"", 1_000, Vec::new(), 1)));
    assert_eq!(unmatched_open.kind, ParseErrorKind::UnmatchedOpeningBracket);
    assert_eq!(unmatched_open.instruction_index, 0);
}

#[test]
fn naive_matches_the_debugger_path_for_hello_and_its_exact_limit() {
    // hello の必要数 N を求め、N(停止する)と N − 1(上限超過)の両方で一致を確かめる。
    let service = ComparisonService::new();
    let needed = naive_exit(&run_ok(
        &service,
        request(HELLO, b"", 1_000_000, Vec::new(), 1),
    ))
    .steps_executed;
    assert!(needed > 1);
    assert_parity(HELLO, b"", needed);
    assert_parity(HELLO, b"", needed - 1);
}

#[test]
fn naive_matches_the_debugger_path_for_echo_and_input_exhaustion() {
    assert_parity(",[.,]", b"echo me\n", 100_000);
    // 入力が尽きてからの読み取りと、尽きた状態での出力。
    assert_parity(",,,.", b"a", 100_000);
}

#[test]
fn naive_matches_the_debugger_path_for_pointer_range_and_step_limit() {
    assert_parity("<", b"", 1_000);
    assert_parity("+>+<<", b"", 1_000);
    let right = ">".repeat(30_000);
    assert_parity(&right, b"", 100_000);
    // 無限ループは上限で止まる(上限超過のエラー値)。
    assert_parity("+[]", b"", 5_000);
}

#[test]
fn output_tail_keeps_the_last_64_kib_and_the_total_length() {
    // `+[.]` は 1 バイトずつ出力し続ける(2 命令で 1 バイト)。
    let service = ComparisonService::new();
    let result = run_ok(&service, request("+[.]", b"", 200_000, Vec::new(), 1));
    let exit = naive_exit(&result);
    let tail = &exit.output_tail;
    assert!(
        tail.total_len > OUTPUT_TAIL_LIMIT,
        "total {}",
        tail.total_len
    );
    assert_eq!(tail.data_start, tail.total_len - OUTPUT_TAIL_LIMIT);
    let bytes = codec::decode(&tail.data_base64, OUTPUT_TAIL_LIMIT).expect("decode tail");
    assert_eq!(bytes.len(), OUTPUT_TAIL_LIMIT);
    assert!(bytes.iter().all(|&b| b == 1));
    assert_parity("+[.]", b"", 200_000);
}

#[test]
fn small_output_is_returned_from_the_start() {
    let service = ComparisonService::new();
    let result = run_ok(&service, request(HELLO, b"", 1_000_000, Vec::new(), 1));
    let exit = naive_exit(&result);
    assert_eq!(exit.output_tail.data_start, 0);
    assert!(exit.output_tail.total_len > 0);
}

#[test]
fn tape_window_is_clamped_at_the_left_edge() {
    let service = ComparisonService::new();
    let result = run_ok(&service, request("+", b"", 10, Vec::new(), 1));
    let window = naive_exit(&result).tape_window;
    assert_eq!(window.start, 0);
    assert_eq!(window.pointer_offset, 0);
    // 左端では [0, dp + 半径] の範囲だけになる(半径 + 1 セル)。
    assert_eq!(window.cells.len(), DEFAULT_TAPE_RADIUS + 1);
    assert_eq!(window.cells[0], 1);
}

#[test]
fn tape_window_is_clamped_at_the_right_edge() {
    let service = ComparisonService::new();
    let source = format!("{}+", ">".repeat(29_999));
    let result = run_ok(&service, request(&source, b"", 100_000, Vec::new(), 1));
    let exit = naive_exit(&result);
    let window = exit.tape_window;
    assert_eq!(exit.dp, 29_999);
    assert_eq!(window.start, 29_999 - DEFAULT_TAPE_RADIUS);
    assert_eq!(window.cells.len(), DEFAULT_TAPE_RADIUS + 1);
    assert_eq!(window.pointer_offset, DEFAULT_TAPE_RADIUS);
    assert_eq!(*window.cells.last().expect("cell"), 1);
    assert_parity(&source, b"", 100_000);
}

#[test]
fn tape_digest_is_sixteen_lowercase_hex_digits() {
    let service = ComparisonService::new();
    let result = run_ok(&service, request(HELLO, b"", 1_000_000, Vec::new(), 1));
    let digest = naive_exit(&result).tape_digest;
    assert_eq!(digest.len(), 16, "digest: {digest}");
    assert!(digest
        .chars()
        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
}

#[test]
fn all_methods_agreeing_give_an_all_match_overall() {
    let service = ComparisonService::new();
    let result = run_ok(
        &service,
        request(
            HELLO,
            b"",
            1_000_000,
            vec![ComparisonMethodDto::Optimized, ComparisonMethodDto::Wasm],
            1,
        ),
    );
    let (overall, methods) = completed(result);
    assert_eq!(overall, OverallDto::AllMatch);
    assert_eq!(methods.len(), 3);
    match &methods[0] {
        MethodResultDto::Completed { verdict, .. } => assert_eq!(*verdict, VerdictDto::Oracle),
        other => panic!("naive must complete: {other:?}"),
    }
    for method in &methods[1..] {
        match method {
            MethodResultDto::Completed { verdict, .. } => assert_eq!(*verdict, VerdictDto::Match),
            other => panic!("method must complete: {other:?}"),
        }
    }
}

#[test]
fn metrics_carry_the_method_specific_units_and_micro_timings() {
    let service = ComparisonService::new();
    let result = run_ok(
        &service,
        request(
            HELLO,
            b"",
            1_000_000,
            vec![ComparisonMethodDto::Optimized, ComparisonMethodDto::Wasm],
            2,
        ),
    );
    let (_, methods) = completed(result);
    let metrics = |index: usize| match &methods[index] {
        MethodResultDto::Completed { metrics, .. } => metrics.clone(),
        other => panic!("method must complete: {other:?}"),
    };
    let naive = metrics(0);
    assert_eq!(naive.prepare_micros, None);
    assert_eq!(naive.repetitions, 2);
    assert!(matches!(naive.unit, UnitDto::NaiveInstructions { .. }));
    let optimized = metrics(1);
    assert!(optimized.prepare_micros.is_some());
    assert!(matches!(optimized.unit, UnitDto::IrOperations { .. }));
    let wasm = metrics(2);
    assert!(wasm.artifact_bytes.is_some());
    assert_eq!(wasm.unit, UnitDto::None);
}

#[test]
fn service_is_shared_across_threads_and_serializes_concurrent_runs() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ComparisonService>();

    let service = Arc::new(ComparisonService::new());
    let expected = naive_exit(&run_ok(
        &service,
        request(HELLO, b"", 1_000_000, Vec::new(), 1),
    ));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let service = Arc::clone(&service);
            thread::spawn(move || {
                (0..3)
                    .map(|_| {
                        naive_exit(&run_ok(
                            &service,
                            request(HELLO, b"", 1_000_000, Vec::new(), 1),
                        ))
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    for handle in handles {
        for exit in handle.join().expect("thread must not panic") {
            assert_eq!(exit, expected);
        }
    }
}

#[test]
fn request_validation_runs_before_parsing() {
    // 解析不可能なソースと不正な繰り返し回数を同時に与える。検証が先なので操作エラーになる。
    let service = ComparisonService::new();
    let error = run_err(&service, request("]", b"", 1_000, Vec::new(), 0));
    assert_rejected(&error, ApiErrorKind::InvalidArgument, Some("repetitions"));

    let error = run_err(
        &service,
        request("]", b"", 1_000, Vec::new(), MAX_REPETITIONS + 1),
    );
    assert_rejected(&error, ApiErrorKind::InvalidArgument, Some("repetitions"));

    let error = run_err(&service, request("]", b"", 0, Vec::new(), 1));
    assert_rejected(&error, ApiErrorKind::InvalidArgument, Some("maxSteps"));
}

#[test]
fn completed_result_echoes_the_requested_step_limit() {
    let service = ComparisonService::new();
    let result = run_ok(&service, request("+", b"", 4_321, Vec::new(), 1));
    match result {
        ComparisonResultDto::Completed { limit, .. } => assert_eq!(limit, 4_321),
        other => panic!("expected completed: {other:?}"),
    }
}

#[test]
fn too_deep_nesting_is_unsupported_for_wasm_and_makes_the_overall_incomplete() {
    // 入れ子 257 は WASM の上限(256)を超える。対象外は他方式を妨げず、総合結果は未完了になる。
    let source = format!("{}{}", "[".repeat(257), "]".repeat(257));
    let service = ComparisonService::new();
    let (overall, methods) = completed(run_ok(
        &service,
        request(
            &source,
            b"",
            1_000,
            vec![ComparisonMethodDto::Optimized, ComparisonMethodDto::Wasm],
            1,
        ),
    ));
    assert_eq!(overall, OverallDto::Incomplete);
    assert!(matches!(methods[1], MethodResultDto::Completed { .. }));
    assert_eq!(
        methods[2],
        MethodResultDto::Unsupported {
            method: ComparisonMethodDto::Wasm,
            reason: UnsupportedReasonDto::NestingTooDeep,
        }
    );
}
