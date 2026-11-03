//! 雛形の振る舞い: 引数なしで起動すると何も書かずに終了コード 0 で終了する。

use std::process::Command;

#[test]
fn binary_without_arguments_exits_zero_with_empty_streams() {
    let output = Command::new(env!("CARGO_BIN_EXE_brainfuck-cli"))
        .output()
        .expect("binary must be runnable");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    assert!(output.stderr.is_empty(), "stderr must be empty");
}
