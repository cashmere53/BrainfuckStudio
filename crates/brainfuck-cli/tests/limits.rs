//! 命令数上限(6.x)と状態表示 `--dump-state`(7.x)の結合テスト。実バイナリを起動して検証する。
//! 重い実行(既定上限への到達)はこのファイルの 1 件だけに限る。

mod common;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use common::{example_path, run_cli, write_temp, StdinMode};

/// 既定の命令数上限(requirements 6.1)。
const DEFAULT_LIMIT: &str = "10000000";

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `+++` を書いた一時ファイルのパス(命令 3 個、正常停止する)。
fn plus3() -> PathBuf {
    write_temp("limits-plus3.bf", b"+++")
}

/// `+[]` を書いた一時ファイル(既定上限に達するまで停止しない)。
fn plus_loop() -> PathBuf {
    write_temp("limits-plus-loop.bf", b"+[]")
}

/// 引数列を組み立てる補助。
fn args(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

// ---- 6.1 既定の上限・6.6 診断・7.1 状態表示(重い実行は本ファイルでこの 1 件のみ) ----

#[test]
fn default_limit_stops_infinite_loop_with_exit_3_and_diagnostic() {
    let program = plus_loop();
    let output = run_cli(
        &[program.as_os_str(), "--dump-state".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(3), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains(&format!("limit={DEFAULT_LIMIT}")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("steps={DEFAULT_LIMIT}")),
        "{stderr}"
    );
}

#[test]
fn default_limit_dump_state_reports_exact_limit_steps() {
    let program = plus_loop();
    let output = run_cli(
        &[program.as_os_str(), "--dump-state".as_ref()],
        StdinMode::Null,
    );

    let stderr = text(&output.stderr);
    let state_line = stderr
        .lines()
        .find(|line| line.starts_with("state: "))
        .unwrap_or_else(|| panic!("state line missing: {stderr}"));
    assert!(
        state_line.ends_with(&format!(" steps={DEFAULT_LIMIT}")),
        "{state_line}"
    );
}

// ---- 6.2 / 6.3 / 6.4 `--max-steps <N>` の境界 ----

#[test]
fn max_steps_exactly_matching_program_length_succeeds() {
    let program = plus3();
    let output = run_cli(
        &[program.as_os_str(), "--max-steps".as_ref(), "3".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(0), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn max_steps_equals_form_is_accepted() {
    let program = plus3();
    let output = run_cli(
        &[program.as_os_str(), "--max-steps=3".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(0), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn max_steps_below_program_length_stops_with_exit_3_and_diagnostic() {
    let program = plus3();
    let output = run_cli(
        &[program.as_os_str(), "--max-steps".as_ref(), "2".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(3), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("limit=2"), "{stderr}");
    assert!(stderr.contains("pc=2"), "{stderr}");
}

#[test]
fn max_steps_zero_with_nonempty_program_executes_nothing() {
    let program = plus3();
    let output = run_cli(
        &[
            program.as_os_str(),
            "--max-steps".as_ref(),
            "0".as_ref(),
            "--dump-state".as_ref(),
        ],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(3), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("limit=0"), "{stderr}");
    assert!(stderr.contains("pc=0"), "{stderr}");
    // 1 命令も実行していないので、状態表示は dp=0 steps=0 のまま。
    assert!(stderr.ends_with("state: pc=0 dp=0 steps=0\n"), "{stderr}");
}

#[test]
fn max_steps_zero_with_empty_program_succeeds() {
    let program = write_temp("limits-empty.bf", b"");
    let output = run_cli(
        &[program.as_os_str(), "--max-steps".as_ref(), "0".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(0), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

// ---- 6.5 `--no-step-limit` ----

#[test]
fn no_step_limit_lets_finite_program_finish_normally() {
    let hello = example_path("hello.bf");
    let output = run_cli(
        &[hello.as_os_str(), "--no-step-limit".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(0), "stderr: {}", text(&output.stderr));
    assert_eq!(output.stdout, b"Hello World!\n");
    assert!(output.stderr.is_empty());
}

// ---- 6.7 `--max-steps` と `--no-step-limit` の併用 ----

#[test]
fn max_steps_with_no_step_limit_is_usage_error_in_either_order() {
    let program = plus3();
    for order in [
        args(&["--max-steps", "3", "--no-step-limit"]),
        args(&["--no-step-limit", "--max-steps", "3"]),
    ] {
        let mut argv: Vec<OsString> = vec![program.clone().into_os_string()];
        argv.extend(order);
        let output = run_cli(&argv, StdinMode::Null);

        assert_eq!(output.code, Some(2), "stderr: {}", text(&output.stderr));
        assert!(output.stdout.is_empty());
        let stderr = text(&output.stderr);
        assert!(stderr.contains("--max-steps"), "{stderr}");
        assert!(stderr.contains("--no-step-limit"), "{stderr}");
    }
}

// ---- 6.8 `--max-steps` の不正値 ----

#[test]
fn invalid_max_steps_values_are_usage_errors_without_echoing_the_value() {
    let program = plus3();
    // 空、符号付き、非数字、表現できる最大値(u64::MAX)を超える値。
    let invalid_values = ["", "-1", "+3", "abc", "18446744073709551616"];

    for value in invalid_values {
        let output = run_cli(
            &[program.as_os_str(), "--max-steps".as_ref(), value.as_ref()],
            StdinMode::Null,
        );

        assert_eq!(
            output.code,
            Some(2),
            "value {value:?}, stderr: {}",
            text(&output.stderr)
        );
        assert!(output.stdout.is_empty(), "value {value:?}");
        let stderr = text(&output.stderr);
        assert!(stderr.contains("--max-steps"), "value {value:?}: {stderr}");
        if !value.is_empty() {
            assert!(
                !stderr.contains(value),
                "diagnostic must not echo value {value:?}: {stderr}"
            );
        }
    }
}

// ---- 7.1 / 7.2 / 7.4 `--dump-state` の表示条件 ----

#[test]
fn dump_state_on_normal_stop_prints_pc_dp_steps_to_stderr() {
    let program = plus3();
    let output = run_cli(
        &[program.as_os_str(), "--dump-state".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(0), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    assert_eq!(text(&output.stderr), "state: pc=3 dp=0 steps=3\n");
}

#[test]
fn dump_state_on_step_limit_prints_interrupted_values() {
    let program = plus3();
    let output = run_cli(
        &[
            program.as_os_str(),
            "--max-steps".as_ref(),
            "2".as_ref(),
            "--dump-state".as_ref(),
        ],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(3), "stderr: {}", text(&output.stderr));
    let stderr = text(&output.stderr);
    assert!(stderr.ends_with("state: pc=2 dp=0 steps=2\n"), "{stderr}");
}

#[test]
fn dump_state_on_runtime_error_prints_interrupted_values() {
    // `+` は 1 命令実行できるが、続く `<` はデータポインタを範囲外へ動かして実行エラーになる。
    let program = write_temp("limits-plus-left.bf", b"+<");
    let output = run_cli(
        &[program.as_os_str(), "--dump-state".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(3), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("direction=left"), "{stderr}");
    assert!(stderr.ends_with("state: pc=1 dp=0 steps=1\n"), "{stderr}");
}

#[test]
fn dump_state_is_not_printed_when_not_requested() {
    let program = plus3();
    let normal = run_cli(&[program.as_os_str()], StdinMode::Null);
    assert_eq!(normal.code, Some(0));
    assert!(normal.stderr.is_empty());

    let limited = run_cli(
        &[program.as_os_str(), "--max-steps".as_ref(), "2".as_ref()],
        StdinMode::Null,
    );
    assert_eq!(limited.code, Some(3));
    assert!(!text(&limited.stderr).contains("state:"));
}

// ---- 7.3 `--dump-state` があっても実行に至らない場合は状態表示をしない ----

#[test]
fn dump_state_is_not_printed_on_argument_error() {
    let program = plus3();
    let output = run_cli(
        &[
            program.as_os_str(),
            "--dump-state".as_ref(),
            "--max-steps".as_ref(),
            "3".as_ref(),
            "--no-step-limit".as_ref(),
        ],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(2), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    assert!(!text(&output.stderr).contains("state:"));
}

#[test]
fn dump_state_is_not_printed_on_parse_error() {
    let program = write_temp("limits-unmatched.bf", b"+[");
    let output = run_cli(
        &[program.as_os_str(), "--dump-state".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(2), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    assert!(!text(&output.stderr).contains("state:"));
}

#[test]
fn dump_state_is_not_printed_on_source_read_error() {
    // 作成しないファイル名を指定し、存在しないソースを再現する。
    let absent = Path::new(env!("CARGO_TARGET_TMPDIR")).join("limits-never-created-absent.bf");
    let output = run_cli(
        &[absent.as_os_str(), "--dump-state".as_ref()],
        StdinMode::Null,
    );

    assert_eq!(output.code, Some(4), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    assert!(!text(&output.stderr).contains("state:"));
}
