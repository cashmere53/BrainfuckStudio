//! 終了コードと診断の機微情報除外の結合テスト(実バイナリを起動する)。
//!
//! 検証する契約(要件 3.6, 8.1–8.7, 9.1–9.7, 12.1, 12.4, 12.7):
//! - 代表ケースで終了コード 0 / 2 / 3 / 4 に到達し、終了コードは常にこの 4 つのいずれか
//! - 出力先の振り分け(プログラム出力は標準出力のみ、診断は標準エラーのみ)
//! - 診断に、指定したパス・親ディレクトリ名・カレントディレクトリ・不正な値・ソース全文・入力/出力バイト列を含めない
//! - 同一の引数・入力で複数回実行しても、標準出力・標準エラー・終了コードが毎回一致する

mod common;

use std::env;
use std::path::{Path, PathBuf};
use std::process;

use common::{
    example_path, fixture_path, run_cli, run_cli_closed_stdout, write_temp, CliOutput, StdinMode,
};

/// 要件 8.7: CLI が返しうる終了コードの全集合。
const ALLOWED_CODES: [i32; 4] = [0, 2, 3, 4];

fn stderr_text(output: &CliOutput) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `needle` のバイト列が `haystack` の部分列として現れるか。空の `needle` は常に含まれないものとして扱う。
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// 指定の文字列(ASCII のパス・名前等)が標準エラーに含まれないことを確かめる。
fn assert_stderr_lacks(output: &CliOutput, needle: &str) {
    let stderr = stderr_text(output);
    assert!(
        !stderr.contains(needle),
        "diagnostic must not contain {needle:?}, stderr: {stderr}"
    );
}

/// 存在しない絶対パスを返す。親ディレクトリも存在しないため、どの環境でも読み込みは失敗する。
fn missing_path(file_name: &str) -> (PathBuf, String) {
    let parent_name = format!("exit-codes-no-such-dir-{}", process::id());
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(&parent_name)
        .join(file_name);
    assert!(path.is_absolute(), "test path must be absolute");
    (path, parent_name)
}

#[test]
fn hello_example_exits_0_with_program_output_only() {
    let hello = example_path("hello.bf");
    let output = run_cli(&[hello.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(0), "stderr: {}", stderr_text(&output));
    assert_eq!(output.stdout, b"Hello World!\n");
    assert!(output.stderr.is_empty(), "stderr: {}", stderr_text(&output));
}

#[test]
fn unmatched_closing_bracket_exits_2_with_index_and_offset() {
    // 先頭 3 バイトはコメント。`]` は命令添字 1(`+` が 0)、ソースバイトオフセット 3。
    let source = write_temp("unmatched-close.bf", b"zz+]");
    let output = run_cli(&[source.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(2), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    let stderr = stderr_text(&output);
    assert!(stderr.contains("closing bracket"), "stderr: {stderr}");
    assert!(
        stderr.contains("#1"),
        "instruction index missing, stderr: {stderr}"
    );
    assert!(
        stderr.contains("offset 3"),
        "source offset missing, stderr: {stderr}"
    );
}

#[test]
fn unmatched_opening_bracket_exits_2_with_empty_stdout() {
    let source = write_temp("unmatched-open.bf", b"+[+");
    let output = run_cli(&[source.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(2), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    let stderr = stderr_text(&output);
    assert!(stderr.contains("opening bracket"), "stderr: {stderr}");
    assert!(
        stderr.contains("#1"),
        "instruction index missing, stderr: {stderr}"
    );
    assert!(
        stderr.contains("offset 1"),
        "source offset missing, stderr: {stderr}"
    );
}

#[test]
fn argument_errors_exit_2_with_empty_stdout() {
    // 未知のオプション。
    let hello = example_path("hello.bf");
    let unknown = run_cli(
        &["--no-such-option", hello.to_str().unwrap()],
        StdinMode::Null,
    );
    assert_eq!(unknown.code, Some(2), "stderr: {}", stderr_text(&unknown));
    assert!(unknown.stdout.is_empty(), "stdout must be empty");
    assert!(!unknown.stderr.is_empty(), "diagnostic must be on stderr");

    // プログラムパスの欠落。
    let missing = run_cli::<&str>(&[], StdinMode::Null);
    assert_eq!(missing.code, Some(2), "stderr: {}", stderr_text(&missing));
    assert!(missing.stdout.is_empty(), "stdout must be empty");
    assert!(!missing.stderr.is_empty(), "diagnostic must be on stderr");
}

#[test]
fn pointer_out_of_bounds_exits_3_with_partial_output_and_state_in_diagnostic() {
    // `+++.` で出力 0x03 を書いてから `<` で dp=0 のまま左へ移動し範囲外になる。
    let source = write_temp("pointer-left.bf", b"+++.<");
    let output = run_cli(&[source.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(3), "stderr: {}", stderr_text(&output));
    assert_eq!(
        output.stdout,
        vec![0x03],
        "partial output must reach stdout"
    );
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("out of bounds"),
        "cause missing, stderr: {stderr}"
    );
    assert!(stderr.contains("pc=4"), "pc missing, stderr: {stderr}");
    assert!(stderr.contains("dp=0"), "dp missing, stderr: {stderr}");
    assert!(
        stderr.contains("direction=left"),
        "direction missing, stderr: {stderr}"
    );
}

#[test]
fn missing_source_absolute_path_exits_4_without_leaking_path_or_cwd() {
    let (path, parent_name) = missing_path("missing-source.bf");
    let path_text = path.to_string_lossy().into_owned();
    let cwd = env::current_dir().expect("current dir must be readable");
    let cwd_text = cwd.to_string_lossy().into_owned();

    let output = run_cli(&[path.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(4), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    assert!(!output.stderr.is_empty(), "diagnostic must be on stderr");
    assert_stderr_lacks(&output, &path_text);
    assert_stderr_lacks(&output, "missing-source.bf");
    assert_stderr_lacks(&output, &parent_name);
    assert_stderr_lacks(&output, &cwd_text);
}

#[test]
fn missing_input_absolute_path_exits_4_without_leaking_path() {
    let hello = example_path("hello.bf");
    let (input, parent_name) = missing_path("missing-input.bin");
    let input_text = input.to_string_lossy().into_owned();

    let output = run_cli(
        &[
            hello.as_os_str().to_owned(),
            "--input".into(),
            input.as_os_str().to_owned(),
        ],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(4), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    assert!(!output.stderr.is_empty(), "diagnostic must be on stderr");
    assert_stderr_lacks(&output, &input_text);
    assert_stderr_lacks(&output, "missing-input.bin");
    assert_stderr_lacks(&output, &parent_name);
}

#[test]
fn directory_as_source_exits_4_with_empty_stdout() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = run_cli(&[dir.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(4), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    assert!(!output.stderr.is_empty(), "diagnostic must be on stderr");
    assert_stderr_lacks(&output, &dir.to_string_lossy());
}

#[test]
fn closed_stdout_exits_4_and_diagnostic_targets_standard_output() {
    // 出力が止まらないソースを使う。読み側を閉じる前に書き終えると成功(終了コード 0)してしまうため、
    // パイプの容量を超える出力で、読み側が閉じるまで子の書き込みを待たせる。
    let source = write_temp("endless-output.bf", b"+[.]");
    let output = run_cli_closed_stdout(&[source.as_os_str()]);

    assert_eq!(output.code, Some(4), "stderr: {}", stderr_text(&output));
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("standard output"),
        "target missing, stderr: {stderr}"
    );
    assert!(!stderr.contains("standard input"), "stderr: {stderr}");
    assert_stderr_lacks(&output, "Hello World!");
}

#[test]
fn invalid_max_steps_exits_2_without_echoing_value_or_cwd() {
    let hello = example_path("hello.bf");
    let bad_value = "bogus-VALUE-4242";
    let cwd = env::current_dir().expect("current dir must be readable");

    let output = run_cli(
        &[
            hello.as_os_str().to_owned(),
            "--max-steps".into(),
            bad_value.into(),
        ],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(2), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty(), "stdout must be empty");
    assert!(!output.stderr.is_empty(), "diagnostic must be on stderr");
    assert_stderr_lacks(&output, bad_value);
    assert_stderr_lacks(&output, &cwd.to_string_lossy());
}

#[test]
fn diagnostics_exclude_source_text_and_io_bytes() {
    // 出力 = 入力 9 バイト。ソースには識別可能なコメント文字列を含める。
    let input = b"INMARK123";
    let source = write_temp("secret-source.bf", b"SRCMARK,.,.,.,.,.,.,.,.,.<");
    let output = run_cli(&[source.as_os_str()], StdinMode::Bytes(input.to_vec()));

    assert_eq!(output.code, Some(3), "stderr: {}", stderr_text(&output));
    assert_eq!(output.stdout, input, "program output must reach stdout");
    assert!(!output.stderr.is_empty(), "diagnostic must be on stderr");
    assert!(
        !contains_bytes(&output.stderr, b"SRCMARK"),
        "source text leaked"
    );
    assert!(!contains_bytes(&output.stderr, input), "input bytes leaked");
    assert!(
        !contains_bytes(&output.stderr, &output.stdout),
        "output bytes leaked"
    );
}

#[test]
fn repeated_runs_produce_identical_stdout_stderr_and_code() {
    let source = write_temp("repeat-pointer.bf", b"+++.<");

    let first = run_cli(&[source.as_os_str()], StdinMode::Null);
    for _ in 0..2 {
        let again = run_cli(&[source.as_os_str()], StdinMode::Null);
        assert_eq!(again, first, "every run must match the first run");
    }
    assert_eq!(first.code, Some(3));
}

#[test]
fn exit_codes_observed_are_only_0_2_3_or_4() {
    let hello = example_path("hello.bf");
    let echo = fixture_path("fixtures/echo.bf");
    let observed = [
        run_cli(&[hello.as_os_str()], StdinMode::Null).code,
        run_cli(&[echo.as_os_str()], StdinMode::Bytes(b"A".to_vec())).code,
        run_cli::<&str>(&[], StdinMode::Null).code,
    ];
    for code in observed {
        let code = code.expect("process must exit with a code");
        assert!(ALLOWED_CODES.contains(&code), "unexpected exit code {code}");
    }
}
