//! 共有の型(ブリッジの状態、ダイアログの問い合わせ口)と、コマンド名から呼び出しへの対応表
//! (デバッガの 9 件、文書の 5 件、制御コマンド `e2e:ping` と `e2e:close_requested`)。
//!
//! 文書の 5 件は `apps/desktop/src-tauri/src/commands/files.rs` と同じ順序で呼ぶ。
//! 複製元が変わったら `apps/desktop/e2e/support/bridge/tauriFingerprint.test.ts` の指紋が失敗する。

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, OpenFileResult, SaveFileRequest, SaveFileResult,
    SetDocumentDirtyRequest,
};
use brainfuck_gui_api::service::DebuggerService;
use brainfuck_gui_api::source_file::{display_name, encode_source};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use crate::document::{read_source, write_bytes, BridgeDocument};
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
/// 3.3 の契約テストが集合で比べる。実行時の分岐には使わない(`dispatch` の `match` が正)ため、本体では未使用になる。
#[allow(dead_code)]
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
/// 引数の無いコマンド(`new_document`・`open_source_file`・`confirm_exit`・`e2e:*`)は `payload` を読まない。
pub fn dispatch(
    state: &BridgeState,
    cmd: &str,
    payload: &Value,
    dialogs: &dyn DialogPort,
) -> Result<Value, DispatchError> {
    let service = &state.service;
    match cmd {
        "e2e:ping" => Ok(json!("pong")),
        "e2e:close_requested" => close_requested(state),
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
        "new_document" => {
            document(state)?.reset_to_new();
            Ok(Value::Null)
        }
        "open_source_file" => open_source_file(state, dialogs),
        "save_source_file" => save_source_file(state, payload, dialogs),
        "set_document_dirty" => {
            let request: SetDocumentDirtyRequest = args(cmd, payload)?;
            document(state)?.set_dirty(request.dirty);
            Ok(Value::Null)
        }
        // 終了は行わない。ブリッジは何も記録せず、中継側が呼び出しを数える。
        "confirm_exit" => Ok(Value::Null),
        _ => Err(DispatchError::UnknownCommand(format!(
            "unknown command: {cmd}"
        ))),
    }
}

/// 文書状態のロック。汚染された Mutex は Tauri 層の `document()` と同じく `internal` にする。
fn document(state: &BridgeState) -> Result<MutexGuard<'_, BridgeDocument>, DispatchError> {
    state
        .document
        .lock()
        .map_err(|_| DispatchError::Api(ApiError::new(ApiErrorKind::Internal, None)))
}

/// ダイアログの問い合わせ。取消は `None`。
fn pick(dialogs: &dyn DialogPort, kind: DialogKind) -> Result<Option<PathBuf>, DispatchError> {
    dialogs.pick(kind).map_err(DispatchError::Api)
}

/// 開く(`files.rs::open_source_file` と同じ順序: ダイアログ → 読み込み → 開いた)。
/// 取消は文書を変えずに `cancelled` を返す。
fn open_source_file(state: &BridgeState, dialogs: &dyn DialogPort) -> Result<Value, DispatchError> {
    let Some(path) = pick(dialogs, DialogKind::Open)? else {
        return encode(Ok(OpenFileResult::Cancelled));
    };
    let decoded = read_source(&path).map_err(DispatchError::Api)?;
    let file_name = display_name(&path);
    document(state)?.opened(path, decoded.line_ending);
    encode(Ok(OpenFileResult::Opened {
        source: decoded.text,
        file_name,
        lossy: decoded.lossy,
    }))
}

/// 保存(`files.rs::save_source_file` と同じ順序: 状態の読み取り → 変換の検査 → 保存先の
/// 問い合わせ(紐づけ先が無いか別名のときだけ)→ 書き込み → 保存した)。
/// 変換できない本文は、保存先を尋ねる前に上限超過で返す。取消は書き込まず文書を変えない。
fn save_source_file(
    state: &BridgeState,
    payload: &Value,
    dialogs: &dyn DialogPort,
) -> Result<Value, DispatchError> {
    let request: SaveFileRequest = args("save_source_file", payload)?;
    let (current, line_ending) = {
        let guard = document(state)?;
        (guard.path.clone(), guard.line_ending)
    };
    let bytes = encode_source(&request.source, line_ending).map_err(DispatchError::Api)?;
    let path = match current {
        Some(path) if !request.save_as => path,
        _ => match pick(dialogs, DialogKind::Save)? {
            Some(path) => path,
            None => return encode(Ok(SaveFileResult::Cancelled)),
        },
    };
    write_bytes(&path, &bytes).map_err(DispatchError::Api)?;
    let file_name = display_name(&path);
    document(state)?.saved(path);
    encode(Ok(SaveFileResult::Saved { file_name }))
}

/// 閉じる要求の判定(`window.rs::close_requires_confirmation` と同じ規則)。
/// 汚染された Mutex でも、記録された旗の値で判定する。
fn close_requested(state: &BridgeState) -> Result<Value, DispatchError> {
    let dirty = match state.document.lock() {
        Ok(guard) => guard.dirty,
        Err(poisoned) => poisoned.into_inner().dirty,
    };
    Ok(json!({"confirmationRequired": dirty}))
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
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use brainfuck_gui_api::limits::MAX_SOURCE_BYTES;
    use brainfuck_gui_api::source_file::LineEnding;

    /// ダイアログを使わないコマンドで使う。呼ばれたら失敗にする。
    struct NoDialog;

    impl DialogPort for NoDialog {
        fn pick(&self, _kind: DialogKind) -> Result<Option<PathBuf>, ApiError> {
            panic!("unexpected dialog");
        }
    }

    /// 問い合わせの種類を記録し、用意した答え(`None` は取消)を返すダイアログ。
    struct RecordingDialog {
        answer: Option<PathBuf>,
        asked: RefCell<Vec<DialogKind>>,
    }

    impl RecordingDialog {
        fn answering(answer: Option<PathBuf>) -> Self {
            Self {
                answer,
                asked: RefCell::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<DialogKind> {
            self.asked.borrow().clone()
        }
    }

    impl DialogPort for RecordingDialog {
        fn pick(&self, kind: DialogKind) -> Result<Option<PathBuf>, ApiError> {
            self.asked.borrow_mut().push(kind);
            Ok(self.answer.clone())
        }
    }

    fn document_of(state: &BridgeState) -> BridgeDocument {
        state.document.lock().expect("document lock").clone()
    }

    fn run_with_args(
        state: &BridgeState,
        dialogs: &dyn DialogPort,
        cmd: &str,
        args: Value,
    ) -> Result<Value, DispatchError> {
        dispatch(state, cmd, &json!({"args": args}), dialogs)
    }

    fn save_args(source: &str, save_as: bool) -> Value {
        json!({"source": source, "saveAs": save_as})
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
    fn comparison_commands_are_excluded_and_rejected_as_unknown() {
        // 比較の 2 件はブラウザ E2E の除外一覧(e2e/support/bridge/commandContract.test.ts)にある。
        // 呼ばれても未対応として文字列エラーで拒否する。
        let state = BridgeState::default();
        for name in ["get_comparison_info", "run_comparison"] {
            let error = call(&state, name, json!({"args": null})).expect_err("excluded");
            assert!(matches!(error, DispatchError::UnknownCommand(_)), "{name}");
            assert!(error.to_error_value().is_string());
        }
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

    #[test]
    fn open_source_file_reads_the_picked_file_and_records_the_document() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("crlf.bf");
        std::fs::write(&path, b"+>\r\n-<\r\n").expect("write");
        let state = BridgeState::default();
        let dialog = RecordingDialog::answering(Some(path.clone()));

        let value = dispatch(&state, "open_source_file", &json!({}), &dialog).expect("open");
        assert_eq!(
            value,
            json!({"kind": "opened", "source": "+>\n-<\n", "fileName": "crlf.bf", "lossy": false})
        );
        assert_eq!(dialog.asked(), vec![DialogKind::Open]);
        let document = document_of(&state);
        assert_eq!(document.path, Some(path));
        assert_eq!(document.line_ending, LineEnding::CrLf);
        assert!(!document.dirty);
    }

    #[test]
    fn crlf_file_opened_and_saved_keeps_crlf_and_asks_only_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("round.bf");
        std::fs::write(&path, b"+>\r\n-<\r\n").expect("write");
        let state = BridgeState::default();
        let dialog = RecordingDialog::answering(Some(path.clone()));

        dispatch(&state, "open_source_file", &json!({}), &dialog).expect("open");
        let value = run_with_args(
            &state,
            &dialog,
            "save_source_file",
            save_args("+>\n-<\n+\n", false),
        )
        .expect("save");
        assert_eq!(value, json!({"kind": "saved", "fileName": "round.bf"}));
        assert_eq!(std::fs::read(&path).expect("read"), b"+>\r\n-<\r\n+\r\n");
        assert_eq!(dialog.asked(), vec![DialogKind::Open]);
    }

    #[test]
    fn open_source_file_cancel_leaves_the_document_unchanged() {
        let state = BridgeState::default();
        {
            let mut document = state.document.lock().expect("document lock");
            document.opened(PathBuf::from("keep.bf"), LineEnding::CrLf);
            document.set_dirty(true);
        }
        let before = document_of(&state);
        let dialog = RecordingDialog::answering(None);

        let value = dispatch(&state, "open_source_file", &json!({}), &dialog).expect("open");
        assert_eq!(value, json!({"kind": "cancelled"}));
        assert_eq!(dialog.asked(), vec![DialogKind::Open]);
        assert_eq!(document_of(&state), before);
    }

    #[test]
    fn open_source_file_rejects_an_oversized_file_without_changing_the_document() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("big.bf");
        std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES + 1]).expect("write");
        let state = BridgeState::default();
        let before = document_of(&state);
        let dialog = RecordingDialog::answering(Some(path));

        let error = dispatch(&state, "open_source_file", &json!({}), &dialog)
            .expect_err("oversized file must be rejected");
        let DispatchError::Api(api) = &error else {
            panic!("expected a structured api error, got {error:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::FileTooLarge);
        assert_eq!(document_of(&state), before);
    }

    #[test]
    fn save_with_a_bound_path_writes_the_crlf_body_without_asking() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bound.bf");
        std::fs::write(&path, b"old\r\n").expect("write");
        let state = BridgeState::default();
        state
            .document
            .lock()
            .expect("document lock")
            .opened(path.clone(), LineEnding::CrLf);
        let dialog = RecordingDialog::answering(None);

        let value = run_with_args(
            &state,
            &dialog,
            "save_source_file",
            save_args("+>\n-<\n", false),
        )
        .expect("save");
        assert_eq!(value, json!({"kind": "saved", "fileName": "bound.bf"}));
        assert_eq!(std::fs::read(&path).expect("read"), b"+>\r\n-<\r\n");
        assert!(dialog.asked().is_empty(), "a bound save must not ask");
        let document = document_of(&state);
        assert_eq!(document.path, Some(path));
        assert_eq!(document.line_ending, LineEnding::CrLf);
        assert!(!document.dirty);
    }

    #[test]
    fn save_as_asks_even_when_a_path_is_bound_and_rebinds_to_the_new_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = dir.path().join("a.bf");
        let second = dir.path().join("b.bf");
        std::fs::write(&first, b"keep").expect("write");
        let state = BridgeState::default();
        state
            .document
            .lock()
            .expect("document lock")
            .opened(first.clone(), LineEnding::Lf);
        let dialog = RecordingDialog::answering(Some(second.clone()));

        let value = run_with_args(&state, &dialog, "save_source_file", save_args("+", true))
            .expect("save as");
        assert_eq!(value, json!({"kind": "saved", "fileName": "b.bf"}));
        assert_eq!(dialog.asked(), vec![DialogKind::Save]);
        assert_eq!(std::fs::read(&first).expect("read a"), b"keep");
        assert_eq!(std::fs::read(&second).expect("read b"), b"+");
        assert_eq!(document_of(&state).path, Some(second));
    }

    #[test]
    fn save_without_a_path_asks_for_a_save_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("new.bf");
        let state = BridgeState::default();
        let dialog = RecordingDialog::answering(Some(path.clone()));

        let value = run_with_args(&state, &dialog, "save_source_file", save_args("+-", false))
            .expect("save");
        assert_eq!(value, json!({"kind": "saved", "fileName": "new.bf"}));
        assert_eq!(dialog.asked(), vec![DialogKind::Save]);
        assert_eq!(std::fs::read(&path).expect("read"), b"+-");
        assert_eq!(document_of(&state).path, Some(path));
    }

    #[test]
    fn save_cancel_writes_nothing_and_keeps_the_document() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bound = dir.path().join("bound.bf");
        std::fs::write(&bound, b"keep").expect("write");
        let state = BridgeState::default();
        state
            .document
            .lock()
            .expect("document lock")
            .opened(bound.clone(), LineEnding::Lf);
        state
            .document
            .lock()
            .expect("document lock")
            .set_dirty(true);
        let before = document_of(&state);
        let dialog = RecordingDialog::answering(None);

        let value = run_with_args(&state, &dialog, "save_source_file", save_args("+", true))
            .expect("cancelled save");
        assert_eq!(value, json!({"kind": "cancelled"}));
        assert_eq!(dialog.asked(), vec![DialogKind::Save]);
        assert_eq!(std::fs::read(&bound).expect("read"), b"keep");
        assert_eq!(document_of(&state), before);

        let unbound = BridgeState::default();
        let no_path = RecordingDialog::answering(None);
        let value = run_with_args(
            &unbound,
            &no_path,
            "save_source_file",
            save_args("+", false),
        )
        .expect("cancelled save without path");
        assert_eq!(value, json!({"kind": "cancelled"}));
        assert_eq!(document_of(&unbound), BridgeDocument::default());
    }

    #[test]
    fn save_refuses_a_source_over_one_mib_before_asking() {
        let state = BridgeState::default();
        let dialog = RecordingDialog::answering(Some(PathBuf::from("never.bf")));
        let source = "a".repeat(MAX_SOURCE_BYTES + 1);

        let error = run_with_args(
            &state,
            &dialog,
            "save_source_file",
            save_args(&source, false),
        )
        .expect_err("oversized source");
        let DispatchError::Api(api) = &error else {
            panic!("expected a structured api error, got {error:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::SourceTooLarge);
        assert!(dialog.asked().is_empty(), "must not ask before encoding");
        assert_eq!(document_of(&state), BridgeDocument::default());
    }

    #[test]
    fn save_refuses_a_crlf_expansion_over_one_mib_before_asking() {
        let state = BridgeState::default();
        state.document.lock().expect("document lock").line_ending = LineEnding::CrLf;
        let dialog = RecordingDialog::answering(Some(PathBuf::from("never.bf")));
        // 本文は 600_000 バイト(上限内)だが、CRLF に戻すと 1_200_000 バイトになり上限を超える。
        let source = "\n".repeat(600_000);

        let error = run_with_args(
            &state,
            &dialog,
            "save_source_file",
            save_args(&source, false),
        )
        .expect_err("expanded source");
        let DispatchError::Api(api) = &error else {
            panic!("expected a structured api error, got {error:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::SourceTooLarge);
        assert!(dialog.asked().is_empty(), "must not ask before encoding");
    }

    #[test]
    fn save_into_a_missing_directory_is_an_io_write_error_without_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("no-such-dir").join("out.bf");
        let state = BridgeState::default();
        let dialog = RecordingDialog::answering(Some(target));

        let error = run_with_args(&state, &dialog, "save_source_file", save_args("+", false))
            .expect_err("write must fail");
        let DispatchError::Api(api) = &error else {
            panic!("expected a structured api error, got {error:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::Io);
        assert!(api.message.starts_with("failed to write file: "));
        assert!(!api.message.contains("out.bf"));
        assert!(!api.message.contains("no-such-dir"));
        assert_eq!(document_of(&state), BridgeDocument::default());
    }

    #[test]
    fn set_document_dirty_records_the_flag() {
        let state = BridgeState::default();
        let dialog = RecordingDialog::answering(None);

        let value = run_with_args(
            &state,
            &dialog,
            "set_document_dirty",
            json!({"dirty": true}),
        )
        .expect("set dirty");
        assert_eq!(value, Value::Null);
        assert!(document_of(&state).dirty);

        run_with_args(
            &state,
            &dialog,
            "set_document_dirty",
            json!({"dirty": false}),
        )
        .expect("clear dirty");
        assert!(!document_of(&state).dirty);
    }

    #[test]
    fn file_commands_reject_flat_payloads_and_unknown_fields_without_asking() {
        let state = BridgeState::default();
        let dialog = RecordingDialog::answering(Some(PathBuf::from("never.bf")));

        let flat = dispatch(
            &state,
            "save_source_file",
            &json!({"source": "+", "saveAs": false}),
            &dialog,
        )
        .expect_err("flat payload");
        assert!(matches!(flat, DispatchError::InvalidArgs(_)));
        assert!(flat.to_error_value().is_string());

        let unknown = run_with_args(
            &state,
            &dialog,
            "save_source_file",
            json!({"source": "+", "saveAs": false, "path": "x"}),
        )
        .expect_err("unknown field");
        assert!(matches!(unknown, DispatchError::InvalidArgs(_)));

        let dirty = run_with_args(
            &state,
            &dialog,
            "set_document_dirty",
            json!({"dirty": true, "extra": 1}),
        )
        .expect_err("unknown dirty field");
        assert!(matches!(dirty, DispatchError::InvalidArgs(_)));

        assert!(dialog.asked().is_empty());
        assert!(!document_of(&state).dirty);
    }

    #[test]
    fn new_document_resets_path_line_ending_and_dirty() {
        let state = BridgeState::default();
        {
            let mut document = state.document.lock().expect("document lock");
            document.opened(PathBuf::from("a.bf"), LineEnding::CrLf);
            document.set_dirty(true);
        }

        let value = dispatch(&state, "new_document", &json!({}), &NoDialog).expect("new");
        assert_eq!(value, Value::Null);
        assert_eq!(document_of(&state), BridgeDocument::default());
    }

    #[test]
    fn confirm_exit_answers_null_and_changes_nothing() {
        let state = BridgeState::default();
        state
            .document
            .lock()
            .expect("document lock")
            .set_dirty(true);
        let before = document_of(&state);

        let value = dispatch(&state, "confirm_exit", &json!({}), &NoDialog).expect("confirm");
        assert_eq!(value, Value::Null);
        assert_eq!(document_of(&state), before);
    }

    #[test]
    fn close_requested_reports_the_recorded_dirty_flag() {
        let state = BridgeState::default();
        let clean = dispatch(&state, "e2e:close_requested", &json!({}), &NoDialog).expect("close");
        assert_eq!(clean, json!({"confirmationRequired": false}));

        state
            .document
            .lock()
            .expect("document lock")
            .set_dirty(true);
        let dirty = dispatch(&state, "e2e:close_requested", &json!({}), &NoDialog).expect("close");
        assert_eq!(dirty, json!({"confirmationRequired": true}));
    }

    #[test]
    fn close_requested_uses_the_flag_even_when_the_document_lock_is_poisoned() {
        let state = BridgeState::default();
        state
            .document
            .lock()
            .expect("document lock")
            .set_dirty(true);
        std::thread::scope(|scope| {
            let poisoner = scope.spawn(|| {
                let _guard = state.document.lock().expect("document lock");
                panic!("poison the document lock");
            });
            assert!(poisoner.join().is_err());
        });
        assert!(state.document.is_poisoned());

        let value = dispatch(&state, "e2e:close_requested", &json!({}), &NoDialog).expect("close");
        assert_eq!(value, json!({"confirmationRequired": true}));
    }

    /// 常に失敗するダイアログ。問い合わせの失敗が取消と区別されることを確かめる。
    struct FailingDialog {
        error: ApiError,
    }

    impl DialogPort for FailingDialog {
        fn pick(&self, _kind: DialogKind) -> Result<Option<PathBuf>, ApiError> {
            Err(self.error.clone())
        }
    }

    /// 文書状態のロックを汚染する(保持したまま別スレッドで panic させる)。
    fn poison_document(state: &BridgeState) {
        std::thread::scope(|scope| {
            let poisoner = scope.spawn(|| {
                let _guard = state.document.lock().expect("document lock");
                panic!("poison the document lock");
            });
            assert!(poisoner.join().is_err());
        });
        assert!(state.document.is_poisoned());
    }

    #[test]
    fn open_source_file_reports_lossy_when_the_file_is_not_utf8() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sjis.bf");
        // Shift_JIS の「あ」(0x82 0xA0)は UTF-8 として不正
        std::fs::write(&path, [b'+', 0x82, 0xA0, b'-']).expect("write");
        let state = BridgeState::default();
        let dialog = RecordingDialog::answering(Some(path));

        let value = dispatch(&state, "open_source_file", &json!({}), &dialog).expect("open");
        assert_eq!(value["kind"], json!("opened"));
        assert_eq!(value["lossy"], json!(true));
        assert!(value["source"]
            .as_str()
            .expect("source")
            .contains('\u{FFFD}'));
    }

    /// 上限超過は中身を読む前に拒否される(開く経路も `read_source` の事前検査を通ること)。
    #[cfg(windows)]
    #[test]
    fn open_source_file_rejects_an_oversized_file_without_reading_it() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("locked-big.bf");
        std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES + 1]).expect("write");
        // 共有モード 0(排他)で開いたままにする。メタデータ取得は成功し、読み込みは共有違反で失敗する。
        let _locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .expect("lock");
        let state = BridgeState::default();
        let before = document_of(&state);
        let dialog = RecordingDialog::answering(Some(path));

        let error = dispatch(&state, "open_source_file", &json!({}), &dialog)
            .expect_err("oversized file must be rejected before reading");
        let DispatchError::Api(api) = &error else {
            panic!("expected a structured api error, got {error:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::FileTooLarge);
        assert_eq!(document_of(&state), before);
    }

    /// 上限超過は中身を読む前に拒否される(開く経路も `read_source` の事前検査を通ること)。
    ///
    /// 読み取り権限を外し、`read` まで進んだ場合は I/O エラーになって区別できる。
    /// root など権限を無視できる環境では読めてしまうので、そのときは検証せずに終える。
    #[cfg(unix)]
    #[test]
    fn open_source_file_rejects_an_oversized_file_without_reading_it() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("locked-big.bf");
        std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES + 1]).expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if std::fs::read(&path).is_ok() {
            // 権限が効かない環境(root など)では区別できないため、検証を省く。
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("restore");
            return;
        }
        let state = BridgeState::default();
        let before = document_of(&state);
        let dialog = RecordingDialog::answering(Some(path));

        let error = dispatch(&state, "open_source_file", &json!({}), &dialog)
            .expect_err("oversized file must be rejected before reading");
        let DispatchError::Api(api) = &error else {
            panic!("expected a structured api error, got {error:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::FileTooLarge);
        assert_eq!(document_of(&state), before);
    }

    #[test]
    fn open_source_file_of_a_missing_path_is_an_io_error_not_a_cancel() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing.bf");
        let state = BridgeState::default();
        let before = document_of(&state);
        let dialog = RecordingDialog::answering(Some(path));

        let error = dispatch(&state, "open_source_file", &json!({}), &dialog)
            .expect_err("a missing file must fail, not cancel");
        let DispatchError::Api(api) = &error else {
            panic!("expected a structured api error, got {error:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::Io);
        assert_eq!(document_of(&state), before);
    }

    #[test]
    fn dialog_failure_is_returned_as_is_for_open_and_save_not_as_cancel() {
        let state = BridgeState::default();
        let before = document_of(&state);
        let failure = ApiError::new(ApiErrorKind::Internal, None);
        let dialog = FailingDialog {
            error: failure.clone(),
        };

        let open = dispatch(&state, "open_source_file", &json!({}), &dialog)
            .expect_err("a dialog failure must not become a cancel on open");
        assert_eq!(open, DispatchError::Api(failure.clone()));

        // 保存先を尋ねる経路(紐づけ先が無い)でも同じこと。
        let save = run_with_args(&state, &dialog, "save_source_file", save_args("+", false))
            .expect_err("a dialog failure must not become a cancel on save");
        assert_eq!(save, DispatchError::Api(failure));
        assert_eq!(document_of(&state), before);
    }

    #[test]
    fn document_commands_report_internal_when_the_document_lock_is_poisoned() {
        let state = BridgeState::default();
        poison_document(&state);

        let new_document = dispatch(&state, "new_document", &json!({}), &NoDialog)
            .expect_err("new_document on a poisoned lock");
        let DispatchError::Api(api) = &new_document else {
            panic!("expected a structured api error, got {new_document:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::Internal);

        let dirty = run_with_args(
            &state,
            &NoDialog,
            "set_document_dirty",
            json!({"dirty": true}),
        )
        .expect_err("set_document_dirty on a poisoned lock");
        let DispatchError::Api(api) = &dirty else {
            panic!("expected a structured api error, got {dirty:?}");
        };
        assert_eq!(api.kind, ApiErrorKind::Internal);
    }
}
