//! プログラムの実行と結果の整理。
//!
//! 解析済みプログラム・入力バイト列・解決済みの命令数上限から core の機械を生成し、
//! `run` を 1 回だけ呼ぶ。実行意味論と上限判定は core に委ね、ここでは再実装しない。
//! 出力・スナップショット・判定は [`RunReport`] に束ね、後続の出力段へ渡す。

use std::ffi::OsString;
use std::io::Write;

use brainfuck_core::{Machine, MachineConfig, MachineSnapshot, Program, RuntimeError, StopReason};

use crate::args::{parse_args, Command, RunOptions, USAGE};
use crate::error::{CliError, IoTarget, EXIT_SUCCESS};
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
/// 実行に至る場合は [`execute`] で固定順序に進め、結果の書き出しと終了コードの決定は
/// [`emit_results`] に委ねる。
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
            Ok(report) => emit_results(&options, &report, streams),
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

/// 診断を 1 行で標準エラーへ書く。書き込み失敗は無視する(パニックしない)。
fn report_diagnostic(error: &CliError, stderr: &mut dyn Write) {
    let _ = stderr.write_all(format!("{error}\n").as_bytes());
    let _ = stderr.flush();
}

/// 実行に至らない失敗の診断を標準エラーへ書き、終了コードを返す。書き込み失敗は無視する。
fn report_failure(error: &CliError, streams: &mut Streams<'_>) -> u8 {
    report_diagnostic(error, streams.stderr);
    error.exit_code()
}

/// 出力バイト列を無加工で標準出力へ書き、フラッシュする。0 バイトなら何も書かない。
fn write_output(output: &[u8], stdout: &mut dyn Write) -> Result<(), CliError> {
    if output.is_empty() {
        return Ok(());
    }
    stdout
        .write_all(output)
        .and_then(|()| stdout.flush())
        .map_err(|source| CliError::Io {
            target: IoTarget::StandardOutput,
            source,
        })
}

/// 実行結果を標準ストリームへ書き、終了コードを決める出力段。ファイルには依存しない。
///
/// 順序は固定: 出力(部分出力を含む)→ 標準出力の書き込み失敗の診断 → 実行エラーの診断
/// → 状態表示(`--dump-state` 時、標準出力の結果によらず)。終了コードは、標準出力の
/// 書き込み失敗があれば 4、なければ実行エラー・未完了停止の分類、どちらもなければ 0。
/// 標準エラーへの書き込み失敗は無視する。
fn emit_results(options: &RunOptions, report: &RunReport, streams: &mut Streams<'_>) -> u8 {
    let write_failure = write_output(&report.output, streams.stdout).err();
    if let Some(error) = &write_failure {
        report_diagnostic(error, streams.stderr);
    }
    if let Err(error) = &report.result {
        report_diagnostic(error, streams.stderr);
    }
    if options.dump_state {
        let _ = streams
            .stderr
            .write_all(format_state(&report.snapshot).as_bytes());
        let _ = streams.stderr.flush();
    }
    match (write_failure, &report.result) {
        (Some(error), _) => error.exit_code(),
        (None, Ok(())) => EXIT_SUCCESS,
        (None, Err(error)) => error.exit_code(),
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
    fn unlimited_run_exceeds_default_limit_and_completes_with_exact_steps() {
        // `-[>-[>-[-]<-]<-]` の実行数は手計算で 2^25 = 33554432(既定上限 10,000,000 を超える)。
        // 内側の `[-]` は 1 ループあたり `-` と `]` の 2 命令(`[` は入口で 1 回のみ)。
        // 外側 1 周は 131586 命令、外側ループは 255 周、前後の `-` と `[` で +2。
        // 255 × 131586 + 2 = 33554432。停止時の pc は命令数 16、dp は 0 に戻る。
        let report = run_program(program(b"-[>-[>-[-]<-]<-]"), Vec::new(), None);
        assert!(report.result.is_ok(), "{:?}", report.result);
        assert_eq!(report.snapshot.steps_executed, 33_554_432);
        assert_eq!(report.snapshot.pc, 16);
        assert_eq!(report.snapshot.data_pointer, 0);
        assert!(report.output.is_empty());
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

    mod emit {
        use super::super::*;
        use crate::source::parse_source;
        use std::cell::RefCell;
        use std::io;
        use std::path::PathBuf;

        /// 書き込み先と呼び出し順を一つのログへ記録する。
        struct Tagged<'a> {
            tag: char,
            log: &'a RefCell<String>,
        }

        impl io::Write for Tagged<'_> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let text = String::from_utf8_lossy(buf);
                self.log
                    .borrow_mut()
                    .push_str(&format!("{}:{text}|", self.tag));
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                self.log
                    .borrow_mut()
                    .push_str(&format!("{}:flush|", self.tag));
                Ok(())
            }
        }

        /// 書き込みを呼ばれた回数だけ数える。
        #[derive(Default)]
        struct Counting {
            calls: usize,
        }

        impl io::Write for Counting {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.calls += 1;
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                self.calls += 1;
                Ok(())
            }
        }

        /// 常に書き込みに失敗する(閉じたパイプ相当)。
        struct Broken;

        impl io::Write for Broken {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        /// 参照プログラム。出力は `Hello World!\n`、停止時 `pc=106 dp=6`、実行済み命令数 906。
        const HELLO: &[u8] = b"++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";

        fn options(dump_state: bool) -> RunOptions {
            RunOptions {
                program: PathBuf::from("prog.bf"),
                input: None,
                max_steps: None,
                dump_state,
            }
        }

        fn report(source: &[u8], max_steps: Option<u64>) -> RunReport {
            let program = parse_source(source).expect("test program must parse");
            run_program(program, Vec::new(), max_steps)
        }

        fn emit(
            report: &RunReport,
            dump_state: bool,
            stdout: &mut dyn io::Write,
            stderr: &mut dyn io::Write,
        ) -> u8 {
            let mut stdin = io::empty();
            let mut streams = Streams {
                stdin: &mut stdin,
                stdout,
                stderr,
            };
            emit_results(&options(dump_state), report, &mut streams)
        }

        /// 成功時は出力だけを書き、`--dump-state` なしなら標準エラーは空で 0 を返す。
        #[test]
        fn success_writes_output_and_leaves_stderr_empty() {
            let log = RefCell::new(String::new());
            let mut stdout = Tagged {
                tag: 'O',
                log: &log,
            };
            let mut stderr = Vec::new();
            let code = emit(&report(HELLO, None), false, &mut stdout, &mut stderr);
            assert_eq!(code, 0);
            assert!(stderr.is_empty());
            assert_eq!(*log.borrow(), "O:Hello World!\n|O:flush|");
        }

        /// `--dump-state` 指定時は、停止時のスナップショットを標準エラーへ書く。
        #[test]
        fn dump_state_writes_snapshot_to_stderr_on_success() {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = emit(&report(HELLO, None), true, &mut stdout, &mut stderr);
            assert_eq!(code, 0);
            assert_eq!(stdout, b"Hello World!\n");
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                "state: pc=106 dp=6 steps=906\n"
            );
        }

        /// 0 バイト出力では標準出力へ一切書かず、フラッシュもしない。
        #[test]
        fn empty_output_never_touches_stdout() {
            let mut stdout = Counting::default();
            let mut stderr = Vec::new();
            let code = emit(&report(b"+++", None), false, &mut stdout, &mut stderr);
            assert_eq!(code, 0);
            assert_eq!(stdout.calls, 0);
            assert!(stderr.is_empty());
        }

        /// 実行エラーでは部分出力を先に書き、その後に診断を書く。終了コードは 3。
        #[test]
        fn runtime_error_writes_partial_output_before_diagnostic() {
            let log = RefCell::new(String::new());
            let mut stdout = Tagged {
                tag: 'O',
                log: &log,
            };
            let mut stderr = Tagged {
                tag: 'E',
                log: &log,
            };
            let code = emit(&report(b"+.<", None), false, &mut stdout, &mut stderr);
            assert_eq!(code, 3);
            let text = log.borrow().clone();
            assert!(
                text.starts_with("O:\u{1}|O:flush|E:error: runtime error: data pointer out of bounds: pc=2, dp=0, direction=left\n|"),
                "{text}"
            );
        }

        /// 実行エラーの後も `--dump-state` の状態表示は診断の次に書かれる。
        #[test]
        fn runtime_error_with_dump_state_prints_diagnostic_then_state() {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = emit(&report(b"+.<", None), true, &mut stdout, &mut stderr);
            assert_eq!(code, 3);
            assert_eq!(stdout, [1u8]);
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                "error: runtime error: data pointer out of bounds: pc=2, dp=0, direction=left\nstate: pc=2 dp=0 steps=2\n"
            );
        }

        /// 上限超過でも、停止時点のスナップショットを状態表示に使う。
        #[test]
        fn step_limit_exceeded_dump_state_shows_partial_progress() {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = emit(&report(b"+++", Some(2)), true, &mut stdout, &mut stderr);
            assert_eq!(code, 3);
            assert!(stdout.is_empty());
            let text = String::from_utf8(stderr).unwrap();
            assert!(text.ends_with("state: pc=2 dp=0 steps=2\n"), "{text}");
            assert!(text.contains("limit=2"), "{text}");
        }

        /// 標準出力の書き込み失敗は 4。実行エラーがなければ I/O 診断と状態表示を書く。
        #[test]
        fn stdout_write_failure_without_runtime_error_is_exit_four() {
            let mut stdout = Broken;
            let mut stderr = Vec::new();
            let code = emit(&report(HELLO, None), true, &mut stdout, &mut stderr);
            assert_eq!(code, 4);
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                "error: I/O error: failed to write standard output: closed pipe\nstate: pc=106 dp=6 steps=906\n"
            );
        }

        /// 実行エラーの後に標準出力の書き込みが失敗した場合は 4 とし、両方の診断と状態表示を書く。
        #[test]
        fn stdout_write_failure_after_runtime_error_reports_both_diagnostics() {
            let mut stdout = Broken;
            let mut stderr = Vec::new();
            let code = emit(&report(b"+.<", None), true, &mut stdout, &mut stderr);
            assert_eq!(code, 4);
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                "error: I/O error: failed to write standard output: closed pipe\nerror: runtime error: data pointer out of bounds: pc=2, dp=0, direction=left\nstate: pc=2 dp=0 steps=2\n"
            );
        }

        /// 標準エラーへの書き込み失敗は無視し、終了コードは決まったまま返る。
        #[test]
        fn stderr_write_failure_is_ignored() {
            let mut stdout = Vec::new();
            let mut stderr = Broken;
            let code = emit(&report(b"+.<", None), true, &mut stdout, &mut stderr);
            assert_eq!(code, 3);
            assert_eq!(stdout, [1u8]);
        }

        /// 同じ入力からの再実行は、出力・標準エラー・終了コードが一致する。
        #[test]
        fn repeated_runs_are_deterministic() {
            let mut first_out = Vec::new();
            let mut first_err = Vec::new();
            let first = emit(&report(HELLO, None), true, &mut first_out, &mut first_err);
            let mut second_out = Vec::new();
            let mut second_err = Vec::new();
            let second = emit(&report(HELLO, None), true, &mut second_out, &mut second_err);
            assert_eq!(first, second);
            assert_eq!(first_out, second_out);
            assert_eq!(first_err, second_err);
        }
    }
}
