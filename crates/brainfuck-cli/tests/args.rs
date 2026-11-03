//! 引数処理と使用方法の結合テスト(要件 1・2・9.6)。実バイナリを起動し、
//! 標準出力・標準エラー・終了コードを検証する。
//!
//! 引数エラーは検証段で止まり、ファイルも標準入力も読まない。そのことを確かめるため、
//! 存在しないパスや書かず閉じない標準入力を与えるケースを含む。

mod common;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use common::{example_path, fixture_path, run_cli, write_temp, CliOutput, StdinMode};

/// 読みに行けば終了コード 4 になる、存在しないプログラムパス。
const MISSING_PROGRAM: &str = "no-such-dir-6-1/missing-program.bf";
/// 読みに行けば終了コード 4 になる、存在しない入力ファイルパス。
const MISSING_INPUT: &str = "no-such-dir-6-1/missing-input.bin";

/// `fixtures/input.bin` を入力として `fixtures/echo.bf` を実行すると、最初の 0x00 までを出力する。
const ECHO_OUTPUT: [u8; 3] = [0xFF, 0x0A, 0x41];

fn hello() -> OsString {
    example_path("hello.bf").into_os_string()
}

fn echo() -> OsString {
    fixture_path("fixtures/echo.bf").into_os_string()
}

fn echo_input_path() -> PathBuf {
    fixture_path("fixtures/input.bin")
}

/// `prefix` に続けてパスを連結した引数(`--input=PATH` 形式の組み立てに使う)。
fn with_prefix(prefix: &str, path: &Path) -> OsString {
    let mut arg = OsString::from(prefix);
    arg.push(path.as_os_str());
    arg
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn stderr_text(output: &CliOutput) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 引数エラーの共通確認: 終了コード 2、標準出力空、標準エラーに原因と使用方法の案内。
fn assert_usage_error(output: &CliOutput, cause: &str) {
    let stderr = stderr_text(output);
    assert_eq!(output.code, Some(2), "stderr: {stderr}");
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty, got {:?}",
        output.stdout
    );
    assert!(stderr.contains(cause), "stderr lacks {cause:?}: {stderr}");
    assert!(
        stderr.contains("--help"),
        "stderr lacks usage guidance: {stderr}"
    );
}

// --- 1.5 〜 1.9: 引数エラーは標準出力を汚さず、原因を標準エラーに書いて終了コード 2 ---

#[test]
fn missing_program_path_is_usage_error() {
    let output = run_cli(&os(&[]), StdinMode::Null);
    assert_usage_error(&output, "missing program path");
}

#[test]
fn only_flags_without_program_path_is_usage_error() {
    let output = run_cli(&os(&["--dump-state"]), StdinMode::Null);
    assert_usage_error(&output, "missing program path");
}

#[test]
fn two_program_paths_are_usage_error() {
    let args = [hello(), echo()];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "unexpected extra argument");
}

#[test]
fn unknown_option_is_usage_error() {
    let args = [hello(), OsString::from("--frobnicate")];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "unknown option");
}

#[test]
fn input_option_without_value_is_usage_error() {
    let args = [hello(), OsString::from("--input")];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "missing value for --input");
}

#[test]
fn input_option_with_empty_inline_value_is_usage_error() {
    let args = [hello(), OsString::from("--input=")];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "missing value for --input");
}

#[test]
fn max_steps_option_without_value_is_usage_error() {
    let args = [hello(), OsString::from("--max-steps")];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "missing value for --max-steps");
}

#[test]
fn duplicate_input_option_is_usage_error() {
    let args = [
        hello(),
        OsString::from("--input"),
        echo_input_path().into_os_string(),
        with_prefix("--input=", &echo_input_path()),
    ];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "option --input specified more than once");
}

#[test]
fn duplicate_max_steps_option_is_usage_error() {
    let args = [
        OsString::from("--max-steps=10"),
        OsString::from("--max-steps"),
        OsString::from("10"),
        hello(),
    ];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "option --max-steps specified more than once");
}

#[test]
fn max_steps_and_no_step_limit_together_is_usage_error() {
    let args = [
        OsString::from("--max-steps"),
        OsString::from("5"),
        OsString::from("--no-step-limit"),
        hello(),
    ];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(
        &output,
        "--max-steps and --no-step-limit cannot be used together",
    );
}

// --- 1.3・1.4: 値の形式とオプションの位置によらず同じ意味 ---

#[test]
fn equals_form_and_separate_form_mean_the_same_for_input() {
    let separate = run_cli(
        &[
            OsString::from("--input"),
            echo_input_path().into_os_string(),
            echo(),
        ],
        StdinMode::Null,
    );
    let joined = run_cli(
        &[with_prefix("--input=", &echo_input_path()), echo()],
        StdinMode::Null,
    );

    assert_eq!(separate.code, Some(0), "stderr: {}", stderr_text(&separate));
    assert_eq!(separate.stdout, ECHO_OUTPUT);
    assert!(separate.stderr.is_empty());
    assert_eq!(joined, separate);
}

#[test]
fn input_option_before_or_after_program_path_means_the_same() {
    let input = || echo_input_path().into_os_string();
    let cases = [
        run_cli(
            &[OsString::from("--input"), input(), echo()],
            StdinMode::Null,
        ),
        run_cli(
            &[echo(), OsString::from("--input"), input()],
            StdinMode::Null,
        ),
        run_cli(
            &[with_prefix("--input=", &echo_input_path()), echo()],
            StdinMode::Null,
        ),
        run_cli(
            &[echo(), with_prefix("--input=", &echo_input_path())],
            StdinMode::Null,
        ),
    ];

    for output in &cases {
        assert_eq!(output.code, Some(0), "stderr: {}", stderr_text(output));
        assert_eq!(output.stdout, ECHO_OUTPUT);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn max_steps_before_or_after_program_path_means_the_same() {
    // `+++` は 3 命令。上限 3 は成功、上限 2 は上限超過(終了コード 3)になる。
    let plus = write_temp("args-6-1-plus3.bf", b"+++");

    for limit in ["3", "2"] {
        let inline = format!("--max-steps={limit}");
        let before = run_cli(
            &[OsString::from(&inline), plus.clone().into_os_string()],
            StdinMode::Null,
        );
        let after = run_cli(
            &[
                plus.clone().into_os_string(),
                OsString::from("--max-steps"),
                OsString::from(limit),
            ],
            StdinMode::Null,
        );
        assert_eq!(after, before, "limit {limit}");
    }

    let at_limit = run_cli(
        &[
            OsString::from("--max-steps=3"),
            plus.clone().into_os_string(),
        ],
        StdinMode::Null,
    );
    assert_eq!(at_limit.code, Some(0), "stderr: {}", stderr_text(&at_limit));

    let below_limit = run_cli(
        &[OsString::from("--max-steps=2"), plus.into_os_string()],
        StdinMode::Null,
    );
    assert_eq!(below_limit.code, Some(3));
}

// --- 1.11: 値なしフラグの重複は 1 回指定と同じ ---

#[test]
fn repeated_dump_state_behaves_like_single_flag() {
    let single = run_cli(&[OsString::from("--dump-state"), hello()], StdinMode::Null);
    let repeated = run_cli(
        &[
            OsString::from("--dump-state"),
            hello(),
            OsString::from("--dump-state"),
        ],
        StdinMode::Null,
    );

    assert_eq!(single.code, Some(0), "stderr: {}", stderr_text(&single));
    assert_eq!(single.stdout, b"Hello World!\n");
    assert_eq!(stderr_text(&single), "state: pc=106 dp=6 steps=906\n");
    assert_eq!(repeated, single);
}

#[test]
fn repeated_no_step_limit_behaves_like_single_flag() {
    let single = run_cli(
        &[OsString::from("--no-step-limit"), hello()],
        StdinMode::Null,
    );
    let repeated = run_cli(
        &[
            OsString::from("--no-step-limit"),
            OsString::from("--no-step-limit"),
            hello(),
        ],
        StdinMode::Null,
    );

    assert_eq!(single.code, Some(0), "stderr: {}", stderr_text(&single));
    assert_eq!(single.stdout, b"Hello World!\n");
    assert_eq!(repeated, single);
}

// --- 2.1〜2.4: --help は標準エラーへ使用方法を書き、終了コード 0 ---

#[test]
fn help_writes_full_usage_to_stderr_and_exits_zero() {
    let output = run_cli(&os(&["--help"]), StdinMode::Null);
    let stderr = stderr_text(&output);

    assert_eq!(output.code, Some(0), "stderr: {stderr}");
    assert!(output.stdout.is_empty(), "stdout must be empty");
    assert!(stderr.starts_with("Usage: brainfuck-cli"), "{stderr}");
    for needle in [
        "--input",
        "--max-steps",
        "--no-step-limit",
        "--dump-state",
        "--help",
        "10000000",
        "0  Success",
        "2  Usage",
        "3  Runtime",
        "4  I/O",
    ] {
        assert!(stderr.contains(needle), "usage lacks {needle:?}: {stderr}");
    }
}

#[test]
fn help_takes_precedence_over_other_argument_errors() {
    let cases: [&[&str]; 4] = [
        &["--frobnicate", "--help"],
        &["--help", "a.bf", "b.bf"],
        &["--max-steps", "x", "--no-step-limit", "--help"],
        &["--input", "a.bin", "--input", "b.bin", "--help"],
    ];

    for args in cases {
        let output = run_cli(&os(args), StdinMode::Null);
        let stderr = stderr_text(&output);
        assert_eq!(output.code, Some(0), "args {args:?}: {stderr}");
        assert!(output.stdout.is_empty(), "args {args:?}");
        assert!(
            stderr.contains("Usage: brainfuck-cli"),
            "args {args:?}: {stderr}"
        );
    }
}

#[test]
fn help_does_not_read_program_input_or_stdin() {
    // 存在しないプログラム・入力を指定しても読みに行かないため 0 になる。
    // 標準入力は書かず閉じないため、読めば終了せず、テストはタイムアウトで失敗する。
    let missing = run_cli(
        &os(&["--help", MISSING_PROGRAM, "--input", MISSING_INPUT]),
        StdinMode::OpenUnused,
    );
    assert_eq!(missing.code, Some(0), "stderr: {}", stderr_text(&missing));
    assert!(missing.stdout.is_empty());
    assert!(!stderr_text(&missing).contains("failed to read"));

    let with_program = run_cli(&[OsString::from("--help"), hello()], StdinMode::OpenUnused);
    assert_eq!(with_program.code, Some(0));
    assert!(with_program.stdout.is_empty());
}

// --- 1.10: 引数の検証に失敗したらファイルの読み込みもプログラムの実行もしない ---

#[test]
fn argument_errors_do_not_read_program_input_or_stdin() {
    let cases: [(&[&str], &str); 3] = [
        (
            &["--max-steps", "x", MISSING_PROGRAM],
            "invalid value for --max-steps",
        ),
        (&[MISSING_PROGRAM, "--frobnicate"], "unknown option"),
        (
            &["--input", MISSING_INPUT, MISSING_PROGRAM, "extra.bf"],
            "unexpected extra argument",
        ),
    ];

    for (args, cause) in cases {
        // 存在しないパスを読めば終了コード 4 になり、標準入力を読めばこのテストは止まる。
        let output = run_cli(&os(args), StdinMode::OpenUnused);
        assert_usage_error(&output, cause);
        assert!(
            !stderr_text(&output).contains("failed to read"),
            "args {args:?}"
        );
    }
}

// --- 9.6: 引数エラーの診断は指定値・未知のオプション名を含まず、種類だけを示す ---

#[test]
fn argument_diagnostics_do_not_echo_supplied_values() {
    let cases: Vec<(Vec<OsString>, &str, &str)> = vec![
        (
            vec![
                OsString::from("--max-steps"),
                OsString::from("xq9z-secret"),
                hello(),
            ],
            "xq9z",
            "invalid value for --max-steps",
        ),
        (
            vec![OsString::from("--max-steps=xq9z-secret"), hello()],
            "xq9z",
            "invalid value for --max-steps",
        ),
        (
            vec![OsString::from("--frobnicate-q7"), hello()],
            "frobnicate",
            "unknown option",
        ),
        (
            vec![hello(), OsString::from("extra-q7-program.bf")],
            "extra-q7",
            "unexpected extra argument",
        ),
    ];

    for (args, forbidden, cause) in cases {
        let output = run_cli(&args, StdinMode::Null);
        let stderr = stderr_text(&output);
        assert_usage_error(&output, cause);
        assert!(
            !stderr.contains(forbidden),
            "diagnostic must not contain {forbidden:?}: {stderr}"
        );
    }
}

// --- 1.3・1.7・1.10・6.8: 非 UTF-8 の `--name=value` も位置引数にせず、オプションとして扱う ---

/// `prefix` の後ろに UTF-8 にできない 1 単位を付けた引数(Windows では不正サロゲート、Unix では不正バイト)。
#[cfg(any(unix, windows))]
fn non_utf8_after(prefix: &str) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut bytes = prefix.as_bytes().to_vec();
        bytes.push(0xFF);
        OsString::from_vec(bytes)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let mut units: Vec<u16> = prefix.encode_utf16().collect();
        units.push(0xD800);
        OsString::from_wide(&units)
    }
}

#[cfg(any(unix, windows))]
#[test]
fn non_utf8_max_steps_equals_value_is_usage_error_not_program_path() {
    // 読みに行けば終了コード 4 になるプログラムパスを併記し、引数エラーで止まることを確かめる。
    let args = [
        non_utf8_after("--max-steps="),
        OsString::from(MISSING_PROGRAM),
    ];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "invalid value for --max-steps");
}

#[cfg(any(unix, windows))]
#[test]
fn non_utf8_max_steps_equals_value_alone_is_invalid_value_not_missing_program() {
    // プログラムパスが無い場合でも、最初の引数エラー(不正な値)が `missing program path` より優先される。
    // `ParseState::finish` は保持した最初のエラーを、プログラムの欠落より先に返すため。
    // 検証段で止まるので、ファイルの読込(終了コード 4)にも進まない。
    let args = [non_utf8_after("--max-steps=")];
    let output = run_cli(&args, StdinMode::OpenUnused);
    assert_usage_error(&output, "invalid value for --max-steps");
    let stderr = stderr_text(&output);
    assert!(!stderr.contains("missing program path"), "{stderr}");
    assert!(!stderr.contains("program file"), "{stderr}");
    assert!(!stderr.contains("failed to read"), "{stderr}");
    assert_ne!(output.code, Some(4), "stderr: {stderr}");
}

#[cfg(any(unix, windows))]
#[test]
fn non_utf8_max_steps_equals_value_with_valid_program_does_not_run_it() {
    // 実在するプログラムを併記しても、引数エラーのため実行されず `Hello World!` は出ない。
    let args = [non_utf8_after("--max-steps="), hello()];
    let output = run_cli(&args, StdinMode::OpenUnused);
    assert_usage_error(&output, "invalid value for --max-steps");
    assert!(
        !output.stdout.windows(5).any(|window| window == b"Hello"),
        "program must not run: {:?}",
        output.stdout
    );
}

#[cfg(any(unix, windows))]
#[test]
fn non_utf8_unknown_equals_option_is_usage_error_not_program_path() {
    let args = [non_utf8_after("--frob="), OsString::from(MISSING_PROGRAM)];
    let output = run_cli(&args, StdinMode::Null);
    assert_usage_error(&output, "unknown option");
}

#[cfg(any(unix, windows))]
#[test]
fn non_utf8_input_equals_value_is_accepted_as_path() {
    // 非 UTF-8 のパスは値として受理され、存在しないためファイルの読込で I/O エラー(4)になる。
    let args = [non_utf8_after("--input=no-such-dir-6-1/"), hello()];
    let output = run_cli(&args, StdinMode::Null);
    assert_eq!(output.code, Some(4), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty());
    assert!(
        stderr_text(&output).contains("input file"),
        "{}",
        stderr_text(&output)
    );
}
