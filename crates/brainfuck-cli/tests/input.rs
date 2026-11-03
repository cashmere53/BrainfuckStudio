//! 入力処理の結合テスト(Requirement 4、12.3、12.6)。
//! 実バイナリを起動し、入力の供給元(標準入力 / `--input`)、EOF=0、入力を読まない条件、
//! 入力ファイルの失敗を、標準出力・標準エラー・終了コードで検証する。

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use common::{fixture_path, run_cli, write_temp, StdinMode};

/// `fixtures/input.bin` の全バイト(0xFF 0x0A 0x41 0x00 0xFF 0x0A)。
fn input_bytes() -> Vec<u8> {
    fs::read(fixture_path("fixtures/input.bin")).expect("fixture must be readable")
}

/// `fixtures/echo.bf`(`,[.,]`)は 0x00 で停止するため、最初の 0x00 の手前までが出力される。
const ECHO_EXPECTED: [u8; 3] = [0xFF, 0x0A, 0x41];

/// 入力命令を含まないプログラム。セル 0 に 65(`A`)を作って出力する。
const NO_INPUT_PROGRAM: &[u8] = b"++++++++[>++++++++<-]>+.";

/// 0x00 から 0xFF までの全 256 値(昇順)。
fn all_byte_values() -> Vec<u8> {
    (0..=255u8).collect()
}

/// `,.` を 256 個並べたエコープログラム(512 バイト)。`,[.,]` と違い 0x00 で止まらない。
/// 1 組ごとに 1 バイト読んで直ちに出力するため、入力 256 バイトがそのまま出力される。
fn echo_256_program() -> Vec<u8> {
    b",.".repeat(256)
}

#[test]
fn echo_from_stdin_stops_before_first_nul_byte() {
    let echo = fixture_path("fixtures/echo.bf");
    let output = run_cli(&[echo.as_os_str()], StdinMode::Bytes(input_bytes()));

    assert_eq!(
        output.code,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, ECHO_EXPECTED);
    assert!(output.stderr.is_empty());
}

#[test]
fn echo_from_input_file_matches_stdin_source() {
    let echo = fixture_path("fixtures/echo.bf");
    let input = fixture_path("fixtures/input.bin");
    let from_file = run_cli(
        &[echo.as_os_str(), OsStr::new("--input"), input.as_os_str()],
        StdinMode::Null,
    );
    let from_stdin = run_cli(&[echo.as_os_str()], StdinMode::Bytes(input_bytes()));

    assert_eq!(from_file.code, Some(0));
    assert_eq!(from_file.stdout, ECHO_EXPECTED);
    assert!(from_file.stderr.is_empty());
    assert_eq!(from_file.stdout, from_stdin.stdout);
    assert_eq!(from_file.code, from_stdin.code);
}

#[test]
fn input_file_is_read_while_stdin_stays_open_and_unused() {
    // `--input` 指定時は標準入力を読まないため、書かず閉じないパイプでも終了する。
    let echo = fixture_path("fixtures/echo.bf");
    let input = fixture_path("fixtures/input.bin");
    let output = run_cli(
        &[echo.as_os_str(), OsStr::new("--input"), input.as_os_str()],
        StdinMode::OpenUnused,
    );

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, ECHO_EXPECTED);
}

#[test]
fn nul_byte_from_input_is_passed_through_unchanged() {
    // `,.` に 0x00 を 1 バイト与えると、0x00 がそのまま 1 バイト出力される。
    let program = write_temp("input-read-print.bf", b",.");
    let output = run_cli(&[program.as_os_str()], StdinMode::Bytes(vec![0x00]));

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, [0x00]);
    assert!(output.stderr.is_empty());
}

#[test]
fn all_256_byte_values_echo_from_stdin_unchanged() {
    // 0x00 と 0xFF を含む全 256 値を標準入力から与え、`,.`×256 で同一バイト列が出ることを確かめる。
    let program = write_temp("input-echo-256-stdin.bf", &echo_256_program());
    let bytes = all_byte_values();
    let output = run_cli(&[program.as_os_str()], StdinMode::Bytes(bytes.clone()));

    assert_eq!(
        output.code,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, bytes);
    assert!(output.stderr.is_empty());
}

#[test]
fn all_256_byte_values_echo_from_input_file_unchanged() {
    // 同じ全 256 値を `--input` で与え、標準入力を閉じずに同一バイト列が出ることを確かめる。
    let program = write_temp("input-echo-256-file-prog.bf", &echo_256_program());
    let input = write_temp("input-all-256-bytes.bin", &all_byte_values());
    let output = run_cli(
        &[
            program.as_os_str(),
            OsStr::new("--input"),
            input.as_os_str(),
        ],
        StdinMode::OpenUnused,
    );

    assert_eq!(
        output.code,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, all_byte_values());
    assert!(output.stderr.is_empty());
}

#[test]
fn read_after_input_is_exhausted_yields_zero() {
    // `,>,.<.` に 1 バイト(0x41)を与える。2 回目の `,` は EOF=0 としてセル 1 に入り、
    // `.` は 0x00、戻って `.` は 0x41 を出力する。
    let program = write_temp("input-eof-after-exhaust.bf", b",>,.<.");
    let output = run_cli(&[program.as_os_str()], StdinMode::Bytes(vec![0x41]));

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, [0x00, 0x41]);
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_stdin_makes_input_command_read_eof_as_zero() {
    let program = write_temp("input-eof-empty-stdin.bf", b",.");
    let output = run_cli(&[program.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, [0x00]);
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_input_file_makes_input_command_read_eof_as_zero() {
    let program = write_temp("input-eof-empty-file-prog.bf", b",>,.<.");
    let empty = write_temp("input-eof-empty-file.bin", &[]);
    let output = run_cli(
        &[
            program.as_os_str(),
            OsStr::new("--input"),
            empty.as_os_str(),
        ],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, [0x00, 0x00]);
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_stdin_echo_program_produces_no_output() {
    // 入力が空でも `,[.,]` は最初の EOF(0)でループを抜け、出力は空になる。
    let echo = fixture_path("fixtures/echo.bf");
    let output = run_cli(&[echo.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn program_without_input_command_finishes_with_open_unused_stdin() {
    // 入力命令を含まないプログラムは標準入力を読まないため、閉じられていなくても終了する。
    let program = write_temp("input-no-comma.bf", NO_INPUT_PROGRAM);
    let output = run_cli(&[program.as_os_str()], StdinMode::OpenUnused);

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, b"A");
    assert!(output.stderr.is_empty());
}

#[test]
fn input_file_with_program_without_input_command_finishes_with_open_unused_stdin() {
    // `--input` 指定時は、入力命令の有無によらず標準入力を読まない。
    let program = write_temp("input-no-comma-with-file.bf", NO_INPUT_PROGRAM);
    let input = fixture_path("fixtures/input.bin");
    let output = run_cli(
        &[
            program.as_os_str(),
            OsStr::new("--input"),
            input.as_os_str(),
        ],
        StdinMode::OpenUnused,
    );

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, b"A");
    assert!(output.stderr.is_empty());
}

#[test]
fn missing_input_file_is_io_error_for_program_with_input_command() {
    let echo = fixture_path("fixtures/echo.bf");
    let missing = missing_input_path("missing-with-comma.bin");
    let output = run_cli(
        &[echo.as_os_str(), OsStr::new("--input"), missing.as_os_str()],
        StdinMode::Bytes(input_bytes()),
    );

    assert_missing_input_failure(&output, &missing);
}

#[test]
fn missing_input_file_is_io_error_for_program_without_input_command() {
    // 入力命令を含まなくても、`--input` で指定したファイルが読めなければ終了コード 4。
    let program = write_temp("input-missing-no-comma.bf", NO_INPUT_PROGRAM);
    let missing = missing_input_path("missing-without-comma.bin");
    let output = run_cli(
        &[
            program.as_os_str(),
            OsStr::new("--input"),
            missing.as_os_str(),
        ],
        StdinMode::OpenUnused,
    );

    assert_missing_input_failure(&output, &missing);
}

/// 存在しない入力ファイルのパス。テスト実行中に作られないことを前提とする。
fn missing_input_path(name: &str) -> PathBuf {
    let path = fixture_path(&format!("fixtures/{name}"));
    assert!(
        !path.exists(),
        "missing-input fixture must not exist: {}",
        path.display()
    );
    path
}

/// 入力ファイルの読み込み失敗の共通検証: 終了コード 4、標準出力は空、
/// 診断は入力ファイルを対象とし、指定したパス文字列を含まない。
fn assert_missing_input_failure(output: &common::CliOutput, missing: &std::path::Path) {
    assert_eq!(
        output.code,
        Some(4),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on input-file failure"
    );

    let stderr = String::from_utf8(output.stderr.clone()).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("failed to read input file"), "{stderr}");
    assert!(!stderr.contains("standard input"), "{stderr}");
    assert!(
        !stderr.contains(&missing.display().to_string()),
        "diagnostic must not include the path: {stderr}"
    );
}
