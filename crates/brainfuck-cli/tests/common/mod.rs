//! 結合テスト共通ヘルパ。実バイナリ(`CARGO_BIN_EXE_brainfuck-cli`)を起動し、
//! 標準出力・標準エラー・終了コードを返す。各テストファイルから `mod common;` で取り込む。
//!
//! 全テストファイルが同じヘルパを全部使うわけではないため、未使用コードの警告を抑止する。
#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// プロセスがこの時間内に終了しなければ強制終了してテストを失敗させる(12.8)。
const TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// 子プロセスの標準入力の与え方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdinMode {
    /// 即座に閉じる(EOF を与える)。
    Null,
    /// 指定したバイト列を書いてから閉じる。
    Bytes(Vec<u8>),
    /// 何も書かず閉じない(端末相当)。子が読めば終了までブロックする。
    OpenUnused,
}

/// 子プロセスの実行結果。標準出力と標準エラーはバイト列のまま保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// 終了コード。シグナル等で終了コードが無い場合は `None`。
    pub code: Option<i32>,
}

/// 実バイナリを引数と標準入力の与え方を指定して起動し、結果を返す。
pub fn run_cli<S: AsRef<OsStr>>(args: &[S], stdin: StdinMode) -> CliOutput {
    run(args, stdin, false)
}

/// 標準出力の読み取り側を起動直後に閉じて起動する(子の書き込みを失敗させる。終了コード 4 の再現用)。
/// 標準入力は即座に閉じる。戻り値の `stdout` は常に空。
pub fn run_cli_closed_stdout<S: AsRef<OsStr>>(args: &[S]) -> CliOutput {
    run(args, StdinMode::Null, true)
}

fn run<S: AsRef<OsStr>>(args: &[S], stdin: StdinMode, close_stdout: bool) -> CliOutput {
    let mut child = Command::new(env!("CARGO_BIN_EXE_brainfuck-cli"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary must be runnable");

    let child_stdin = child.stdin.take().expect("stdin pipe must be present");
    let mut held_stdin: Option<ChildStdin> = None;
    let mut writer: Option<JoinHandle<()>> = None;
    match stdin {
        StdinMode::Null => drop(child_stdin),
        StdinMode::Bytes(bytes) => {
            // 子が読まずに終了した場合の書き込み失敗は無視する(終了後の比較で検出される)。
            writer = Some(thread::spawn(move || {
                let mut pipe = child_stdin;
                let _ = pipe.write_all(&bytes);
            }));
        }
        StdinMode::OpenUnused => held_stdin = Some(child_stdin),
    }

    let stdout_pipe = child.stdout.take().expect("stdout pipe must be present");
    let stdout_reader = if close_stdout {
        drop(stdout_pipe);
        None
    } else {
        Some(spawn_reader(stdout_pipe))
    };
    let stderr_reader = spawn_reader(child.stderr.take().expect("stderr pipe must be present"));

    let status = wait_with_timeout(&mut child);

    // 子の終了後に閉じる。OpenUnused の入力はここまで保持して、子が入力待ちで止まらないことを確かめる。
    drop(held_stdin);
    if let Some(handle) = writer {
        let _ = handle.join();
    }
    let stdout = match stdout_reader {
        Some(handle) => handle.join().expect("stdout reader must not panic"),
        None => Vec::new(),
    };
    let stderr = stderr_reader.join().expect("stderr reader must not panic");

    CliOutput {
        stdout,
        stderr,
        code: status_code(status),
    }
}

/// パイプを EOF まで読み切るスレッドを起動する。標準出力と標準エラーを並行に読むことでデッドロックを避ける。
fn spawn_reader<R: Read + Send + 'static>(mut pipe: R) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        // 読み取りエラーで途中まで読めた場合も、終了コードとの比較で不一致として検出される。
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

/// 子が終了するまでポーリングし、30 秒を超えたら強制終了してパニックさせる。
fn wait_with_timeout(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait().expect("child status must be readable") {
            Some(status) => return status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("brainfuck-cli did not exit within {TIMEOUT:?} and was killed");
            }
            None => thread::sleep(POLL_INTERVAL),
        }
    }
}

fn status_code(status: ExitStatus) -> Option<i32> {
    status.code()
}

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// `CARGO_TARGET_TMPDIR` 配下に一意名でバイト列のファイルを作成し、そのパスを返す。
/// 名前は「プロセス ID・連番・指定名」で組み立てるため、並列実行しても衝突しない。
pub fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
    let unique = format!(
        "{}-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed),
        name
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(unique);
    fs::write(&path, bytes).expect("temp file must be writable");
    path
}

/// ワークスペース直下の `examples/` 内のファイルパス(例: `hello.bf`)。
pub fn example_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join(name)
}

/// `crates/brainfuck-cli/tests/` 配下のファイルパス(例: `fixtures/echo.bf`、`regression/empty_program.bf`)。
pub fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(relative)
}
