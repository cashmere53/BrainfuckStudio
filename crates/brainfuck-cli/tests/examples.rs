//! サンプル・README・core との一致の結合テスト(12.1, 12.2, 10.1〜10.5, 11.2)。
//! 実バイナリを起動して、`examples/hello.bf` の出力・終了コード、`--dump-state` の回帰オラクル、
//! README の検証手順・オプション表・終了コード表を確かめる。さらに同じプログラムと入力を
//! core を直接実行した結果(出力バイト列、実行済み命令数、位置、エラー種別)と CLI の結果を比べる。

mod common;

use std::ffi::{OsStr, OsString};
use std::fs;

use brainfuck_core::{Machine, MachineConfig, Program, StopReason};
use common::{example_path, fixture_path, run_cli, write_temp, StdinMode};

/// README を実行可能な文書の正として取り込む(10.4, 10.5)。
const README: &str = include_str!("../../../README.md");

#[test]
fn hello_example_prints_exactly_hello_world_with_empty_stderr() {
    let hello = example_path("hello.bf");
    let output = run_cli(&[hello.as_os_str()], StdinMode::Null);

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, b"Hello World!\n");
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn hello_example_finishes_with_open_unused_stdin() {
    // hello.bf は入力命令を含まないため、書かず閉じない標準入力でも時間内に終了する。
    let hello = example_path("hello.bf");
    let output = run_cli(&[hello.as_os_str()], StdinMode::OpenUnused);

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, b"Hello World!\n");
}

#[test]
fn dump_state_for_hello_matches_regression_oracle() {
    // 回帰オラクル: 既定オプションの hello の実行数は 906、停止位置は pc=106 dp=6。
    let hello = example_path("hello.bf");
    let output = run_cli(
        &[hello.as_os_str(), OsStr::new("--dump-state")],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout, b"Hello World!\n");
    assert_eq!(output.stderr, b"state: pc=106 dp=6 steps=906\n");
}

#[test]
fn readme_contains_both_verification_commands() {
    assert!(
        README.contains("cargo test --workspace"),
        "README must mention cargo test --workspace"
    );
    assert!(
        README.contains("cargo run -p brainfuck-cli -- examples/hello.bf"),
        "README must mention the hello run command"
    );
}

#[test]
fn readme_option_table_lists_every_option() {
    for option in [
        "--input",
        "--max-steps",
        "--no-step-limit",
        "--dump-state",
        "--help",
    ] {
        let row_prefix = format!("| `{option}");
        assert!(
            README.lines().any(|line| line.starts_with(&row_prefix)),
            "README option table must have a row for {option}"
        );
    }
}

#[test]
fn readme_exit_code_table_lists_0_2_3_and_4() {
    for code in ["0", "2", "3", "4"] {
        let row_prefix = format!("| {code} |");
        assert!(
            README.lines().any(|line| line.starts_with(&row_prefix)),
            "README exit code table must have a row for {code}"
        );
    }
}

#[test]
fn cli_agrees_with_core_on_hello() {
    let source = fs::read(example_path("hello.bf")).expect("hello.bf must be readable");
    assert_cli_matches_core("hello.bf", &source, b"", None);
}

#[test]
fn cli_agrees_with_core_on_echo_with_stdin_input() {
    let source = fs::read(fixture_path("fixtures/echo.bf")).expect("echo.bf must be readable");
    let input = fs::read(fixture_path("fixtures/input.bin")).expect("input.bin must be readable");
    assert_cli_matches_core("echo.bf", &source, &input, None);
}

#[test]
fn cli_agrees_with_core_on_echo_stopping_at_end_of_input() {
    let source = fs::read(fixture_path("fixtures/echo.bf")).expect("echo.bf must be readable");
    assert_cli_matches_core("echo-eof.bf", &source, b"AB", None);
}

#[test]
fn cli_agrees_with_core_on_pointer_out_of_bounds_after_partial_output() {
    assert_cli_matches_core("range.bf", b"+.<", b"", None);
}

#[test]
fn cli_agrees_with_core_on_step_limit_exceeded() {
    assert_cli_matches_core("limit.bf", b"+[]", b"", Some(1000));
}

#[test]
fn cli_agrees_with_core_on_step_limit_boundary() {
    assert_cli_matches_core("boundary-ok.bf", b"+++", b"", Some(3));
    assert_cli_matches_core("boundary-ng.bf", b"+++", b"", Some(2));
}

#[test]
fn cli_agrees_with_core_on_unmatched_closing_bracket() {
    assert_cli_matches_core("close.bf", b"+]", b"", None);
}

#[test]
fn cli_agrees_with_core_on_unmatched_opening_bracket() {
    assert_cli_matches_core("open.bf", b"+[", b"", None);
}

#[test]
fn cli_agrees_with_core_on_empty_program() {
    assert_cli_matches_core("empty.bf", b"", b"", None);
}

/// 同じソース・入力・上限を core で直接実行した結果と、CLI の結果が一致することを確かめる。
/// 比較対象は、標準出力のバイト列、`--dump-state` の pc・dp・steps、終了コード、
/// 実行エラーまたは解析エラーの core の説明文(表示文)。
fn assert_cli_matches_core(name: &str, source: &[u8], input: &[u8], max_steps: Option<u64>) {
    let path = write_temp(name, source);
    let mut args: Vec<OsString> = vec![path.clone().into_os_string(), "--dump-state".into()];
    if let Some(limit) = max_steps {
        args.push("--max-steps".into());
        args.push(limit.to_string().into());
    }

    match Program::parse_bytes(source) {
        Err(error) => {
            let output = run_cli(args.as_slice(), StdinMode::Bytes(input.to_vec()));
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            assert_eq!(output.code, Some(2), "{name}: stderr: {stderr}");
            assert!(
                output.stdout.is_empty(),
                "{name}: stdout must be empty on parse error"
            );
            assert!(
                stderr.contains(&format!("error: parse error: {error}")),
                "{name}: stderr must embed core's message `{error}`: {stderr}"
            );
        }
        Ok(program) => {
            let config = match max_steps {
                Some(limit) => MachineConfig::with_max_steps(limit),
                None => MachineConfig::unlimited(),
            };
            let mut machine = Machine::new(program, input.to_vec(), config);
            let result = machine.run();
            let snapshot = machine.snapshot();

            let (expected_code, expected_error) = match result {
                Ok(StopReason::Halted) => (0, None),
                Ok(other) => panic!("{name}: unexpected stop reason {other:?}"),
                Err(error) => (3, Some(format!("error: runtime error: {error}"))),
            };

            let output = run_cli(args.as_slice(), StdinMode::Bytes(input.to_vec()));
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            assert_eq!(output.code, Some(expected_code), "{name}: stderr: {stderr}");
            assert_eq!(
                output.stdout,
                machine.output(),
                "{name}: stdout must match core output"
            );

            let state = format!(
                "state: pc={} dp={} steps={}\n",
                snapshot.pc, snapshot.data_pointer, snapshot.steps_executed
            );
            match expected_error {
                None => assert_eq!(output.stderr, state.as_bytes(), "{name}: state line"),
                Some(message) => {
                    assert!(
                        stderr.contains(&message),
                        "{name}: stderr must embed core's message `{message}`: {stderr}"
                    );
                    assert!(
                        stderr.ends_with(&state),
                        "{name}: state line must follow: {stderr}"
                    );
                }
            }
        }
    }
}
