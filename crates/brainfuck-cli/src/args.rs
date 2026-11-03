//! コマンドライン引数の解析。argv から [`Command`] を決定的に構築する。
//!
//! ファイルシステムには触れない(存在確認は後段)。引数エラーは [`ArgsError`] の
//! 静的な分類で返し、利用者の指定値は保持しない。

// 後続タスク(4.x)で実行経路に接続するまで未使用のため許可する。
#![allow(dead_code)]

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::error::ArgsError;

/// 命令数上限の既定値(`--max-steps` / `--no-step-limit` が無いときに使う)。
pub const DEFAULT_MAX_STEPS: u64 = 10_000_000;

/// 使用方法の文面。`--help` のときに標準エラーへ書く。
pub const USAGE: &str = "\
Usage: brainfuck-cli <PROGRAM> [OPTIONS]

Runs a Brainfuck program file and writes its output to standard output.

Arguments:
  <PROGRAM>            Path to the Brainfuck source file (exactly one)

Options:
  --input <PATH>       Read the program input from PATH instead of standard input
  --max-steps <N>      Stop with exit code 3 after N executed instructions
                       (default: 10000000)
  --no-step-limit      Disable the instruction count limit
  --dump-state         Write pc, dp and steps to standard error after execution
  --help               Show this help on standard error and exit with code 0

Options may be written as --name value or --name=value, and may appear
before or after <PROGRAM>. --max-steps and --no-step-limit cannot be used
together.

Exit codes:
  0  Success
  2  Usage error or parse error (unmatched brackets)
  3  Runtime error (pointer out of bounds, step limit exceeded, incomplete run)
  4  I/O error (program file, input file, standard input or standard output)
";

/// 解析結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// 使用方法を表示する。
    Help,
    /// プログラムを実行する。
    Run(RunOptions),
}

/// 実行オプション。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// プログラムファイルのパス。
    pub program: PathBuf,
    /// 入力ファイルのパス(`--input`)。`None` は標準入力、または入力なし。
    pub input: Option<PathBuf>,
    /// 命令数上限。`None` は制限なし(`--no-step-limit`)。
    pub max_steps: Option<u64>,
    /// 停止後に実行状態を標準エラーへ書くか(`--dump-state`)。
    pub dump_state: bool,
}

/// 引数(argv[0] を除く)を解析する。
///
/// 同じ入力からは常に同じ結果を返す。`Err` の値は利用者の指定値を含まない。
pub fn parse_args<I>(args: I) -> Result<Command, ArgsError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut state = ParseState::default();
    let mut tokens = args.into_iter();
    while let Some(token) = tokens.next() {
        match token.to_str() {
            // `-` 単独はプログラムパスとして扱う(標準入力の慣習に合わせる)。
            Some(text) if text.starts_with('-') && text != "-" => {
                let (name, inline) = match text.split_once('=') {
                    Some((name, value)) => (name, Some(OsString::from(value))),
                    None => (text, None),
                };
                match name {
                    "--input" => {
                        let value = inline.or_else(|| tokens.next());
                        state.take_input(value);
                    }
                    "--max-steps" => {
                        let value = inline.or_else(|| tokens.next());
                        state.take_max_steps(value);
                    }
                    "--no-step-limit" if inline.is_none() => state.take_no_step_limit(),
                    "--dump-state" if inline.is_none() => state.dump_state = true,
                    "--help" if inline.is_none() => state.help = true,
                    _ => state.record(ArgsError::UnknownOption),
                }
            }
            _ => state.take_positional(PathBuf::from(token)),
        }
    }
    state.finish()
}

/// 走査中の状態。最初の引数エラーのみを保持し、後続のエラーでは上書きしない。
#[derive(Default)]
struct ParseState {
    first_error: Option<ArgsError>,
    help: bool,
    program: Option<PathBuf>,
    input: Option<PathBuf>,
    input_seen: bool,
    max_steps_seen: bool,
    max_steps: Option<u64>,
    no_step_limit: bool,
    dump_state: bool,
}

impl ParseState {
    fn record(&mut self, error: ArgsError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn take_positional(&mut self, path: PathBuf) {
        if self.program.is_some() {
            self.record(ArgsError::ExtraProgram);
        } else {
            self.program = Some(path);
        }
    }

    fn take_input(&mut self, value: Option<OsString>) {
        if self.input_seen {
            self.record(ArgsError::DuplicateOption("--input"));
        }
        self.input_seen = true;
        match value {
            Some(path) if !path.is_empty() => self.input = Some(PathBuf::from(path)),
            // 空の入力ファイル指定は値なしとして扱う。
            Some(_) | None => self.record(ArgsError::MissingValue("--input")),
        }
    }

    fn take_max_steps(&mut self, value: Option<OsString>) {
        if self.max_steps_seen {
            self.record(ArgsError::DuplicateOption("--max-steps"));
        }
        self.max_steps_seen = true;
        match value {
            Some(text) => match parse_max_steps(&text) {
                Some(limit) => self.max_steps = Some(limit),
                None => self.record(ArgsError::InvalidValue("--max-steps")),
            },
            None => self.record(ArgsError::MissingValue("--max-steps")),
        }
        self.check_step_limit_conflict();
    }

    fn take_no_step_limit(&mut self) {
        self.no_step_limit = true;
        self.check_step_limit_conflict();
    }

    fn check_step_limit_conflict(&mut self) {
        if self.max_steps_seen && self.no_step_limit {
            self.record(ArgsError::ConflictingStepLimit);
        }
    }

    fn finish(self) -> Result<Command, ArgsError> {
        if self.help {
            return Ok(Command::Help);
        }
        if let Some(error) = self.first_error {
            return Err(error);
        }
        let program = self.program.ok_or(ArgsError::MissingProgram)?;
        let max_steps = if self.no_step_limit {
            None
        } else {
            Some(self.max_steps.unwrap_or(DEFAULT_MAX_STEPS))
        };
        Ok(Command::Run(RunOptions {
            program,
            input: self.input,
            max_steps,
            dump_state: self.dump_state,
        }))
    }
}

/// `--max-steps` の値。ASCII 数字のみ・1 文字以上・`u64` に収まる場合に限り受理する。
fn parse_max_steps(value: &OsStr) -> Option<u64> {
    let text = value.to_str()?;
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, ArgsError> {
        parse_args(args.iter().map(OsString::from))
    }

    fn run(program: &str) -> RunOptions {
        RunOptions {
            program: PathBuf::from(program),
            input: None,
            max_steps: Some(DEFAULT_MAX_STEPS),
            dump_state: false,
        }
    }

    fn run_of(args: &[&str]) -> RunOptions {
        match parse(args) {
            Ok(Command::Run(options)) => options,
            other => panic!("expected Run for {args:?}, got {other:?}"),
        }
    }

    #[test]
    fn default_max_steps_is_ten_million() {
        assert_eq!(DEFAULT_MAX_STEPS, 10_000_000);
    }

    #[test]
    fn program_only_uses_defaults() {
        assert_eq!(parse(&["prog.bf"]), Ok(Command::Run(run("prog.bf"))));
    }

    #[test]
    fn help_alone_is_help() {
        assert_eq!(parse(&["--help"]), Ok(Command::Help));
    }

    #[test]
    fn usage_mentions_all_options_default_and_exit_codes() {
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
            assert!(USAGE.contains(needle), "usage lacks {needle:?}");
        }
    }

    #[test]
    fn all_options_with_separate_values() {
        let options = run_of(&[
            "--input",
            "in.bin",
            "--max-steps",
            "42",
            "--dump-state",
            "prog.bf",
        ]);
        assert_eq!(
            options,
            RunOptions {
                program: PathBuf::from("prog.bf"),
                input: Some(PathBuf::from("in.bin")),
                max_steps: Some(42),
                dump_state: true,
            }
        );
    }

    #[test]
    fn equals_form_is_same_as_separate_form() {
        assert_eq!(
            parse(&["--input=in.bin", "--max-steps=42", "prog.bf"]),
            parse(&["--input", "in.bin", "--max-steps", "42", "prog.bf"]),
        );
    }

    #[test]
    fn options_may_precede_or_follow_program() {
        let before = run_of(&["--dump-state", "--no-step-limit", "prog.bf"]);
        let after = run_of(&["prog.bf", "--dump-state", "--no-step-limit"]);
        assert_eq!(before, after);
        assert_eq!(before.max_steps, None);
        assert!(before.dump_state);
    }

    #[test]
    fn value_option_consumes_next_token_regardless_of_content() {
        // 値として消費されたトークンはオプションでもプログラムパスでもない。
        let options = run_of(&["--input", "--dump-state", "prog.bf"]);
        assert_eq!(options.input, Some(PathBuf::from("--dump-state")));
        assert!(!options.dump_state);

        let options = run_of(&["--input", "-", "prog.bf"]);
        assert_eq!(options.input, Some(PathBuf::from("-")));
    }

    #[test]
    fn value_consumed_help_is_not_help() {
        let options = run_of(&["--input", "--help", "prog.bf"]);
        assert_eq!(options.input, Some(PathBuf::from("--help")));
        assert_eq!(options.program, PathBuf::from("prog.bf"));

        // `--max-steps` の値として消費された `--help` は不正な値になり、Help にはならない。
        assert_eq!(
            parse(&["--max-steps", "--help", "prog.bf"]),
            Err(ArgsError::InvalidValue("--max-steps"))
        );
        // 2 回目の `--input` は値を消費したうえで重複と判定され、消費された `--help` は効かない。
        assert_eq!(
            parse(&["--input", "--help", "--input"]),
            Err(ArgsError::DuplicateOption("--input"))
        );
    }

    #[test]
    fn help_takes_precedence_over_other_errors() {
        assert_eq!(parse(&["--help"]), Ok(Command::Help));
        assert_eq!(parse(&["--unknown", "--help"]), Ok(Command::Help));
        assert_eq!(parse(&["--help", "a.bf", "b.bf"]), Ok(Command::Help));
        assert_eq!(
            parse(&["--max-steps", "x", "--no-step-limit", "--help"]),
            Ok(Command::Help)
        );
        assert_eq!(parse(&["--help", "--max-steps=x"]), Ok(Command::Help));
    }

    #[test]
    fn missing_program_is_error() {
        assert_eq!(parse(&[]), Err(ArgsError::MissingProgram));
        assert_eq!(parse(&["--dump-state"]), Err(ArgsError::MissingProgram));
    }

    #[test]
    fn extra_program_is_error() {
        assert_eq!(parse(&["a.bf", "b.bf"]), Err(ArgsError::ExtraProgram));
        assert_eq!(
            parse(&["a.bf", "--dump-state", "b.bf", "c.bf"]),
            Err(ArgsError::ExtraProgram)
        );
    }

    #[test]
    fn unknown_option_is_error() {
        assert_eq!(
            parse(&["--frobnicate", "a.bf"]),
            Err(ArgsError::UnknownOption)
        );
        assert_eq!(parse(&["-h", "a.bf"]), Err(ArgsError::UnknownOption));
        assert_eq!(
            parse(&["--dump-state=yes", "a.bf"]),
            Err(ArgsError::UnknownOption)
        );
    }

    #[test]
    fn missing_value_is_error() {
        assert_eq!(
            parse(&["a.bf", "--input"]),
            Err(ArgsError::MissingValue("--input"))
        );
        assert_eq!(
            parse(&["a.bf", "--max-steps"]),
            Err(ArgsError::MissingValue("--max-steps"))
        );
        assert_eq!(
            parse(&["--input="]),
            Err(ArgsError::MissingValue("--input"))
        );
    }

    #[test]
    fn empty_input_path_is_missing_value() {
        assert_eq!(
            parse(&["a.bf", "--input", ""]),
            Err(ArgsError::MissingValue("--input"))
        );
    }

    #[test]
    fn duplicate_value_option_is_error() {
        assert_eq!(
            parse(&["a.bf", "--input", "x", "--input", "y"]),
            Err(ArgsError::DuplicateOption("--input"))
        );
        assert_eq!(
            parse(&["--max-steps=1", "--max-steps", "2", "a.bf"]),
            Err(ArgsError::DuplicateOption("--max-steps"))
        );
    }

    #[test]
    fn duplicate_flags_are_idempotent() {
        assert_eq!(
            parse(&["--dump-state", "a.bf", "--dump-state"]),
            parse(&["--dump-state", "a.bf"])
        );
        assert_eq!(
            parse(&["--no-step-limit", "--no-step-limit", "a.bf"]),
            parse(&["--no-step-limit", "a.bf"])
        );
    }

    #[test]
    fn max_steps_and_no_step_limit_conflict() {
        assert_eq!(
            parse(&["--max-steps", "5", "--no-step-limit", "a.bf"]),
            Err(ArgsError::ConflictingStepLimit)
        );
        assert_eq!(
            parse(&["--no-step-limit", "a.bf", "--max-steps=5"]),
            Err(ArgsError::ConflictingStepLimit)
        );
    }

    #[test]
    fn invalid_max_steps_values() {
        let cases: [&str; 8] = [
            "",
            "-1",
            "+3",
            "abc",
            " 3",
            "0x10",
            "1_000",
            "18446744073709551616",
        ];
        for value in cases {
            assert_eq!(
                parse(&["--max-steps", value, "a.bf"]),
                Err(ArgsError::InvalidValue("--max-steps")),
                "value {value:?}"
            );
            let joined = format!("--max-steps={value}");
            assert_eq!(
                parse(&[joined.as_str(), "a.bf"]),
                Err(ArgsError::InvalidValue("--max-steps")),
                "equals form, value {value:?}"
            );
        }
    }

    #[test]
    fn max_steps_resolution() {
        assert_eq!(run_of(&["a.bf"]).max_steps, Some(DEFAULT_MAX_STEPS));
        assert_eq!(run_of(&["--no-step-limit", "a.bf"]).max_steps, None);
        assert_eq!(run_of(&["--max-steps", "0", "a.bf"]).max_steps, Some(0));
        assert_eq!(run_of(&["--max-steps=7", "a.bf"]).max_steps, Some(7));
        assert_eq!(
            run_of(&["--max-steps", "18446744073709551615", "a.bf"]).max_steps,
            Some(u64::MAX)
        );
    }

    #[test]
    fn first_error_is_kept_and_later_errors_do_not_replace_it() {
        // 先頭のエラーが報告される(走査は続けるが上書きしない)。
        assert_eq!(
            parse(&["--frobnicate", "a.bf", "b.bf"]),
            Err(ArgsError::UnknownOption)
        );
        assert_eq!(
            parse(&["a.bf", "b.bf", "--frobnicate"]),
            Err(ArgsError::ExtraProgram)
        );
        assert_eq!(
            parse(&["--max-steps", "x", "--max-steps", "1", "a.bf"]),
            Err(ArgsError::InvalidValue("--max-steps"))
        );
    }

    #[test]
    fn parsing_never_touches_the_filesystem_and_is_deterministic() {
        let args = ["--input", "does-not-exist.bin", "missing.bf"];
        assert_eq!(parse(&args), parse(&args));
        assert_eq!(
            parse(&args),
            Ok(Command::Run(RunOptions {
                program: PathBuf::from("missing.bf"),
                input: Some(PathBuf::from("does-not-exist.bin")),
                max_steps: Some(DEFAULT_MAX_STEPS),
                dump_state: false,
            }))
        );
    }

    #[test]
    fn errors_never_contain_supplied_values() {
        // ArgsError は静的な分類のみ。指定値は Debug にも現れない。
        let secret = "秘密めいた文字列";
        let errors = [
            parse(&["--max-steps", secret, "a.bf"]),
            parse(&["--frobnicate-secret", "a.bf"]),
            parse(&["a.bf", secret]),
        ];
        for error in errors {
            let text = format!("{error:?}");
            assert!(!text.contains(secret), "{text}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn non_utf8_positional_is_program_path_without_panic() {
        use std::os::windows::ffi::OsStringExt;
        let program = OsString::from_wide(&[0xD800, 0x0061]);
        let result = parse_args(vec![program.clone()]);
        assert_eq!(
            result,
            Ok(Command::Run(RunOptions {
                program: PathBuf::from(program),
                input: None,
                max_steps: Some(DEFAULT_MAX_STEPS),
                dump_state: false,
            }))
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_positional_is_program_path_without_panic() {
        use std::os::unix::ffi::OsStringExt;
        let program = OsString::from_vec(vec![0xFF, 0xFE, b'.', b'b', b'f']);
        let result = parse_args(vec![program.clone()]);
        assert_eq!(
            result,
            Ok(Command::Run(RunOptions {
                program: PathBuf::from(program),
                input: None,
                max_steps: Some(DEFAULT_MAX_STEPS),
                dump_state: false,
            }))
        );
    }
}
