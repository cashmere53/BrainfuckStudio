//! 比較コマンド(`get_comparison_info` / `run_comparison`)の登録・IPC・実行基盤を検査する。

use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager};

use brainfuck_compare_api::ComparisonService;
use brainfuck_gui_api::dto::{ApiError, ApiErrorKind};
use brainfuck_studio::app_state::AppState;
use brainfuck_studio::commands::comparison::run_blocking;
use brainfuck_studio::commands::names::COMMAND_NAMES;
use brainfuck_studio::commands::REGISTERED_COMMANDS;

const COMPARISON_COMMAND_NAMES: [&str; 2] = ["get_comparison_info", "run_comparison"];

fn mock_app() -> App<MockRuntime> {
    mock_builder()
        .manage(AppState::default())
        .invoke_handler(brainfuck_studio::commands::invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("mock app builds")
}

fn ipc_request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.to_owned(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .expect("valid url"),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_owned(),
    }
}

fn request_body(source: &str, max_steps: u64) -> serde_json::Value {
    serde_json::json!({
        "args": {
            "source": source,
            "inputBase64": "",
            "maxSteps": max_steps,
            "methods": ["optimized"],
            "repetitions": 1
        }
    })
}

#[test]
fn comparison_commands_are_registered_and_listed_for_the_build_manifest() {
    for name in COMPARISON_COMMAND_NAMES {
        assert!(
            REGISTERED_COMMANDS.contains(&name),
            "{name} is not registered"
        );
        assert!(COMMAND_NAMES.contains(&name), "{name} has no permission");
    }
}

#[test]
fn get_comparison_info_returns_the_service_constants_over_ipc() {
    let app = mock_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock window builds");
    let body = get_ipc_response(
        &webview,
        ipc_request("get_comparison_info", serde_json::json!({})),
    )
    .expect("get_comparison_info succeeds");
    let value: serde_json::Value = body.deserialize().expect("JSON response");
    let expected = ComparisonService::new().info();
    assert_eq!(value["maxSteps"], serde_json::json!(expected.max_steps));
    assert_eq!(
        value["maxRepetitions"],
        serde_json::json!(expected.max_repetitions)
    );
    assert_eq!(
        value["defaultRepetitions"],
        serde_json::json!(expected.default_repetitions)
    );
}

#[test]
fn run_comparison_takes_its_request_inside_args_and_reports_parse_errors() {
    let app = mock_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock window builds");
    // 解析エラー(括弧の不一致)は方式を実行せず、解析エラーの結果として返る。
    let body = get_ipc_response(
        &webview,
        ipc_request("run_comparison", request_body("[", 1000)),
    )
    .expect("run_comparison accepts the request");
    let value: serde_json::Value = body.deserialize().expect("JSON response");
    assert_eq!(value["kind"], "parseError");
}

#[test]
fn run_comparison_rejects_an_out_of_range_step_limit_with_a_field_name() {
    let app = mock_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock window builds");
    let error = get_ipc_response(
        &webview,
        ipc_request("run_comparison", request_body("+.", 0)),
    )
    .expect_err("a zero step limit is rejected");
    assert_eq!(error["kind"], "invalidArgument");
    assert_eq!(error["field"], "maxSteps");
}

#[test]
fn app_state_holds_the_shared_comparison_service() {
    let app = mock_app();
    let handle = app.handle().clone();
    let info = handle.state::<AppState>().comparison.info();
    assert_eq!(info, ComparisonService::new().info());
}

#[test]
fn a_panicking_blocking_task_becomes_a_fixed_internal_error() {
    let result: Result<(), ApiError> = tauri::async_runtime::block_on(run_blocking(|| {
        panic!("simulated failure inside the comparison");
    }));
    let error = result.expect_err("a panic must not succeed");
    assert_eq!(error.kind, ApiErrorKind::Internal);
    assert_eq!(error.field, None);
}

#[test]
fn a_blocking_task_returns_its_value_when_it_does_not_panic() {
    let result: Result<u32, ApiError> = tauri::async_runtime::block_on(run_blocking(|| Ok(7)));
    assert_eq!(result.expect("the value is returned"), 7);
}
