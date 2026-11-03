#![forbid(unsafe_code)]

mod error;
mod args;
mod source;
mod input;
mod app;

use std::process::ExitCode;

// 後続タスクで実行経路に接続するまでの仮の本体。何も出力せず終了コード 0 を返す。
fn main() -> ExitCode {
    ExitCode::SUCCESS
}
