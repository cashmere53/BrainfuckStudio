//! ソース処理の結合テスト(3.x)。実バイナリを起動し、標準出力・標準エラー・終了コードを検証する。
//!
//! - 括弧エラーの診断が、UTF-8 として不正なバイトや日本語(UTF-8)を含むコメントを挟んでも、
//!   元ファイルの実バイト位置(ソースバイトオフセット)を示すこと(3.3)
//! - 括弧エラーでは命令を 1 個も実行せず、標準出力は空で終了コード 2 になること(3.5)
//! - 空ファイル・コメントのみのファイルは標準出力・標準エラー空で終了コード 0 になること(3.4)
//! - 存在しないソース・ディレクトリのソースは、終了コード 4 で、プログラムファイルを対象とする診断になり、
//!   パスを含まないこと(3.6, 9.3, 9.4)
//! - 括弧エラーと入力ファイル不在が同時の場合、段の優先順位により終了コード 2 になること(3.7)

mod common;

use std::ffi::OsStr;

use common::{fixture_path, run_cli, write_temp, CliOutput, StdinMode};

fn stderr_text(output: &CliOutput) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 括弧エラーの期待: 終了コード 2、標準出力空、標準エラーに各断片を含む。
fn assert_bracket_error(output: &CliOutput, expected_fragments: &[&str]) {
    assert_eq!(output.code, Some(2), "stderr: {}", stderr_text(output));
    assert!(
        output.stdout.is_empty(),
        "no instruction may run, stdout: {:?}",
        output.stdout
    );
    let stderr = stderr_text(output);
    for fragment in expected_fragments {
        assert!(stderr.contains(fragment), "stderr: {stderr}");
    }
}

#[test]
fn closing_bracket_after_invalid_utf8_comment_reports_real_byte_offset() {
    // 0xFF はコメント(1 バイト)。`+` `.` `]` の `]` はファイル先頭から 3 バイト目(offset 3)、
    // 命令添字は 2(`+` が 0、`.` が 1)。`.` は実行されれば出力されるため、標準出力空で未実行を確かめる。
    let source = write_temp("closing-invalid-utf8.bf", &[0xFF, b'+', b'.', b']']);
    let output = run_cli(&[source.as_os_str()], StdinMode::OpenUnused);

    assert_bracket_error(
        &output,
        &[
            "unmatched closing bracket",
            "instruction #2",
            "source offset 3",
        ],
    );
}

#[test]
fn closing_bracket_after_japanese_utf8_comment_reports_real_byte_offset() {
    // 0xFF(1 バイト) + 「日本語」(UTF-8 で 9 バイト)の後ろの `]` は offset 1 + 9 + 2 = 12。
    // 命令添字は 2。
    let mut bytes = vec![0xFF];
    bytes.extend_from_slice("日本語".as_bytes());
    bytes.extend_from_slice(b"+.]");
    let source = write_temp("closing-japanese.bf", &bytes);
    let output = run_cli(&[source.as_os_str()], StdinMode::OpenUnused);

    assert_bracket_error(
        &output,
        &[
            "unmatched closing bracket",
            "instruction #2",
            "source offset 12",
        ],
    );
}

#[test]
fn opening_bracket_after_non_utf8_and_japanese_comments_reports_real_byte_offset() {
    // 0x80 0xFF(各 1 バイト) + 「日本語」(9 バイト) の後ろの `+` は offset 11、`[` は offset 12、
    // 命令添字は 1(`+` が 0)。`.` は実行されないこと(標準出力空)を確かめる。
    let mut bytes = vec![0x80, 0xFF];
    bytes.extend_from_slice("日本語".as_bytes());
    bytes.extend_from_slice(b"+[.");
    let source = write_temp("opening-mixed.bf", &bytes);
    let output = run_cli(&[source.as_os_str()], StdinMode::OpenUnused);

    assert_bracket_error(
        &output,
        &[
            "unmatched opening bracket",
            "instruction #1",
            "source offset 12",
        ],
    );
}

#[test]
fn empty_source_file_exits_zero_with_no_output() {
    let source = write_temp("empty-source.bf", b"");
    let output = run_cli(&[source.as_os_str()], StdinMode::OpenUnused);

    assert_eq!(output.code, Some(0), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn comment_only_source_file_exits_zero_with_no_output() {
    // UTF-8 BOM、0xFF / 0xFE、日本語、改行のみ。すべてコメント扱いで命令 0 個。
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&[0xFF, 0xFE, b'\n']);
    bytes.extend_from_slice("これはコメントだけ".as_bytes());
    bytes.extend_from_slice(b"\r\n");
    let source = write_temp("comment-only.bf", &bytes);
    let output = run_cli(&[source.as_os_str()], StdinMode::OpenUnused);

    assert_eq!(output.code, Some(0), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn missing_source_file_exits_four_and_names_program_file_without_path() {
    let missing = fixture_path("fixtures/no-such-source-6-2.bf");
    let output = run_cli(&[missing.as_os_str()], StdinMode::OpenUnused);

    assert_eq!(output.code, Some(4), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty());
    let stderr = stderr_text(&output);
    assert!(stderr.contains("program file"), "stderr: {stderr}");
    assert!(!stderr.contains("no-such-source-6-2"), "stderr: {stderr}");
    assert!(
        !stderr.contains(&*missing.to_string_lossy()),
        "stderr: {stderr}"
    );
}

#[test]
fn directory_as_source_exits_four_and_names_program_file_without_path() {
    let directory = fixture_path("fixtures");
    let output = run_cli(&[directory.as_os_str()], StdinMode::OpenUnused);

    assert_eq!(output.code, Some(4), "stderr: {}", stderr_text(&output));
    assert!(output.stdout.is_empty());
    let stderr = stderr_text(&output);
    assert!(stderr.contains("program file"), "stderr: {stderr}");
    assert!(
        !stderr.contains(&*directory.to_string_lossy()),
        "stderr: {stderr}"
    );
}

#[test]
fn bracket_error_takes_precedence_over_missing_input_file() {
    // 段の順序は ソース読込 → 解析 → 入力取得。解析で失敗するため入力ファイル不在は評価されず 2 になる。
    let source = write_temp("unbalanced-with-missing-input.bf", b"+]");
    let missing_input = fixture_path("fixtures/no-such-input-6-2.bin");
    let args = [
        source.as_os_str(),
        OsStr::new("--input"),
        missing_input.as_os_str(),
    ];
    let output = run_cli(&args, StdinMode::OpenUnused);

    assert_bracket_error(&output, &["unmatched closing bracket"]);
    assert!(
        !stderr_text(&output).contains("input file"),
        "stderr: {}",
        stderr_text(&output)
    );
}
