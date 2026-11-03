//! プログラムの実行と結果の整理。
//!
//! 解析済みプログラム・入力バイト列・解決済みの命令数上限から core の機械を生成し、
//! `run` を 1 回だけ呼ぶ。実行意味論と上限判定は core に委ね、ここでは再実装しない。
//! 出力・スナップショット・判定は [`RunReport`] に束ね、後続の出力段へ渡す。

// 後続タスク(4.2 以降)で実行経路に接続するまで未使用のため許可する。
#![allow(dead_code)]

use std::ffi::OsString;

use brainfuck_core::{Machine, MachineConfig, MachineSnapshot, Program, RuntimeError, StopReason};

use crate::args::{parse_args, Command, RunOptions, USAGE};
use crate::error::{CliError, EXIT_SUCCESS};
use crate::input::acquire_input;
use crate::source::{parse_source, read_source_file};

/// 実行の結果を束ねた値。成否にかかわらず、取得できた出力とスナップショットを保持する。
#[derive(Debug)]
struct RunReport {
    /// プログラムが出力したバイト列(実行エラー時は部分出力)。
    output: Vec<u8>,
    /// 停止時(または中断時)の機械の状態。
    snapshot: MachineSnapshot,
    /// 正常停止なら `Ok(())`、それ以外は分類済みのエラー。
    result: Result<(), CliError>,
}

/// 機械を生成して 1 回だけ実行し、出力・スナップショット・判定を返す。
///
/// `max_steps` が `Some(n)` なら上限 `n`、`None` なら制限なしで実行する。
fn run_program(program: Program, input: Vec<u8>, max_steps: Option<u64>) -> RunReport {
    let config = match max_steps {
        Some(limit) => MachineConfig::with_max_steps(limit),
        None => MachineConfig::unlimited(),
    };
    let mut machine = Machine::new(program, input, config);
    let outcome = machine.run();
    RunReport {
        output: machine.output().to_vec(),
        snapshot: machine.snapshot(),
        result: classify_stop(outcome),
    }
}

/// `run` の戻り値を CLI の判定へ写す。
///
/// 正常停止のみ成功とする。core の予約済み停止理由(現状は返されない)は未完了停止、
/// core の実行エラーは実行エラーとして分類する。
fn classify_stop(outcome: Result<StopReason, RuntimeError>) -> Result<(), CliError> {
    match outcome {
        Ok(StopReason::Halted) => Ok(()),
        Ok(_) => Err(CliError::Incomplete),
        Err(error) => Err(CliError::Runtime(error)),
    }
}

/// 状態表示の文字列を作る。値はスナップショットをそのまま用いる。
fn format_state(snapshot: &MachineSnapshot) -> String {
    format!(
        "state: pc={} dp={} steps={}\n",
        snapshot.pc, snapshot.data_pointer, snapshot.steps_executed
    )
}

/// 標準ストリームの注入点。`stdin` は入力取得に、`stdout` / `stderr` は出力と診断に使う。
pub struct Streams<'a> {
    /// 標準入力(入力命令を含み `--input` が無いときのみ読む)。
    pub stdin: &'a mut dyn std::io::Read,
    /// 標準出力(プログラムの出力バイト列のみを書く)。
    pub stdout: &'a mut dyn std::io::Write,
    /// 標準エラー(使用方法・診断・状態表示)。
    pub stderr: &'a mut dyn std::io::Write,
}

/// 全体の入口。引数の列を受け取り、終了コードを返す。
///
/// `--help` は使用方法を標準エラーへ書いて 0。引数エラーは診断を標準エラーへ書いて 2。
/// 実行に至る場合は [`execute`] で固定順序に進める。この段階では標準出力へ書かない。
pub fn run_cli<I>(args: I, streams: &mut Streams<'_>) -> u8
where
    I: IntoIterator<Item = OsString>,
{
    match parse_args(args) {
        Ok(Command::Help) => {
            let _ = streams.stderr.write_all(USAGE.as_bytes());
            let _ = streams.stderr.flush();
            EXIT_SUCCESS
        }
        Ok(Command::Run(options)) => match execute(&options, streams) {
            Ok(report) => finish_run(&report),
            Err(error) => report_failure(&error, streams),
        },
        Err(error) => report_failure(&CliError::Args(error), streams),
    }
}

/// ソース読込 → 解析 → 入力取得 → 実行を固定順序で進める。最初に失敗した段のエラーを返す。
fn execute(options: &RunOptions, streams: &mut Streams<'_>) -> Result<RunReport, CliError> {
    let source = read_source_file(&options.program)?;
    let program = parse_source(&source)?;
    let input = acquire_input(options, &program, streams.stdin)?;
    Ok(run_program(program, input, options.max_steps))
}

/// 実行に至らない失敗の診断を標準エラーへ書き、終了コードを返す。書き込み失敗は無視する。
fn report_failure(error: &CliError, streams: &mut Streams<'_>) -> u8 {
    let _ = writeln!(streams.stderr, "{error}");
    error.exit_code()
}

/// 実行結果の出力段への受け渡し点。4.3 で出力・診断・状態表示の実装に置き換える。
/// 現段階では標準出力へ書かず、判定から終了コードのみを返す。
fn finish_run(report: &RunReport) -> u8 {
    match &report.result {
        Ok(()) => EXIT_SUCCESS,
        Err(error) => error.exit_code(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CliError;
    use crate::source::parse_source;
    use brainfuck_core::{PointerDirection, RuntimeError};

    fn program(source: &[u8]) -> Program {
        parse_source(source).expect("test program must parse")
    }

    #[test]
    fn run_within_limit_halts_successfully() {
        let report = run_program(program(b"+++"), Vec::new(), Some(3));
        assert!(report.result.is_ok());
        assert!(report.output.is_empty());
        assert_eq!(report.snapshot.steps_executed, 3);
        assert!(report.snapshot.halted);
    }

    #[test]
    fn run_limit_two_exceeds_limit_with_runtime_error() {
        let report = run_program(program(b"+++"), Vec::new(), Some(2));
        match report.result {
            Err(CliError::Runtime(RuntimeError::StepLimitExceeded { limit, pc })) => {
                assert_eq!(limit, 2);
                assert_eq!(pc, 2);
            }
            other => panic!("expected step limit runtime error, got {other:?}"),
        }
        assert_eq!(report.snapshot.steps_executed, 2);
        assert!(!report.snapshot.halted);
    }

    #[test]
    fn run_limit_zero_exceeds_limit_before_any_step() {
        let report = run_program(program(b"+++"), Vec::new(), Some(0));
        assert!(matches!(
            report.result,
            Err(CliError::Runtime(RuntimeError::StepLimitExceeded {
                limit: 0,
                pc: 0
            }))
        ));
        assert_eq!(report.snapshot.steps_executed, 0);
    }

    #[test]
    fn empty_program_with_limit_zero_succeeds() {
        let report = run_program(program(b""), Vec::new(), Some(0));
        assert!(report.result.is_ok());
        assert!(report.output.is_empty());
        assert_eq!(report.snapshot.steps_executed, 0);
    }

    #[test]
    fn unlimited_run_completes_past_default_style_limits() {
        let report = run_program(program(b"+++"), Vec::new(), None);
        assert!(report.result.is_ok());
        assert_eq!(report.snapshot.steps_executed, 3);
    }

    #[test]
    fn pointer_out_of_bounds_keeps_partial_output_and_snapshot() {
        // `+` でセルを 1 にし `.` で出力した後、`<` で範囲外へ移動する。
        let report = run_program(program(b"+.<"), Vec::new(), None);
        assert_eq!(report.output, vec![1u8]);
        assert_eq!(report.snapshot.pc, 2);
        assert_eq!(report.snapshot.data_pointer, 0);
        assert_eq!(report.snapshot.steps_executed, 2);
        assert!(matches!(
            report.result,
            Err(CliError::Runtime(RuntimeError::PointerOutOfBounds {
                pc: 2,
                dp: 0,
                direction: PointerDirection::Left,
            }))
        ));
    }

    #[test]
    fn input_instruction_reads_supplied_input() {
        let report = run_program(program(b",."), vec![0x41], None);
        assert!(report.result.is_ok());
        assert_eq!(report.output, vec![0x41]);
    }

    #[test]
    fn classify_halted_is_success() {
        assert!(classify_stop(Ok(StopReason::Halted)).is_ok());
    }

    #[test]
    fn classify_reserved_stop_reasons_are_incomplete() {
        for reason in [
            StopReason::StepLimitReached,
            StopReason::Breakpoint,
            StopReason::Cancelled,
        ] {
            let error = classify_stop(Ok(reason)).expect_err("reserved reason must fail");
            assert!(matches!(error, CliError::Incomplete));
            assert_eq!(error.exit_code(), 3);
        }
    }

    #[test]
    fn classify_core_error_is_runtime_error() {
        let error = classify_stop(Err(RuntimeError::StepLimitExceeded { limit: 1, pc: 1 }))
            .expect_err("runtime error must fail");
        assert!(matches!(error, CliError::Runtime(_)));
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn format_state_uses_snapshot_values() {
        let report = run_program(program(b"+++"), Vec::new(), None);
        assert_eq!(format_state(&report.snapshot), "state: pc=3 dp=0 steps=3\n");
    }

    #[test]
    fn format_state_reports_data_pointer_and_partial_progress() {
        let report = run_program(program(b">+"), Vec::new(), Some(2));
        assert_eq!(format_state(&report.snapshot), "state: pc=2 dp=1 steps=2\n");
    }

    mod entry {
        use super::super::*;
        use crate::args::USAGE;
        use std::ffi::OsString;
        use std::io;

        /// 入口を標準入力を空にして呼び、(終了コード, 標準出力, 標準エラー)を返す。
        fn call(args: &[&str]) -> (u8, Vec<u8>, Vec<u8>) {
            let mut stdin = io::empty();
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = {
                let mut streams = Streams {
                    stdin: &mut stdin,
                    stdout: &mut stdout,
                    stderr: &mut stderr,
                };
                run_cli(args.iter().map(|s| OsString::from(*s)), &mut streams)
            };
            (code, stdout, stderr)
        }

        #[test]
        fn help_writes_usage_to_stderr_and_exits_zero() {
            let (code, stdout, stderr) = call(&["--help"]);
            assert_eq!(code, 0);
            assert!(stdout.is_empty());
            assert_eq!(String::from_utf8(stderr).unwrap(), USAGE);
        }

        #[test]
        fn help_does_not_read_the_program_file() {
            // 存在しないパスを併せて渡しても、`--help` が優先され読み込みは起きない。
            let (code, stdout, stderr) = call(&["--help", "no-such-dir-4-2/missing.bf"]);
            assert_eq!(code, 0);
            assert!(stdout.is_empty());
            assert_eq!(String::from_utf8(stderr).unwrap(), USAGE);
        }

        #[test]
        fn missing_program_is_usage_error_with_empty_stdout() {
            let (code, stdout, stderr) = call(&[]);
            assert_eq!(code, 2);
            assert!(stdout.is_empty());
            let text = String::from_utf8(stderr).unwrap();
            assert!(text.contains("missing program path"));
            assert!(text.contains("--help"));
        }

        #[test]
        fn unknown_option_diagnostic_does_not_echo_the_value() {
            let (code, stdout, stderr) = call(&["--bogus-4-2-token", "prog.bf"]);
            assert_eq!(code, 2);
            assert!(stdout.is_empty());
            let text = String::from_utf8(stderr).unwrap();
            assert!(text.contains("unknown option"));
            assert!(!text.contains("bogus-4-2-token"));
        }

        #[test]
        fn missing_source_file_is_io_error_without_path_in_diagnostic() {
            let (code, stdout, stderr) = call(&["no-such-dir-4-2/missing-program-4-2.bf"]);
            assert_eq!(code, 4);
            assert!(stdout.is_empty());
            let text = String::from_utf8(stderr).unwrap();
            assert!(text.contains("failed to read program file"));
            assert!(!text.contains("missing-program-4-2"));
            assert!(!text.contains("no-such-dir-4-2"));
        }
    }
}
