//! 出力と診断のストリーム分離、およびバイト透過の結合テスト(Requirement 5, 12.5)。
//! 実バイナリを起動し、標準出力・標準エラー・終了コードを検証する。
//! 期待値は要件(プログラム出力のバイト列、診断は標準エラーのみ)から導出する。

mod common;

use std::ffi::OsStr;

use common::{example_path, fixture_path, run_cli, write_temp, StdinMode};

/// `.+` を 256 回繰り返すプログラム。セル値 0, 1, ..., 255 を順に 1 バイトずつ出力する。
fn all_byte_values_program() -> Vec<u8> {
    ".+".repeat(256).into_bytes()
}

#[test]
fn all_256_byte_values_reach_stdout_unchanged() {
    let program = write_temp("all_bytes.bf", &all_byte_values_program());
    let output = run_cli(&[program.as_os_str()], StdinMode::Null);

    let expected: Vec<u8> = (0u8..=255).collect();
    assert_eq!(
        output.code,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout, expected,
        "0x00 から 0xFF まで欠落・置換・変換なしで出ること"
    );
    assert!(output.stderr.is_empty(), "成功時は標準エラーが空であること");
}

#[test]
fn crlf_bytes_are_not_converted_or_appended() {
    // セル 13(CR)を出力し、3 減らして 10(LF)を出力し、55 増やして 65(`A`)を出力し、
    // 52 減らして 13(CR)を出力し、3 減らして 10(LF)を出力する。期待出力は CR LF A CR LF。
    let mut program = String::new();
    program.push_str(&"+".repeat(13));
    program.push('.');
    program.push_str(&"-".repeat(3));
    program.push('.');
    program.push_str(&"+".repeat(55));
    program.push('.');
    program.push_str(&"-".repeat(52));
    program.push('.');
    program.push_str(&"-".repeat(3));
    program.push('.');
    let path = write_temp("crlf.bf", program.as_bytes());
    let output = run_cli(&[path.as_os_str()], StdinMode::Null);

    assert_eq!(
        output.code,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"\r\nA\r\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn argument_errors_write_only_diagnostics_to_stderr() {
    let hello = example_path("hello.bf");

    // プログラムパスの欠落
    let no_path: [&OsStr; 0] = [];
    let missing = run_cli(&no_path, StdinMode::Null);
    assert_eq!(missing.code, Some(2));
    assert!(
        missing.stdout.is_empty(),
        "引数エラーで標準出力に書かないこと"
    );
    assert!(!missing.stderr.is_empty(), "診断は標準エラーに書くこと");

    // 未知のオプション
    let unknown = run_cli(&[hello.as_os_str(), OsStr::new("--bogus")], StdinMode::Null);
    assert_eq!(unknown.code, Some(2));
    assert!(unknown.stdout.is_empty());
    assert!(!unknown.stderr.is_empty());

    // プログラムパスが 2 個以上
    let two_paths = run_cli(&[hello.as_os_str(), hello.as_os_str()], StdinMode::Null);
    assert_eq!(two_paths.code, Some(2));
    assert!(two_paths.stdout.is_empty());
    assert!(!two_paths.stderr.is_empty());
}

#[test]
fn bracket_error_writes_no_program_output_and_only_stderr_diagnostic() {
    // `+.[` は先頭で出力命令を含むが、解析段階で括弧エラーになり実行に至らない。
    let path = write_temp("unbalanced_open.bf", b"+.[");
    let output = run_cli(&[path.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(2));
    assert!(
        output.stdout.is_empty(),
        "解析エラーでは出力を一切書かないこと"
    );
    assert!(
        !output.stderr.is_empty(),
        "括弧エラーの診断は標準エラーに書くこと"
    );
}

#[test]
fn read_errors_write_only_diagnostics_to_stderr() {
    // ソースファイルの不在
    let missing_source = fixture_path("fixtures/does_not_exist.bf");
    let source = run_cli(&[missing_source.as_os_str()], StdinMode::Null);
    assert_eq!(source.code, Some(4));
    assert!(
        source.stdout.is_empty(),
        "読込エラーで標準出力に書かないこと"
    );
    assert!(
        !source.stderr.is_empty(),
        "読込エラーの診断は標準エラーに書くこと"
    );

    // 入力ファイルの不在(プログラムは `,` を含まないが、--input 指定時は読込失敗で 4)
    let program = write_temp("no_input_read.bf", b"+.");
    let missing_input = fixture_path("fixtures/does_not_exist.bin");
    let input = run_cli(
        &[
            program.as_os_str(),
            OsStr::new("--input"),
            missing_input.as_os_str(),
        ],
        StdinMode::Null,
    );
    assert_eq!(input.code, Some(4));
    assert!(
        input.stdout.is_empty(),
        "入力読込エラーで標準出力に書かないこと"
    );
    assert!(!input.stderr.is_empty());
}

#[test]
fn runtime_error_keeps_partial_output_on_stdout_and_diagnostic_on_stderr() {
    // 255 を出力し、1 を加えて 0 を出力し、`<` で範囲外(dp=0 から左)になり実行エラーで中断する。
    // 部分出力は 0xFF 0x00 であり、UTF-8 として不正なバイトも欠落なく出ること。
    let mut program = String::new();
    program.push_str(&"+".repeat(255));
    program.push('.');
    program.push('+');
    program.push('.');
    program.push('<');
    let path = write_temp("runtime_partial.bf", program.as_bytes());
    let output = run_cli(&[path.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(3));
    assert_eq!(
        output.stdout,
        [0xFF, 0x00],
        "中断までの出力が標準出力にあること"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("runtime error"),
        "実行エラーの診断が標準エラーにあること: {stderr}"
    );
    assert!(
        !output.stdout.windows(7).any(|window| window == b"runtime"),
        "診断の文言が標準出力に混ざらないこと"
    );
}

#[test]
fn empty_output_program_writes_no_bytes_to_stdout() {
    // 出力命令を持たないプログラムは、標準出力へ 1 バイトも書かず、成功時は標準エラーが空。
    let program = write_temp("no_output.bf", b"+++[-]");
    let output = run_cli(&[program.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(0));
    assert!(
        output.stdout.is_empty(),
        "出力が空なら標準出力に 1 バイトも書かないこと"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_source_file_writes_no_bytes_to_stdout() {
    let output = run_cli(
        &[fixture_path("regression/empty_program.bf").as_os_str()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
