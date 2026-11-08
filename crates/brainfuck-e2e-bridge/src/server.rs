//! 受信ループ、要求ごとのスレッド、ダイアログの返事の振り分け、行単位の書き込み。
//! 入力の終わり(EOF)で受信ループは戻る。実行中の要求のスレッドは待たない(プロセスの終了で止まる)。

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use brainfuck_gui_api::dto::{ApiError, ApiErrorKind};

use crate::dispatch::{dispatch, BridgeState, DialogPort};
use crate::protocol::{parse_inbound, write_line, DialogKind, DialogResult, Inbound, Outbound};

/// ダイアログの返事の振り分け表。問い合わせの `id`(要求の `id`)ごとに返事の送信口を持つ。
/// 入力が閉じた後は新しい問い合わせを受け付けず、保留中の送信口をすべて捨てて待ちを解放する。
#[derive(Default)]
struct DialogRouter {
    waiting: Mutex<Waiting>,
}

#[derive(Default)]
struct Waiting {
    closed: bool,
    senders: HashMap<u64, Sender<DialogResult>>,
}

impl DialogRouter {
    fn state(&self) -> std::sync::MutexGuard<'_, Waiting> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 返事を待つ口を登録する。入力が閉じていれば `None`。
    fn register(&self, id: u64) -> Option<Receiver<DialogResult>> {
        let mut waiting = self.state();
        if waiting.closed {
            return None;
        }
        let (sender, receiver) = mpsc::channel();
        waiting.senders.insert(id, sender);
        Some(receiver)
    }

    /// 返事を対応する `id` の待ちへ渡す。待ちが無ければ `false`。
    fn deliver(&self, id: u64, result: DialogResult) -> bool {
        let sender = self.state().senders.remove(&id);
        match sender {
            Some(sender) => {
                // 待ち手が先に抜けていても(送信口の受信側が無い)問題は無い。
                let _ = sender.send(result);
                true
            }
            None => false,
        }
    }

    /// 登録を取り消す(書き込みに失敗したとき)。
    fn forget(&self, id: u64) {
        self.state().senders.remove(&id);
    }

    /// 入力が閉じた。保留中の送信口を捨て、以後の登録を拒否する。
    fn close(&self) {
        let mut waiting = self.state();
        waiting.closed = true;
        waiting.senders.clear();
    }
}

/// 1 つの要求のダイアログの問い合わせ口。問い合わせの `id` は要求の `id` を使う。
struct RequestDialogs<'a, W> {
    id: u64,
    output: &'a Mutex<W>,
    router: &'a DialogRouter,
}

impl<W: Write> DialogPort for RequestDialogs<'_, W> {
    fn pick(&self, kind: DialogKind) -> Result<Option<PathBuf>, ApiError> {
        let Some(reply) = self.router.register(self.id) else {
            return Err(internal());
        };
        if send(self.output, &Outbound::Dialog { id: self.id, kind }).is_err() {
            self.router.forget(self.id);
            return Err(internal());
        }
        match reply.recv() {
            Ok(DialogResult::Path { path }) => Ok(Some(PathBuf::from(path))),
            Ok(DialogResult::Cancel) => Ok(None),
            // 送信口が捨てられた(入力が閉じた)。返事は来ない。
            Err(_) => Err(internal()),
        }
    }
}

fn internal() -> ApiError {
    ApiError::new(ApiErrorKind::Internal, None)
}

/// 1 メッセージを 1 行として、出力をロックして書く(複数スレッドの行が混ざらない)。
fn send<W: Write>(output: &Mutex<W>, message: &Outbound) -> io::Result<()> {
    let mut guard = output.lock().unwrap_or_else(PoisonError::into_inner);
    write_line(&mut *guard, message)
}

/// 入力が終わる(EOF)まで受信する。要求ごとにスレッドを立て、応答は `output` を行単位でロックして書く。
/// UTF-8 として読めない行や解釈できない行は標準エラーへ記録して読み飛ばす。
/// それ以外の入力の失敗は標準エラーへ記録してから受信ループを抜ける。
/// 返事の `id` に待ちが無ければ記録して捨てる。
pub fn serve<R: BufRead, W: Write + Send + 'static>(
    mut input: R,
    output: Arc<Mutex<W>>,
    state: Arc<BridgeState>,
) {
    let router = Arc::new(DialogRouter::default());
    let mut raw = Vec::new();
    loop {
        raw.clear();
        match input.read_until(b'\n', &mut raw) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                eprintln!("brainfuck-e2e-bridge: stopped reading input: {error}");
                break;
            }
        }
        let line = match std::str::from_utf8(&raw) {
            Ok(text) => text.trim_end_matches(['\r', '\n']),
            Err(error) => {
                eprintln!("brainfuck-e2e-bridge: skipped an input line that is not UTF-8: {error}");
                continue;
            }
        };
        match parse_inbound(line) {
            Ok(Inbound::Request { id, cmd, payload }) => {
                let output = Arc::clone(&output);
                let state = Arc::clone(&state);
                let router = Arc::clone(&router);
                thread::spawn(move || {
                    let dialogs = RequestDialogs {
                        id,
                        output: &output,
                        router: &router,
                    };
                    let message = match dispatch(&state, &cmd, &payload, &dialogs) {
                        Ok(value) => Outbound::ok(id, value),
                        Err(error) => Outbound::err(id, error.to_error_value()),
                    };
                    if let Err(error) = send(&output, &message) {
                        eprintln!("brainfuck-e2e-bridge: failed to write response {id}: {error}");
                    }
                });
            }
            Ok(Inbound::DialogResult { id, result }) => {
                if !router.deliver(id, result) {
                    eprintln!("brainfuck-e2e-bridge: dropped a dialog result with no pending dialog (id {id})");
                }
            }
            Err(error) => {
                eprintln!("brainfuck-e2e-bridge: skipped an unreadable input line: {error}");
            }
        }
    }
    router.close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Read};
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    use serde_json::{json, Value};

    use crate::dispatch::COMMANDS;

    /// テストの出力先。要求のスレッドが書いた行を、行の単位で読み返す。
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    /// 1 回の書き込みで受け付けるバイト数の上限。行を複数回の書き込みに分け、その間に他スレッドが割り込めるようにする。
    const CHUNK: usize = 16;

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            // 部分書き込みは Write の正当な振る舞い。途中で譲り、ロックを持たない他スレッドに機会を与える。
            let n = buf.len().min(CHUNK);
            thread::yield_now();
            self.0
                .lock()
                .expect("captured lock")
                .extend_from_slice(&buf[..n]);
            thread::yield_now();
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        /// これまでの出力を行ごとに JSON として読む。JSON でない完結した行があれば失敗にする(標準出力の純粋さ)。
        /// 書き込みの途中(改行の無い末尾)は、まだ書かれている最中なので読まない。
        fn lines(&self) -> Vec<Value> {
            let bytes = self.0.lock().expect("captured lock").clone();
            // 改行までを読む(書きかけの文字は途中で切れうるので、デコードの前に切る)。
            let complete = match bytes.iter().rposition(|&b| b == b'\n') {
                Some(last) => &bytes[..=last],
                None => &[],
            };
            let text = std::str::from_utf8(complete).expect("stdout is utf-8");
            text.lines()
                .map(|line| {
                    serde_json::from_str(line).unwrap_or_else(|e| {
                        panic!("stdout line is not protocol JSON: {line:?} ({e})")
                    })
                })
                .collect()
        }
    }

    /// 応答(`type` が `response`)で `id` が一致するもの。
    fn response(lines: &[Value], id: u64) -> Option<&Value> {
        lines
            .iter()
            .find(|line| line["type"] == json!("response") && line["id"] == json!(id))
    }

    /// 問い合わせ(`type` が `dialog`)で `id` が一致するもの。
    fn dialog(lines: &[Value], id: u64) -> Option<&Value> {
        lines
            .iter()
            .find(|line| line["type"] == json!("dialog") && line["id"] == json!(id))
    }

    fn position_of_response(lines: &[Value], id: u64) -> Option<usize> {
        lines
            .iter()
            .position(|line| line["type"] == json!("response") && line["id"] == json!(id))
    }

    fn wait_until(timeout: Duration, mut probe: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if probe() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// 入力(パイプの書き込み側)と出力を持つサーバを起動する。`done` は `serve` が戻ったときに届く。
    struct Bridge {
        input: io::PipeWriter,
        out: Captured,
        done: Receiver<()>,
    }

    fn spawn_server(state: Arc<BridgeState>) -> Bridge {
        let (reader, writer) = io::pipe().expect("pipe");
        let out = Captured::default();
        let shared = Arc::new(Mutex::new(out.clone()));
        let (done_tx, done) = mpsc::channel();
        thread::spawn(move || {
            serve(BufReader::new(reader), shared, state);
            let _ = done_tx.send(());
        });
        Bridge {
            input: writer,
            out,
            done,
        }
    }

    fn send_line(input: &mut io::PipeWriter, value: &Value) {
        let mut line = value.to_string();
        line.push('\n');
        input
            .write_all(line.as_bytes())
            .expect("write request line");
    }

    fn request(id: u64, cmd: &str, args: Value) -> Value {
        json!({"type": "request", "id": id, "cmd": cmd, "payload": {"args": args}})
    }

    fn view() -> Value {
        json!({"tapeRadius": 3, "outputFrom": 0})
    }

    /// `+[]` を読み込み、上限なしのセッションを開始した状態の `BridgeState` と、セッション ID。
    fn state_running_forever() -> (Arc<BridgeState>, u64) {
        let state = Arc::new(BridgeState::default());
        let loaded = dispatch(
            &state,
            "load_source",
            &json!({"args": {"source": "+[]"}}),
            &NoDialogs,
        )
        .expect("load_source");
        let program_id = loaded["programId"].as_u64().expect("programId");
        let started = dispatch(
            &state,
            "start_session",
            &json!({"args": {
                "programId": program_id,
                "inputBase64": "",
                "limit": {"kind": "unlimited"},
                "breakpointOffsets": [],
                "view": view(),
            }}),
            &NoDialogs,
        )
        .expect("start_session");
        (
            state,
            started["state"]["sessionId"].as_u64().expect("sessionId"),
        )
    }

    /// 呼ばれたら失敗にする(ダイアログを使わない準備の呼び出し用)。
    struct NoDialogs;

    impl DialogPort for NoDialogs {
        fn pick(&self, _kind: DialogKind) -> Result<Option<PathBuf>, ApiError> {
            panic!("unexpected dialog during setup");
        }
    }

    #[test]
    fn pause_is_answered_before_an_unlimited_run_batch_and_the_batch_stops() {
        let (state, session_id) = state_running_forever();
        let mut bridge = spawn_server(state);

        send_line(
            &mut bridge.input,
            &request(
                1,
                "run_batch",
                json!({"sessionId": session_id, "batchSize": 1_000_000, "view": view()}),
            ),
        );

        // 旗は run_batch の入口で下ろされるため、run_batch が走り始めるまで Pause を送り続ける。
        // 送った Pause の応答は、バッチの応答より前に届かなければならない(バッチを待たない)。
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut pause_id = 100;
        let mut pause_ids: Vec<u64> = Vec::new();
        loop {
            send_line(
                &mut bridge.input,
                &json!({"type": "request", "id": pause_id, "cmd": "pause", "payload": {"args": {"sessionId": session_id}}}),
            );
            pause_ids.push(pause_id);
            pause_id += 1;
            thread::sleep(Duration::from_millis(5));
            if response(&bridge.out.lines(), 1).is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "run_batch did not stop after pause"
            );
        }

        // 送ったすべての Pause(と run_batch)の応答が届くまで待つ。
        let expected = pause_ids.len() + 1;
        assert!(
            wait_until(Duration::from_secs(10), || bridge.out.lines().len()
                == expected),
            "every pause and the batch must be answered"
        );

        // 応答の id は送った要求の id(run_batch の 1 と、送った Pause の id)のどれかで、
        // 各 id の応答はちょうど 1 回。
        let lines = bridge.out.lines();
        let mut sent_ids = pause_ids.clone();
        sent_ids.push(1);
        for line in &lines {
            assert_eq!(line["type"], json!("response"));
            let id = line["id"].as_u64().expect("response id");
            assert!(sent_ids.contains(&id), "unexpected response id {id}");
        }
        for id in &sent_ids {
            let count = lines.iter().filter(|line| line["id"] == json!(id)).count();
            assert_eq!(
                count, 1,
                "response for request {id} must appear exactly once"
            );
        }

        // Pause の応答のうち最初のものは、バッチの応答より前に届く(バッチを待たない)。
        let batch_at = position_of_response(&lines, 1).expect("run_batch response");
        let first_pause_at = lines
            .iter()
            .position(|line| {
                line["id"]
                    .as_u64()
                    .is_some_and(|id| pause_ids.contains(&id))
            })
            .expect("a pause response");
        assert!(
            first_pause_at < batch_at,
            "the pause response must come before the batch response"
        );
        assert_eq!(lines[first_pause_at]["ok"], json!(true));
        assert_eq!(lines[first_pause_at]["value"], Value::Null);

        let batch = &lines[batch_at];
        assert_eq!(batch["ok"], json!(true));
        assert_eq!(batch["value"]["stop"]["kind"], json!("cancelled"));
        assert_eq!(batch["value"]["state"]["snapshot"]["halted"], json!(false));

        drop(bridge.input);
        bridge
            .done
            .recv_timeout(Duration::from_secs(10))
            .expect("serve returns at end of input");
    }

    #[test]
    fn concurrent_dialogs_are_answered_by_their_own_request_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path_a = dir.path().join("a.bf");
        let path_b = dir.path().join("b.bf");
        std::fs::write(&path_a, "+").expect("write a");
        std::fs::write(&path_b, "-").expect("write b");

        let mut bridge = spawn_server(Arc::new(BridgeState::default()));
        send_line(
            &mut bridge.input,
            &request(10, "open_source_file", Value::Null),
        );
        send_line(
            &mut bridge.input,
            &request(11, "open_source_file", Value::Null),
        );

        assert!(wait_until(Duration::from_secs(10), || {
            let lines = bridge.out.lines();
            dialog(&lines, 10).is_some() && dialog(&lines, 11).is_some()
        }));

        // 返事は問い合わせと逆の順で返し、取り違えれば結果が入れ替わる。
        send_line(
            &mut bridge.input,
            &json!({"type": "dialogResult", "id": 11, "result": {"kind": "path", "path": path_b}}),
        );
        send_line(
            &mut bridge.input,
            &json!({"type": "dialogResult", "id": 10, "result": {"kind": "path", "path": path_a}}),
        );

        assert!(wait_until(Duration::from_secs(10), || {
            let lines = bridge.out.lines();
            response(&lines, 10).is_some() && response(&lines, 11).is_some()
        }));
        let lines = bridge.out.lines();
        let first = response(&lines, 10).expect("response 10");
        let second = response(&lines, 11).expect("response 11");
        assert_eq!(first["value"]["source"], json!("+"));
        assert_eq!(first["value"]["fileName"], json!("a.bf"));
        assert_eq!(second["value"]["source"], json!("-"));
        assert_eq!(second["value"]["fileName"], json!("b.bf"));
    }

    #[test]
    fn a_dialog_reply_completes_a_save_as_and_writes_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("saved.bf");

        let mut bridge = spawn_server(Arc::new(BridgeState::default()));
        send_line(
            &mut bridge.input,
            &request(
                20,
                "save_source_file",
                json!({"source": "+>-", "saveAs": true}),
            ),
        );
        assert!(wait_until(Duration::from_secs(10), || dialog(
            &bridge.out.lines(),
            20
        )
        .is_some()));
        let asked = dialog(&bridge.out.lines(), 20)
            .expect("save dialog")
            .clone();
        assert_eq!(asked["kind"], json!("save"));

        send_line(
            &mut bridge.input,
            &json!({"type": "dialogResult", "id": 20, "result": {"kind": "path", "path": target}}),
        );
        assert!(wait_until(Duration::from_secs(10), || response(
            &bridge.out.lines(),
            20
        )
        .is_some()));

        let lines = bridge.out.lines();
        let saved = response(&lines, 20).expect("save response");
        assert_eq!(saved["ok"], json!(true));
        assert_eq!(
            saved["value"],
            json!({"kind": "saved", "fileName": "saved.bf"})
        );
        assert_eq!(std::fs::read_to_string(&target).expect("saved file"), "+>-");
    }

    #[test]
    fn concurrent_responses_are_whole_lines_and_stdout_holds_only_protocol_lines() {
        const COUNT: u64 = 200;
        let mut bridge = spawn_server(Arc::new(BridgeState::default()));
        for id in 1..=COUNT {
            send_line(
                &mut bridge.input,
                &json!({"type": "request", "id": id, "cmd": "e2e:ping", "payload": null}),
            );
        }

        assert!(wait_until(Duration::from_secs(10), || {
            bridge.out.lines().len() as u64 == COUNT
        }));
        // lines() は各行を JSON として読み、混ざった行があれば失敗する。
        let lines = bridge.out.lines();
        let mut ids: Vec<u64> = lines
            .iter()
            .map(|line| {
                assert_eq!(line["type"], json!("response"));
                assert_eq!(line["value"], json!("pong"));
                line["id"].as_u64().expect("id")
            })
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=COUNT).collect::<Vec<_>>());
    }

    #[test]
    fn unreadable_lines_and_unknown_dialog_results_are_skipped_without_stdout_output() {
        let mut bridge = spawn_server(Arc::new(BridgeState::default()));
        bridge.input.write_all(b"not json\n").expect("write");
        bridge
            .input
            .write_all(b"{\"type\":\"bogus\",\"id\":1}\n")
            .expect("write");
        send_line(
            &mut bridge.input,
            &json!({"type": "dialogResult", "id": 999, "result": {"kind": "cancel"}}),
        );
        send_line(&mut bridge.input, &request(1, "e2e:ping", Value::Null));

        assert!(wait_until(Duration::from_secs(10), || response(
            &bridge.out.lines(),
            1
        )
        .is_some()));
        let lines = bridge.out.lines();
        assert_eq!(
            lines.len(),
            1,
            "only the ping response may reach stdout: {lines:?}"
        );
        assert_eq!(lines[0]["value"], json!("pong"));
    }

    #[test]
    fn input_end_returns_serve_and_releases_a_pending_dialog() {
        let mut bridge = spawn_server(Arc::new(BridgeState::default()));
        send_line(
            &mut bridge.input,
            &request(30, "open_source_file", Value::Null),
        );
        assert!(wait_until(Duration::from_secs(10), || dialog(
            &bridge.out.lines(),
            30
        )
        .is_some()));

        // 返事を送らずに入力を閉じる。保留中の問い合わせは解放され、要求は失敗で終わる。
        drop(bridge.input);
        bridge
            .done
            .recv_timeout(Duration::from_secs(10))
            .expect("serve returns at end of input even with a pending dialog");
        assert!(wait_until(Duration::from_secs(10), || response(
            &bridge.out.lines(),
            30
        )
        .is_some()));
        let lines = bridge.out.lines();
        let failed = response(&lines, 30).expect("released response");
        assert_eq!(failed["ok"], json!(false));
        assert!(failed.get("value").is_none());
    }

    #[test]
    fn serve_returns_at_end_of_input_when_nothing_is_pending() {
        let bridge = spawn_server(Arc::new(BridgeState::default()));
        let Bridge { input, done, .. } = bridge;
        drop(input);
        match done.recv_timeout(Duration::from_secs(10)) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                panic!("serve must return at end of input")
            }
        }
        // 対応表のコマンド名は契約の一覧と同じく 14 件(受信ループの対象を変えていないことの確認)。
        assert_eq!(COMMANDS.len(), 14);
    }

    #[test]
    fn a_non_utf8_line_is_skipped_and_the_following_request_is_answered() {
        let mut bridge = spawn_server(Arc::new(BridgeState::default()));
        bridge.input.write_all(&[0xFF, 0xFE, b'\n']).expect("write");
        send_line(&mut bridge.input, &request(1, "e2e:ping", Value::Null));

        assert!(wait_until(Duration::from_secs(10), || response(
            &bridge.out.lines(),
            1
        )
        .is_some()));
        let lines = bridge.out.lines();
        assert_eq!(
            lines.len(),
            1,
            "only the ping response may reach stdout: {lines:?}"
        );
        assert_eq!(lines[0]["value"], json!("pong"));
    }

    /// 読み取りが常に失敗する入力。
    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read failed"))
        }
    }

    #[test]
    fn an_input_read_failure_returns_serve_instead_of_looping() {
        let (done_tx, done) = mpsc::channel();
        let out = Captured::default();
        let shared = Arc::new(Mutex::new(out.clone()));
        thread::spawn(move || {
            serve(
                BufReader::new(FailingReader),
                shared,
                Arc::new(BridgeState::default()),
            );
            let _ = done_tx.send(());
        });
        done.recv_timeout(Duration::from_secs(10))
            .expect("serve returns after an input read failure");
        assert!(out.lines().is_empty());
    }

    /// 3000 バイトの出力を出して停止したセッション(出力を含むスナップショットを大きくするため)。
    fn state_with_long_output() -> (Arc<BridgeState>, u64) {
        let state = Arc::new(BridgeState::default());
        let source = ".".repeat(3000);
        let loaded = dispatch(
            &state,
            "load_source",
            &json!({"args": {"source": source}}),
            &NoDialogs,
        )
        .expect("load_source");
        let program_id = loaded["programId"].as_u64().expect("programId");
        let started = dispatch(
            &state,
            "start_session",
            &json!({"args": {
                "programId": program_id,
                "inputBase64": "",
                "limit": {"kind": "unlimited"},
                "breakpointOffsets": [],
                "view": view(),
            }}),
            &NoDialogs,
        )
        .expect("start_session");
        let session_id = started["state"]["sessionId"].as_u64().expect("sessionId");
        let batch = dispatch(
            &state,
            "run_batch",
            &json!({"args": {"sessionId": session_id, "batchSize": 1_000_000, "view": view()}}),
            &NoDialogs,
        )
        .expect("run_batch");
        assert_eq!(batch["stop"]["kind"], json!("halted"));
        (state, session_id)
    }

    #[test]
    fn concurrent_large_responses_are_whole_lines_each_on_its_own_line() {
        const COUNT: u64 = 60;
        let (state, session_id) = state_with_long_output();
        let mut bridge = spawn_server(state);
        for id in 1..=COUNT {
            send_line(
                &mut bridge.input,
                &request(
                    id,
                    "get_snapshot",
                    json!({"sessionId": session_id, "view": {"tapeRadius": 64, "outputFrom": 0}}),
                ),
            );
        }

        assert!(
            wait_until(Duration::from_secs(30), || bridge.out.lines().len() as u64
                == COUNT),
            "all large responses must arrive"
        );
        let lines = bridge.out.lines();
        let mut ids: Vec<u64> = Vec::new();
        for line in &lines {
            assert_eq!(line["type"], json!("response"));
            assert_eq!(line["ok"], json!(true));
            assert!(
                line.to_string().len() > 4096,
                "the response must be several KB so that writes interleave: {}",
                line.to_string().len()
            );
            ids.push(line["id"].as_u64().expect("id"));
        }
        ids.sort_unstable();
        assert_eq!(ids, (1..=COUNT).collect::<Vec<_>>());
    }

    #[test]
    fn a_closed_router_refuses_new_dialog_registrations() {
        let router = DialogRouter::default();
        assert!(router.register(1).is_some());
        router.close();
        assert!(router.register(2).is_none());
        assert!(!router.deliver(1, DialogResult::Cancel));
    }
}
