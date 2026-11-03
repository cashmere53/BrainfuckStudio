//! 入口の振る舞い: 引数なしで起動すると標準出力は空、標準エラーに診断を書き、終了コード 2 で終了する。

use std::process::Command;

#[test]
fn binary_without_arguments_is_usage_error_with_empty_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_brainfuck-cli"))
        .output()
        .expect("binary must be runnable");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("missing program path"), "{stderr}");
}
