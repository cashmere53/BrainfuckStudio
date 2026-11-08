//! 共有の型(ブリッジの状態、ダイアログの問い合わせ口)と、コマンド名から呼び出しへの対応表(デバッガの 9 件と `e2e:ping`)。
//! ファイル操作の 5 件は 2.3 で対応を追加する。

use std::path::PathBuf;
use std::sync::Mutex;

use brainfuck_gui_api::dto::{ApiError, ApiErrorKind};
use brainfuck_gui_api::service::DebuggerService;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use crate::document::BridgeDocument;
use crate::protocol::DialogKind;

/// ブリッジの状態: デバッガのサービスと文書状態。要求ごとのスレッドから共有する。
#[derive(Default)]
pub struct BridgeState {
    pub service: DebuggerService,
    pub document: Mutex<BridgeDocument>,
}

/// ダイアログの問い合わせ口。ワーカースレッドから呼び、返事が届くまでブロックする。
/// 取消は `Ok(None)`、選ばれたパスは `Ok(Some(path))` になる。
pub trait DialogPort {
    fn pick(&self, kind: DialogKind) -> Result<Option<PathBuf>, ApiError>;
}

/// ブリッジが対応するコマンド名(制御コマンド `e2e:*` を除く)。基準は `names.rs` の `COMMAND_NAMES`。
/// 3.3 の契約テストが集合で比べる。ファイル操作の 5 件は 2.3 で対応を追加する(それまで `UnknownCommand`)。
pub const COMMANDS: &[&str] = &[
    "load_source",
    "start_session",
    "run_batch",
    "step",
    "reset",
    "get_snapshot",
    "get_tape_window",
    "set_breakpoints",
    "pause",
    "new_document",
    "open_source_file",
    "save_source_file",
    "set_document_dirty",
    "confirm_exit",
];

/// 対応表の失敗。`Api` は実サービスの構造化エラーで、それ以外は文字列のエラー(実アプリのフロントでも同じ扱い)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// サービスのエラー(`ApiError` のまま)。
    Api(ApiError),
    /// `args` が無い・形が違う・未知のフィールドがある。
    InvalidArgs(String),
    /// 対応しないコマンド名。
    UnknownCommand(String),
}

impl DispatchError {
    /// 応答の `error` に入れる値。`Api` は `ApiError` の JSON、それ以外は文字列。
    pub fn to_error_value(&self) -> Value {
        match self {
            Self::Api(error) => serde_json::to_value(error)
                .unwrap_or_else(|_| Value::String("failed to encode api error".to_string())),
            Self::InvalidArgs(message) | Self::UnknownCommand(message) => {
                Value::String(message.clone())
            }
        }
    }
}

/// コマンド名と `payload`(`{ "args": ... }`)を、Tauri 層と同じ順序のサービス呼び出しに写す。
/// 応答の値は DTO の `Serialize` をそのまま使う。`pause` は `null` を返す。
pub fn dispatch(
    state: &BridgeState,
    cmd: &str,
    payload: &Value,
    dialogs: &dyn DialogPort,
) -> Result<Value, DispatchError> {
    // ダイアログを使うコマンドは 2.3 で対応する。
    let _ = dialogs;
    let service = &state.service;
    match cmd {
        "e2e:ping" => Ok(json!("pong")),
        "load_source" => encode(service.load_source(args(cmd, payload)?)),
        "start_session" => encode(service.start_session(args(cmd, payload)?)),
        "run_batch" => encode(service.run_batch(args(cmd, payload)?)),
        "step" => encode(service.step(args(cmd, payload)?)),
        "reset" => encode(service.reset(args(cmd, payload)?)),
        "get_snapshot" => encode(service.get_snapshot(args(cmd, payload)?)),
        "get_tape_window" => encode(service.get_tape_window(args(cmd, payload)?)),
        "set_breakpoints" => encode(service.set_breakpoints(args(cmd, payload)?)),
        "pause" => {
            service.pause(args(cmd, payload)?);
            Ok(Value::Null)
        }
        _ => Err(DispatchError::UnknownCommand(format!(
            "unknown command: {cmd}"
        ))),
    }
}

/// `payload.args` を要求の型へ読み取る。キーが無い・形が違う・未知のフィールドは `InvalidArgs`。
fn args<T: DeserializeOwned>(cmd: &str, payload: &Value) -> Result<T, DispatchError> {
    let raw = payload
        .get("args")
        .ok_or_else(|| DispatchError::InvalidArgs(format!("{cmd}: missing `args`")))?;
    serde_json::from_value(raw.clone())
        .map_err(|error| DispatchError::InvalidArgs(format!("{cmd}: invalid `args`: {error}")))
}

/// サービスの応答を JSON の値にする。サービスのエラーは構造化のまま返す。
fn encode<T: Serialize>(result: Result<T, ApiError>) -> Result<Value, DispatchError> {
    let value = result.map_err(DispatchError::Api)?;
    serde_json::to_value(value)
        .map_err(|_| DispatchError::Api(ApiError::new(ApiErrorKind::Internal, None)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainfuck_gui_api::dto::ApiErrorKind;
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// 2.2 の範囲ではダイアログを使わない。呼ばれたら失敗にする。
    struct NoDialog;

    impl DialogPort for NoDialog {
        fn pick(&self, _kind: DialogKind) -> Result<Option<PathBuf>, ApiError> {
            panic!("unexpected dialog in a debugger-only command");
        }
    }

    const HELLO: &str = include_str!("../../../examples/hello.bf");

    fn view() -> Value {
        json!({"tapeRadius": 3, "outputFrom": 0})
    }

    fn call(state: &BridgeState, cmd: &str, args: Value) -> Result<Value, DispatchError> {
        dispatch(state, cmd, &json!({"args": args}), &NoDialog)
    }

    fn load(state: &BridgeState, source: &str) -> u64 {
        let value = call(state, "load_source", json!({"source": source})).expect("load_source");
        assert_eq!(value["kind"], json!("ok"));
        value["programId"].as_u64().expect("programId")
    }

    fn load_hello(state: &BridgeState) -> u64 {
        load(state, HELLO)
    }

    fn start_with(state: &BridgeState, program_id: u64, input_base64: &str, limit: Value) -> u64 {
        let value = call(
            state,
            "start_session",
            json!({
                "programId": program_id,
                "inputBase64": input_base64,
                "limit": limit,
                "breakpointOffsets": [],
                "view": view(),
            }),
        )
        .expect("start_session");
        value["state"]["sessionId"].as_u64().expect("sessionId")
    }

    fn start(state: &BridgeState, program_id: u64) -> u64 {
        start_with(state, program_id, "", json!({"kind": "unlimited"}))
    }

    #[test]
    fn commands_lists_the_fourteen_names_of_names_rs() {
        assert_eq!(COMMANDS.len(), 14);
        for name in [
            "load_source",
            "start_session",
            "run_batch",
            "step",
            "reset",
            "get_snapshot",
            "get_tape_window",
            "set_breakpoints",
            "pause",
            "new_document",
            "open_source_file",
            "save_source_file",
            "set_document_dirty",
            "confirm_exit",
        ] {
            assert!(COMMANDS.contains(&name), "missing {name}");
        }
    }

    #[test]
    fn hello_runs_to_pc_106_dp_6_and_906_steps() {
        let state = BridgeState::default();
        let program_id = load_hello(&state);
        let session_id = start(&state, program_id);

        let batch = call(
            &state,
            "run_batch",
            json!({"sessionId": session_id, "batchSize": null, "view": view()}),
        )
        .expect("run_batch");
        assert_eq!(batch["stop"]["kind"], json!("halted"));
        assert_eq!(batch["state"]["snapshot"]["pc"], json!(106));
        assert_eq!(batch["state"]["snapshot"]["dp"], json!(6));
        assert_eq!(batch["state"]["snapshot"]["stepsExecuted"], json!(906));
        assert_eq!(batch["state"]["snapshot"]["halted"], json!(true));
    }

    #[test]
    fn step_reset_snapshot_tape_and_breakpoints_are_dispatched() {
        let state = BridgeState::default();
        let program_id = load_hello(&state);
        let session_id = start(&state, program_id);

        let stepped = call(
            &state,
            "step",
            json!({"sessionId": session_id, "view": view()}),
        )
        .expect("step");
        assert_eq!(stepped["state"]["snapshot"]["stepsExecuted"], json!(1));

        let snapshot = call(
            &state,
            "get_snapshot",
            json!({"sessionId": session_id, "view": view()}),
        )
        .expect("get_snapshot");
        assert_eq!(snapshot["snapshot"]["stepsExecuted"], json!(1));

        let tape = call(
            &state,
            "get_tape_window",
            json!({"sessionId": session_id, "radius": 2}),
        )
        .expect("get_tape_window");
        assert!(tape["cells"].is_array());

        let breakpoints = call(
            &state,
            "set_breakpoints",
            json!({"sessionId": session_id, "offsets": []}),
        )
        .expect("set_breakpoints");
        assert!(breakpoints.is_object());

        let reset = call(
            &state,
            "reset",
            json!({"sessionId": session_id, "view": view()}),
        )
        .expect("reset");
        assert_eq!(reset["snapshot"]["stepsExecuted"], json!(0));
    }

    #[test]
    fn pause_returns_null_and_does_not_fail_for_unknown_session() {
        let state = BridgeState::default();
        let value = call(&state, "pause", json!({"sessionId": 999})).expect("pause");
        assert_eq!(value, Value::Null);
    }

    #[test]
    fn pause_from_another_thread_stops_a_running_batch() {
        let state = BridgeState::default();
        let program_id = load(&state, "+[]");
        let session_id = start(&state, program_id);
        let done = AtomicBool::new(false);

        // run_batch はサービスの Mutex を 1 バッチの間保持する。Pause はそれを取らずに届く必要がある。
        // 旗は run_batch の入口で下ろされるため、送るのは run_batch が走り始めてからでよい。
        // 完了まで繰り返し送り、どの送信も取りこぼさない。
        let batch = std::thread::scope(|scope| {
            let runner = scope.spawn(|| {
                let value = call(
                    &state,
                    "run_batch",
                    json!({"sessionId": session_id, "batchSize": 1_000_000, "view": view()}),
                );
                done.store(true, Ordering::Release);
                value
            });
            while !done.load(Ordering::Acquire) {
                let paused = call(&state, "pause", json!({"sessionId": session_id}));
                assert_eq!(paused, Ok(Value::Null));
                std::thread::sleep(Duration::from_millis(1));
            }
            runner.join().expect("run_batch thread")
        });
        let batch = batch.expect("run_batch");
        assert_eq!(batch["stop"]["kind"], json!("cancelled"));
        assert_eq!(batch["state"]["snapshot"]["halted"], json!(false));
    }

    #[test]
    fn set_breakpoints_passes_offsets_to_the_service() {
        let state = BridgeState::default();
        let program_id = load_hello(&state);
        let session_id = start(&state, program_id);

        // 8 は HELLO の `[` の位置(命令がある)。1_000_000 はソースの外で、未解決として返る。
        let value = call(
            &state,
            "set_breakpoints",
            json!({"sessionId": session_id, "offsets": [8, 1_000_000]}),
        )
        .expect("set_breakpoints");
        let breakpoints = value["breakpoints"].as_array().expect("breakpoints");
        assert_eq!(breakpoints.len(), 1);
        assert_eq!(breakpoints[0]["utf16Offset"], json!(8));
        assert_eq!(value["unresolved"], json!([1_000_000]));
    }

    #[test]
    fn get_tape_window_follows_the_requested_radius() {
        let state = BridgeState::default();
        // dp を 8 まで動かして、左端(添字 0)で窓が切られないようにする。
        let program_id = load(&state, ">>>>>>>>");
        let session_id = start(&state, program_id);
        call(
            &state,
            "run_batch",
            json!({"sessionId": session_id, "batchSize": null, "view": view()}),
        )
        .expect("run_batch");

        let zero = call(
            &state,
            "get_tape_window",
            json!({"sessionId": session_id, "radius": 0}),
        )
        .expect("radius 0");
        assert_eq!(zero["cells"].as_array().expect("cells").len(), 1);

        let two = call(
            &state,
            "get_tape_window",
            json!({"sessionId": session_id, "radius": 2}),
        )
        .expect("radius 2");
        assert_eq!(two["cells"].as_array().expect("cells").len(), 5);
    }

    #[test]
    fn input_base64_reaches_the_machine() {
        let state = BridgeState::default();
        // `,.,.` は入力 2 バイトを読んで順に出力する。"QUI=" は "AB"。
        let program_id = load(&state, ",.,.");
        let session_id = start_with(&state, program_id, "QUI=", json!({"kind": "unlimited"}));

        let batch = call(
            &state,
            "run_batch",
            json!({"sessionId": session_id, "batchSize": null, "view": view()}),
        )
        .expect("run_batch");
        assert_eq!(batch["stop"]["kind"], json!("halted"));
        assert_eq!(batch["state"]["inputTotal"], json!(2));
        assert_eq!(batch["state"]["inputConsumed"], json!(2));
        assert_eq!(batch["state"]["output"]["dataBase64"], json!("QUI="));
    }

    #[test]
    fn flat_payload_without_args_is_an_invalid_args_error() {
        let state = BridgeState::default();
        let program_id = load_hello(&state);
        let session_id = start(&state, program_id);

        // `args` で包まない payload は、中身が要求の形でも拒否する(フォールバックしない)。
        let error = dispatch(
            &state,
            "step",
            &json!({"sessionId": session_id, "view": view()}),
            &NoDialog,
        )
        .expect_err("flat payload");
        assert!(matches!(error, DispatchError::InvalidArgs(_)));
        assert!(error.to_error_value().is_string());
    }

    #[test]
    fn unknown_fields_in_step_and_run_batch_are_invalid_args() {
        let state = BridgeState::default();
        let program_id = load_hello(&state);
        let session_id = start(&state, program_id);

        let step = call(
            &state,
            "step",
            json!({"sessionId": session_id, "view": view(), "extra": true}),
        )
        .expect_err("step with unknown field");
        assert!(matches!(step, DispatchError::InvalidArgs(_)));
        assert!(step.to_error_value().is_string());

        let batch = call(
            &state,
            "run_batch",
            json!({"sessionId": session_id, "batchSize": null, "view": view(), "extra": 1}),
        )
        .expect_err("run_batch with unknown field");
        assert!(matches!(batch, DispatchError::InvalidArgs(_)));
        assert!(batch.to_error_value().is_string());

        // 拒否された要求は機械を進めない。
        let snapshot = call(
            &state,
            "get_snapshot",
            json!({"sessionId": session_id, "view": view()}),
        )
        .expect("get_snapshot");
        assert_eq!(snapshot["snapshot"]["stepsExecuted"], json!(0));
    }

    #[test]
    fn e2e_ping_answers_pong_for_startup_check() {
        let state = BridgeState::default();
        let value = dispatch(&state, "e2e:ping", &json!({}), &NoDialog).expect("ping");
        assert_eq!(value, json!("pong"));
    }

    #[test]
    fn missing_args_key_is_a_string_error() {
        let state = BridgeState::default();
        let error = dispatch(&state, "step", &json!({}), &NoDialog).expect_err("missing args");
        assert!(matches!(error, DispatchError::InvalidArgs(_)));
        assert!(error.to_error_value().is_string());
    }

    #[test]
    fn wrong_shape_and_unknown_fields_are_string_errors() {
        let state = BridgeState::default();
        let wrong_type = call(&state, "pause", json!({"sessionId": "1"})).expect_err("type");
        assert!(matches!(wrong_type, DispatchError::InvalidArgs(_)));

        let unknown_field =
            call(&state, "pause", json!({"sessionId": 1, "extra": true})).expect_err("unknown");
        assert!(matches!(unknown_field, DispatchError::InvalidArgs(_)));
        assert!(unknown_field.to_error_value().is_string());
    }

    #[test]
    fn unknown_command_is_a_string_error() {
        let state = BridgeState::default();
        let error = call(&state, "no_such_command", json!({})).expect_err("unknown");
        assert!(matches!(error, DispatchError::UnknownCommand(_)));
        assert!(error.to_error_value().is_string());
    }

    #[test]
    fn service_errors_stay_structured() {
        let state = BridgeState::default();
        let error = call(
            &state,
            "run_batch",
            json!({"sessionId": 42, "batchSize": null, "view": view()}),
        )
        .expect_err("session not found");
        let DispatchError::Api(api) = &error else {
            panic!("expected a structured api error, got {error:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::SessionNotFound);
        assert_eq!(
            error.to_error_value(),
            json!({"kind": "sessionNotFound", "message": api.message, "field": null})
        );
    }
}
