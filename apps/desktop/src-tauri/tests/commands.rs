//! コマンドの登録・ビルド設定・ケイパビリティの一致と、コマンド層の振る舞いを検査する。

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

use brainfuck_gui_api::dto::{
    BatchResultDto, BatchStopDto, LoadSourceRequest, LoadSourceResult, PauseRequest,
    RunBatchRequest, StartSessionRequest, StepLimitDto, ViewRequestDto,
};
use brainfuck_gui_api::limits::MAX_BATCH_SIZE;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager};

use brainfuck_studio::app_state::AppState;
use brainfuck_studio::commands::names::COMMAND_NAMES;
use brainfuck_studio::commands::REGISTERED_COMMANDS;

/// `load_source` → `allow-load-source`(Tauri の権限名は snake_case を kebab-case にする)。
fn allow_permission(command: &str) -> String {
    format!("allow-{}", command.replace('_', "-"))
}

fn capabilities_permissions() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let text = std::fs::read_to_string(&path).expect("capabilities/default.json is readable");
    let value: serde_json::Value = serde_json::from_str(&text).expect("capabilities is JSON");
    value["permissions"]
        .as_array()
        .expect("permissions is an array")
        .iter()
        .map(|p| p.as_str().expect("permission is a string").to_owned())
        .collect()
}

#[test]
fn command_names_are_unique() {
    let unique: BTreeSet<&str> = COMMAND_NAMES.iter().copied().collect();
    assert_eq!(unique.len(), COMMAND_NAMES.len());
}

#[test]
fn registered_commands_match_the_build_manifest_list() {
    // 登録側(invoke ハンドラと同じ一覧から作る)と権限生成側の一覧が集合として一致すること。
    // 片方だけに名前がある場合、WebView から呼べない(権限が無い)か、権限だけあって登録されていない。
    let registered: BTreeSet<&str> = REGISTERED_COMMANDS.iter().copied().collect();
    let manifest: BTreeSet<&str> = COMMAND_NAMES.iter().copied().collect();
    assert_eq!(registered, manifest);
    assert_eq!(REGISTERED_COMMANDS.len(), registered.len());
}

#[test]
fn capabilities_allow_exactly_the_registered_commands() {
    let mut expected: BTreeSet<String> = COMMAND_NAMES
        .iter()
        .map(|name| allow_permission(name))
        .collect();
    expected.insert("core:event:default".to_owned());
    assert_eq!(capabilities_permissions(), expected);
}

#[test]
fn capabilities_grant_no_plugin_or_window_permissions() {
    for permission in capabilities_permissions() {
        for forbidden in ["fs:", "shell:", "http:", "dialog:", "core:window:"] {
            assert!(
                !permission.starts_with(forbidden),
                "forbidden permission granted: {permission}"
            );
        }
    }
}

/// `tauri.conf.json` を読む(設定値の検査に使う)。
fn tauri_config() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let text = std::fs::read_to_string(&path).expect("tauri.conf.json is readable");
    serde_json::from_str(&text).expect("tauri.conf.json is JSON")
}

#[test]
fn csp_allows_only_the_app_origin_for_scripts_and_styles() {
    // 外部のスクリプト・スタイル・接続先を許さない(WebView に読み込むのは同梱の資産だけ)。
    let config = tauri_config();
    let csp = config["app"]["security"]["csp"]
        .as_str()
        .expect("app.security.csp is a string");
    let directives: std::collections::BTreeMap<&str, Vec<&str>> = csp
        .split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| {
            let mut parts = d.split_whitespace();
            let name = parts.next().expect("directive name");
            (name, parts.collect())
        })
        .collect();
    assert_eq!(
        directives.get("default-src"),
        Some(&vec!["'self'"]),
        "csp: {csp}"
    );
    assert_eq!(
        directives.get("script-src"),
        Some(&vec!["'self'"]),
        "csp: {csp}"
    );
    for (name, sources) in &directives {
        for source in sources {
            assert!(
                !source.contains("http") && *source != "*" && *source != "'unsafe-eval'",
                "csp directive {name} allows an external source or eval: {source}"
            );
        }
    }
}

#[test]
fn main_window_keeps_the_minimum_size_and_no_global_tauri() {
    let config = tauri_config();
    assert_eq!(config["app"]["withGlobalTauri"], serde_json::json!(false));
    let window = &config["app"]["windows"][0];
    assert_eq!(window["minWidth"], serde_json::json!(800));
    assert_eq!(window["minHeight"], serde_json::json!(600));
}

// ---- IPC 経由・コマンド層の検査(モックランタイム。ケイパビリティは検査しない) ----

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

#[test]
fn every_registered_command_is_reachable_over_ipc() {
    let app = mock_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock window builds");
    // 引数を欠いた要求は副作用なく拒否される。未登録のコマンドは「not found」になるため、
    // 登録の有無はその文面で区別できる。
    // 引数を取らないコマンドは空の要求でも受理される。そのうち `open_source_file`(ネイティブのダイアログを開く)と
    // `confirm_exit`(アプリを終了する)は呼ぶと副作用が起きるため、この検査からは外す。
    // `new_document` は副作用が無く成功するだけなので外す。登録は集合の一致テストで検査する。
    const NO_ARGS_COMMANDS: &[&str] = &["new_document", "open_source_file", "confirm_exit"];
    for name in REGISTERED_COMMANDS
        .iter()
        .filter(|name| !NO_ARGS_COMMANDS.contains(name))
    {
        let Err(error) = get_ipc_response(&webview, ipc_request(name, serde_json::json!({})))
        else {
            panic!("{name} accepted a request without args");
        };
        assert!(
            !error.to_string().to_lowercase().contains("not found"),
            "{name} is not registered: {error}"
        );
    }
    let Err(error) = get_ipc_response(
        &webview,
        ipc_request("__unregistered__", serde_json::json!({})),
    ) else {
        panic!("an unregistered command was accepted");
    };
    assert!(error.to_string().to_lowercase().contains("not found"));
}

#[test]
fn commands_take_their_request_inside_a_single_args_object() {
    let app = mock_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock window builds");
    let body = get_ipc_response(
        &webview,
        ipc_request(
            "load_source",
            serde_json::json!({ "args": { "source": "+." } }),
        ),
    )
    .expect("load_source succeeds");
    let value: serde_json::Value = body.deserialize().expect("JSON response");
    assert_eq!(value["kind"], "ok");
}

fn view() -> ViewRequestDto {
    ViewRequestDto {
        tape_radius: 0,
        output_from: 0,
    }
}

#[test]
fn pause_returns_while_a_batch_holds_the_session_lock() {
    let app = mock_app();
    let handle = app.handle().clone();
    let program_id = match handle
        .state::<AppState>()
        .service
        .load_source(LoadSourceRequest {
            source: "+[]".to_owned(),
        })
        .expect("source is accepted")
    {
        LoadSourceResult::Ok { program_id, .. } => program_id,
        other => panic!("expected a loaded program, got {other:?}"),
    };
    let session_id = handle
        .state::<AppState>()
        .service
        .start_session(StartSessionRequest {
            program_id,
            input_base64: String::new(),
            limit: StepLimitDto::Unlimited,
            breakpoint_offsets: Vec::new(),
            view: view(),
        })
        .expect("session starts")
        .state
        .session_id;

    // `+[]` は止まらないため、バッチは一時停止が届くまで続く。
    let run_handle = handle.clone();
    let run = std::thread::spawn(move || {
        tauri::async_runtime::block_on(brainfuck_studio::commands::debugger::run_batch(
            RunBatchRequest {
                session_id,
                batch_size: Some(MAX_BATCH_SIZE),
                view: view(),
            },
            run_handle.state(),
        ))
    });

    let mut pauses = 0_u32;
    while !run.is_finished() && pauses < 100_000 {
        let call = Instant::now();
        tauri::async_runtime::block_on(brainfuck_studio::commands::debugger::pause(
            PauseRequest { session_id },
            handle.state(),
        ))
        .expect("pause succeeds");
        assert!(
            call.elapsed() < Duration::from_millis(250),
            "pause waited for the batch: {:?}",
            call.elapsed()
        );
        pauses += 1;
    }
    let result: BatchResultDto = run
        .join()
        .expect("run thread finishes")
        .expect("batch succeeds");
    assert_eq!(result.stop, BatchStopDto::Cancelled);
}
