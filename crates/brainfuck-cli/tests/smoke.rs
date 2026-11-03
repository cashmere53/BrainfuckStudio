//! 共通ヘルパの疎通確認: 実バイナリを起動し、サンプルの終了コード・標準出力と、
//! 書かず閉じないパイプ(端末相当)でも時間内に終了することを確かめる。
//! このファイルは 6.7 が同内容を取り込んだ時点で削除する。

mod common;

use common::{example_path, run_cli, StdinMode};

#[test]
fn hello_example_runs_through_real_binary() {
    let hello = example_path("hello.bf");
    let output = run_cli(&[hello.as_os_str()], StdinMode::Null);

    assert_eq!(
        output.code,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Hello World!\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn hello_example_finishes_with_open_unused_stdin() {
    let hello = example_path("hello.bf");
    let output = run_cli(&[hello.as_os_str()], StdinMode::OpenUnused);

    assert_eq!(output.code, Some(0));
}
