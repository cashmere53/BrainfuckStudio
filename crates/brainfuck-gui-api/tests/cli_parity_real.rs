//! 実 CLI バイナリとの結果一致テスト(9.3、要件 11.1–11.4、15.4)。
//!
//! 確認する内容: 固定ケース(Hello World、入力あり、ポインタ範囲外、命令数上限超過、括弧不整合 2 種)を
//! 実際の `brainfuck-cli` バイナリで実行し、標準出力のバイト列、`--dump-state` の状態行
//! (`state: pc=… dp=… steps=…`)、実行時エラーの診断行、終了コードの分類(0 / 2 / 3)が
//! `DebuggerService` の結果と一致することを確認する。`examples/hello.bf` は
//! `Hello World!\n` と `state: pc=106 dp=6 steps=906` になることを文字列としても確認する。
//!
//! バイナリの取得: 環境変数 `BRAINFUCK_CLI_BIN` が指定されていればそのファイルを使う。
//! 無ければ `cargo build -p brainfuck-cli --message-format=json` を 1 回だけ実行し、
//! 出力の `compiler-artifact`(`brainfuck-cli` の bin)の `executable` を使う。
//!
//! 外部プロセスの起動はこのファイルだけで行う。CLI の起動には 30 秒のタイムアウトを付け、
//! 超えた場合は強制終了してテストを失敗させる(固まったプロセスでテストがハングし続けないため)。
//! 強制終了後は `wait` しない(固まったプロセスの終了を待って止まらないため)。
//!
//! 文書化した対象外(`cli_parity_core.rs` と同じ): 命令数上限 0(GUI は受け付けない)、
//! 非 UTF-8 ファイル(GUI は置換して読み込む)。本ファイルのソースはすべて ASCII の UTF-8 として与える。

mod common;

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use brainfuck_gui_api::codec;
use brainfuck_gui_api::dto::{
    BatchStopDto, LoadSourceRequest, LoadSourceResult, ParseErrorDto, ParseErrorKind,
    PointerDirectionDto, RuntimeErrorDto, SnapshotDto, StepLimitDto,
};
use brainfuck_gui_api::service::DebuggerService;

use common::{load_ok, run_until_stop, service, start_request};

/// CLI の既定の命令数上限(`--max-steps` を指定しないとき)。
const DEFAULT_MAX_STEPS: u64 = 10_000_000;

/// CLI 起動 1 回あたりの待ち時間。超えたら強制終了して失敗させる。
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

/// `cargo build -p brainfuck-cli` の待ち時間。コンパイル時間を見込んだ上限。
const BUILD_TIMEOUT: Duration = Duration::from_secs(600);

/// 終了待ちのポーリング間隔。
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// 既存のバイナリの場所を指定する環境変数名。
const CLI_BIN_ENV: &str = "BRAINFUCK_CLI_BIN";

/// 実 CLI の 1 回の実行結果。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CliRun {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    code: Option<i32>,
}

/// サービス経由の 1 回の実行結果(`run_batch` を停止まで反復した最後の状態)。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ServiceRun {
    stop: BatchStopDto,
    snapshot: SnapshotDto,
    output: Vec<u8>,
}

/// リポジトリ同梱の `examples/hello.bf` の内容。
fn hello_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello.bf");
    fs::read_to_string(&path).expect("examples/hello.bf must be readable")
}

/// `--dump-state` の状態行(CLI と同じ書式)。
fn state_line(snapshot: &SnapshotDto) -> String {
    format!(
        "state: pc={} dp={} steps={}\n",
        snapshot.pc, snapshot.dp, snapshot.steps_executed
    )
}

#[test]
fn cli_binary_is_resolved_from_build_json_or_environment() {
    // バイナリの取得経路そのものを確認する(以降のケースの前提)。
    let binary = cli_binary();
    assert!(binary.is_file(), "{}", binary.display());
    assert!(
        binary
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("brainfuck-cli")),
        "{}",
        binary.display()
    );
}

#[test]
fn hello_world_matches_the_real_cli_output_exit_code_and_state() {
    let source = hello_source();
    let expected = service_run(&source, b"", DEFAULT_MAX_STEPS);
    assert_eq!(expected.stop, BatchStopDto::Halted);

    let dir = tempfile::tempdir().expect("temp dir");
    let program = write_program(dir.path(), &source);

    let with_dump = run_cli(&[os("--dump-state"), program.clone().into_os_string()], b"");
    assert_eq!(with_dump.code, Some(0), "{with_dump:?}");
    assert_eq!(with_dump.stdout, b"Hello World!\n", "実 CLI の出力");
    assert_eq!(with_dump.stdout, expected.output, "サービスの出力と一致");
    assert_eq!(
        String::from_utf8_lossy(&with_dump.stderr),
        "state: pc=106 dp=6 steps=906\n"
    );
    assert_eq!(with_dump.stderr, state_line(&expected.snapshot).as_bytes());
    assert_eq!(expected.snapshot.pc, 106);
    assert_eq!(expected.snapshot.dp, 6);
    assert_eq!(expected.snapshot.steps_executed, 906);

    // `--dump-state` なしでは成功時の標準エラーは空。
    let plain = run_cli(&[program.into_os_string()], b"");
    assert_eq!(plain.code, Some(0), "{plain:?}");
    assert_eq!(plain.stdout, b"Hello World!\n");
    assert!(plain.stderr.is_empty(), "{plain:?}");
}

#[test]
fn input_bytes_are_echoed_identically_by_the_real_cli_and_the_service() {
    // `,[.,]` は 0x00 を読むまで入力を書き出す。0xFF を含み、非 ASCII のバイトもそのまま出る。
    let source = ",[.,]";
    let input = [0x41, 0xFF, 0x00];
    let expected = service_run(source, &input, DEFAULT_MAX_STEPS);
    assert_eq!(expected.stop, BatchStopDto::Halted);
    assert_eq!(expected.output, [0x41, 0xFF]);

    let dir = tempfile::tempdir().expect("temp dir");
    let program = write_program(dir.path(), source);
    let run = run_cli(&[os("--dump-state"), program.into_os_string()], &input);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(
        run.stdout,
        [0x41, 0xFF],
        "実 CLI の出力(標準入力の読み取り)"
    );
    assert_eq!(run.stdout, expected.output);
    assert_eq!(run.stderr, state_line(&expected.snapshot).as_bytes());
}

#[test]
fn pointer_out_of_bounds_matches_the_real_cli_diagnostic_state_and_exit_code() {
    // `+.<` は 1 を出力してから左端で `<` が範囲外になる。部分出力も一致する。
    let source = "+.<";
    let expected = service_run(source, b"", DEFAULT_MAX_STEPS);
    let BatchStopDto::Error {
        error: RuntimeErrorDto::PointerOutOfBounds { pc, dp, direction },
    } = expected.stop.clone()
    else {
        panic!(
            "service must report a pointer error, got {:?}",
            expected.stop
        );
    };
    assert_eq!(direction, PointerDirectionDto::Left);
    assert_eq!(expected.output, [0x01]);

    let dir = tempfile::tempdir().expect("temp dir");
    let program = write_program(dir.path(), source);
    let run = run_cli(&[os("--dump-state"), program.into_os_string()], b"");
    assert_eq!(run.code, Some(3), "{run:?}");
    assert_eq!(run.stdout, [0x01], "部分出力は標準出力に書かれる");
    assert_eq!(run.stdout, expected.output);

    let expected_stderr = format!(
        "error: runtime error: data pointer out of bounds: pc={pc}, dp={dp}, direction=left\n{}",
        state_line(&expected.snapshot)
    );
    assert_eq!(String::from_utf8_lossy(&run.stderr), expected_stderr);
}

#[test]
fn step_limit_exceeded_matches_the_real_cli_diagnostic_and_exit_code() {
    // `+[]` は cell が 1 のまま無限に回る。上限 10 で停止する。
    let source = "+[]";
    let limit = 10;
    let expected = service_run(source, b"", limit);
    let BatchStopDto::Error {
        error:
            RuntimeErrorDto::StepLimitExceeded {
                limit: reported,
                pc,
            },
    } = expected.stop.clone()
    else {
        panic!(
            "service must report a step limit error, got {:?}",
            expected.stop
        );
    };
    assert_eq!(reported, limit);
    assert_eq!(expected.snapshot.steps_executed, limit);
    assert!(expected.output.is_empty());

    let dir = tempfile::tempdir().expect("temp dir");
    let program = write_program(dir.path(), source);
    let run = run_cli(
        &[
            os("--max-steps"),
            os("10"),
            os("--dump-state"),
            program.into_os_string(),
        ],
        b"",
    );
    assert_eq!(run.code, Some(3), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    let expected_stderr = format!(
        "error: runtime error: step limit exceeded: limit={limit} after {limit} steps, pc={pc}\n{}",
        state_line(&expected.snapshot)
    );
    assert_eq!(String::from_utf8_lossy(&run.stderr), expected_stderr);
}

#[test]
fn unmatched_brackets_match_the_real_cli_message_and_exit_code_2() {
    // 2 種の括弧不整合について、サービスの解析エラーの種類・命令添字・位置と CLI の文面を照合する。
    for (source, expected_kind, kind_text) in [
        (
            "[",
            ParseErrorKind::UnmatchedOpeningBracket,
            "unmatched opening bracket",
        ),
        (
            "+]",
            ParseErrorKind::UnmatchedClosingBracket,
            "unmatched closing bracket",
        ),
    ] {
        let error = service_parse_error(source);
        assert_eq!(error.kind, expected_kind, "{source:?}");

        let dir = tempfile::tempdir().expect("temp dir");
        let program = write_program(dir.path(), source);
        let run = run_cli(&[program.into_os_string()], b"");
        assert_eq!(run.code, Some(2), "{source:?}: {run:?}");
        assert!(run.stdout.is_empty(), "{source:?}: {run:?}");
        // ASCII のみのソースなので、UTF-16 オフセットと CLI のバイトオフセットは一致する。
        let expected_stderr = format!(
            "error: parse error: {kind_text}: instruction #{}, source offset {}\n",
            error.instruction_index, error.position.byte_offset
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stderr),
            expected_stderr,
            "{source:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// サービス側の結果
// ---------------------------------------------------------------------------

/// ソースを読み込み、既定のバッチ幅で停止まで実行したサービスの結果を返す。
fn service_run(source: &str, input: &[u8], max_steps: u64) -> ServiceRun {
    let svc = service();
    let program = load_ok(&svc, source);
    let session = svc
        .start_session(start_request(
            program,
            input,
            StepLimitDto::Limited { max_steps },
            vec![],
        ))
        .expect("session starts")
        .state
        .session_id;
    let run = run_until_stop(&svc, session, None, 1_000);
    ServiceRun {
        stop: run.last.stop,
        snapshot: run.last.state.snapshot,
        output: full_output(&run.last.state),
    }
}

/// 出力全体を取り出す。表示窓の要求を `output_from = 0` にしているため、先頭から全体が返る。
fn full_output(state: &brainfuck_gui_api::dto::SessionStateDto) -> Vec<u8> {
    assert_eq!(state.output.data_start, 0, "出力窓は先頭から返る");
    let bytes = codec::decode(&state.output.data_base64, state.output.total_len)
        .expect("output decodes within its declared length");
    assert_eq!(
        bytes.len(),
        state.output.total_len,
        "出力の総バイト数と一致"
    );
    bytes
}

/// 括弧不整合のソースを読み込み、サービスの解析エラーを返す。
fn service_parse_error(source: &str) -> ParseErrorDto {
    let svc: DebuggerService = service();
    match svc
        .load_source(LoadSourceRequest {
            source: source.to_owned(),
        })
        .expect("load_source accepts the source")
    {
        LoadSourceResult::ParseError { error } => error,
        other => panic!("expected a parse error for {source:?}, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 実 CLI の起動
// ---------------------------------------------------------------------------

fn os(text: &str) -> OsString {
    OsString::from(text)
}

fn write_program(dir: &Path, source: &str) -> PathBuf {
    let path = dir.join("program.bf");
    fs::write(&path, source).expect("program file is written");
    path
}

/// 実 CLI を起動し、標準入力を与えて終了まで待つ。30 秒を超えたら強制終了してパニックする。
fn run_cli(args: &[OsString], stdin: &[u8]) -> CliRun {
    let mut child = Command::new(cli_binary())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("brainfuck-cli must be runnable");

    let mut child_stdin = child.stdin.take().expect("stdin pipe must be present");
    let input = stdin.to_vec();
    // 子が読まずに終了した場合の書き込み失敗は無視する(終了後の比較で検出される)。
    let writer = thread::spawn(move || {
        let _ = child_stdin.write_all(&input);
    });
    let stdout_reader = spawn_reader(child.stdout.take().expect("stdout pipe must be present"));
    let stderr_reader = spawn_reader(child.stderr.take().expect("stderr pipe must be present"));

    let status = wait_with_timeout(&mut child, CLI_TIMEOUT, "brainfuck-cli");
    let _ = writer.join();
    CliRun {
        stdout: stdout_reader.join().expect("stdout reader must not panic"),
        stderr: stderr_reader.join().expect("stderr reader must not panic"),
        code: status.code(),
    }
}

/// パイプを EOF まで読み切るスレッドを起動する。標準出力と標準エラーを並行に読むことでデッドロックを避ける。
fn spawn_reader<R: Read + Send + 'static>(mut pipe: R) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        // 読み取りエラーで途中まで読めた場合も、比較で不一致として検出される。
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

/// 子が終了するまでポーリングし、`timeout` を超えたら強制終了してパニックさせる。
/// 強制終了後は `wait` しない(固まったプロセスの終了を待ってテストが止まらないようにするため)。
fn wait_with_timeout(child: &mut Child, timeout: Duration, what: &str) -> ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().expect("child status must be readable") {
            Some(status) => return status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                panic!("{what} did not exit within {timeout:?} and was killed");
            }
            None => thread::sleep(POLL_INTERVAL),
        }
    }
}

/// テスト実行中に 1 回だけ解決した CLI バイナリの場所を返す。
fn cli_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(resolve_cli_binary).as_path()
}

fn resolve_cli_binary() -> PathBuf {
    if let Some(value) = std::env::var_os(CLI_BIN_ENV).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        assert!(
            path.is_file(),
            "{CLI_BIN_ENV} must point to an existing brainfuck-cli binary: {}",
            path.display()
        );
        return path;
    }

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut child = Command::new(cargo)
        .args(["build", "-p", "brainfuck-cli", "--message-format=json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cargo build -p brainfuck-cli must start");
    let stdout_reader = spawn_reader(child.stdout.take().expect("stdout pipe must be present"));
    let stderr_reader = spawn_reader(child.stderr.take().expect("stderr pipe must be present"));
    let status = wait_with_timeout(&mut child, BUILD_TIMEOUT, "cargo build -p brainfuck-cli");
    let stdout = stdout_reader.join().expect("stdout reader must not panic");
    let stderr = stderr_reader.join().expect("stderr reader must not panic");
    assert!(
        status.success(),
        "cargo build -p brainfuck-cli failed:\n{}",
        String::from_utf8_lossy(&stderr)
    );
    executable_from_build_json(&stdout)
        .unwrap_or_else(|| panic!("cargo build output has no brainfuck-cli executable artifact"))
}

/// `--message-format=json` の出力から `brainfuck-cli` の bin の `executable` を取り出す。
fn executable_from_build_json(stdout: &[u8]) -> Option<PathBuf> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == "brainfuck-cli"
        })
        .filter_map(|message| message["executable"].as_str().map(PathBuf::from))
        .next_back()
}
