//! Node とブリッジの間の行プロトコル。1 行 1 メッセージの JSON を読み書きする。
//!
//! Node → ブリッジ: 要求(`request`)、ダイアログの返事(`dialogResult`)。
//! ブリッジ → Node: 応答(`response`。成功は `value`、失敗は `error` だけを持つ)、ダイアログの問い合わせ(`dialog`)。
//! 解釈できない行は `parse_inbound` が失敗を返す。呼び出し側(server)が標準エラーへ記録して読み飛ばす。

use std::io::{self, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Node → ブリッジ。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Inbound {
    /// コマンドの要求。`payload` は `{ "args": ... }` の形。
    Request {
        id: u64,
        cmd: String,
        payload: Value,
    },
    /// ダイアログの返事(問い合わせの `id` と対応する)。
    DialogResult { id: u64, result: DialogResult },
}

/// ダイアログの返事。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DialogResult {
    Path { path: String },
    Cancel,
}

/// ダイアログの種類(開く / 保存)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DialogKind {
    Open,
    Save,
}

/// ブリッジ → Node。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Outbound {
    /// 要求への応答。成功は `value`(`()` の応答は `null`)、失敗は `error`(`ApiError` の JSON または文字列)だけを持つ。
    Response {
        id: u64,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<Value>,
    },
    /// ダイアログの問い合わせ。返事は `dialogResult` で同じ `id` に返る。
    Dialog { id: u64, kind: DialogKind },
}

impl Outbound {
    /// 成功の応答。
    pub fn ok(id: u64, value: Value) -> Self {
        Self::Response {
            id,
            ok: true,
            value: Some(value),
            error: None,
        }
    }

    /// 失敗の応答。
    pub fn err(id: u64, error: Value) -> Self {
        Self::Response {
            id,
            ok: false,
            value: None,
            error: Some(error),
        }
    }
}

/// 1 行の JSON を `Inbound` として読む。形が合わない行は `Err`。
pub fn parse_inbound(line: &str) -> Result<Inbound, serde_json::Error> {
    serde_json::from_str(line)
}

/// メッセージを 1 行(末尾に改行 1 つ)で書き、フラッシュする。
pub fn write_line<W: Write>(out: &mut W, message: &Outbound) -> io::Result<()> {
    let mut line = serde_json::to_string(message).map_err(io::Error::other)?;
    line.push('\n');
    out.write_all(line.as_bytes())?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn encode(message: &Outbound) -> String {
        let mut buf: Vec<u8> = Vec::new();
        write_line(&mut buf, message).expect("write_line");
        String::from_utf8(buf).expect("utf8")
    }

    #[test]
    fn parses_request_and_round_trips_it() {
        let line =
            r#"{"type":"request","id":1,"cmd":"load_source","payload":{"args":{"source":"+"}}}"#;
        let inbound = parse_inbound(line).expect("request must parse");
        assert_eq!(
            inbound,
            Inbound::Request {
                id: 1,
                cmd: "load_source".to_string(),
                payload: json!({"args": {"source": "+"}}),
            }
        );
        let back: Value = serde_json::to_value(&inbound).expect("serialize");
        assert_eq!(back, serde_json::from_str::<Value>(line).expect("json"));
    }

    #[test]
    fn parses_dialog_results_for_path_and_cancel() {
        let path = parse_inbound(
            r#"{"type":"dialogResult","id":2,"result":{"kind":"path","path":"C:/a.bf"}}"#,
        )
        .expect("path result must parse");
        assert_eq!(
            path,
            Inbound::DialogResult {
                id: 2,
                result: DialogResult::Path {
                    path: "C:/a.bf".to_string()
                },
            }
        );

        let cancel = parse_inbound(r#"{"type":"dialogResult","id":3,"result":{"kind":"cancel"}}"#)
            .expect("cancel result must parse");
        assert_eq!(
            cancel,
            Inbound::DialogResult {
                id: 3,
                result: DialogResult::Cancel,
            }
        );
    }

    #[test]
    fn rejects_invalid_inbound_lines() {
        let invalid = [
            "",
            "not json",
            r#"{"type":"bogus","id":1}"#,
            r#"{"type":"request","cmd":"step","payload":null}"#,
            r#"{"type":"request","id":"1","cmd":"step","payload":null}"#,
            r#"{"type":"request","id":1,"cmd":"step"}"#,
            r#"{"type":"dialogResult","id":1,"result":{"kind":"maybe"}}"#,
            r#"{"type":"dialogResult","id":1,"result":{"kind":"path"}}"#,
        ];
        for line in invalid {
            assert!(parse_inbound(line).is_err(), "must reject: {line:?}");
        }
    }

    #[test]
    fn success_response_has_value_only() {
        let line = encode(&Outbound::ok(4, Value::Null));
        assert_eq!(
            line,
            "{\"type\":\"response\",\"id\":4,\"ok\":true,\"value\":null}\n"
        );
        let value: Value = serde_json::from_str(line.trim_end()).expect("json");
        assert!(value.get("error").is_none());
    }

    #[test]
    fn failure_response_has_error_only() {
        let error = json!({"kind": "invalidArgument", "message": "x", "field": null});
        let line = encode(&Outbound::err(5, error.clone()));
        let value: Value = serde_json::from_str(line.trim_end()).expect("json");
        assert_eq!(
            value,
            json!({"type": "response", "id": 5, "ok": false, "error": error})
        );
        assert!(value.get("value").is_none());
    }

    #[test]
    fn dialog_request_is_one_line() {
        let line = encode(&Outbound::Dialog {
            id: 6,
            kind: DialogKind::Save,
        });
        assert_eq!(line, "{\"type\":\"dialog\",\"id\":6,\"kind\":\"save\"}\n");
    }

    /// 書き込まれた長さと、flush が呼ばれた時点で書かれていた長さを記録する出力。
    #[derive(Default)]
    struct FlushRecorder {
        written: Vec<u8>,
        flushed_at: Vec<usize>,
    }

    impl Write for FlushRecorder {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushed_at.push(self.written.len());
            Ok(())
        }
    }

    #[test]
    fn write_line_flushes_after_the_whole_line_is_written() {
        let mut out = FlushRecorder::default();
        write_line(&mut out, &Outbound::ok(8, Value::Null)).expect("write_line");
        assert_eq!(out.flushed_at, vec![out.written.len()]);
        assert!(out.written.ends_with(b"\n"));
    }

    #[test]
    fn write_line_emits_exactly_one_line() {
        let line = encode(&Outbound::ok(7, json!({"text": "a\nb"})));
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
    }
}
