#![forbid(unsafe_code)]

mod app;
mod args;
mod error;
mod input;
mod source;

use std::io;
use std::process::ExitCode;

/// 標準ストリームのロックを注入用の構造に詰め、引数の列を入口へ渡して終了コードを返す。
/// それ以外の処理は [`app::run_cli`] に委ねる。
fn main() -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stdin_lock = stdin.lock();
    let mut stdout_lock = stdout.lock();
    let mut stderr_lock = stderr.lock();
    let mut streams = app::Streams {
        stdin: &mut stdin_lock,
        stdout: &mut stdout_lock,
        stderr: &mut stderr_lock,
    };
    ExitCode::from(app::run_cli(std::env::args_os().skip(1), &mut streams))
}
