//! 失敗の分類・終了コード・診断文の集約。
//!
//! 終了コードの決定点は [`CliError::exit_code`] のみ。診断文は固定テンプレートで、
//! コマンドラインの値(パス、`--max-steps` の値、未知のトークン)を保持も表示もしない。

use std::fmt;
use std::io;

use brainfuck_core::{ParseError, RuntimeError};

/// 成功(停止し、出力をすべて書き込めた)。
pub const EXIT_SUCCESS: u8 = 0;
/// 引数エラー / 括弧エラー。
pub const EXIT_USAGE: u8 = 2;
/// 実行エラー(範囲外 / 上限超過 / 未完了停止)。
pub const EXIT_RUNTIME: u8 = 3;
/// I/O エラー。
pub const EXIT_IO: u8 = 4;

/// 引数の検証で判明する不正の種類。値は保持しない(既知のオプション名のみ静的文字列で持つ)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgsError {
    /// プログラムパスが指定されていない。
    MissingProgram,
    /// プログラムパス以外の余剰な位置引数がある。
    ExtraProgram,
    /// 未知のオプションがある。
    UnknownOption,
    /// 値を要するオプションに値がない(既知のオプション名)。
    MissingValue(&'static str),
    /// 同じオプションが重複指定された(既知のオプション名)。
    DuplicateOption(&'static str),
    /// `--max-steps` と `--no-step-limit` が同時に指定された。
    ConflictingStepLimit,
    /// 値の形式が不正(既知のオプション名)。
    InvalidValue(&'static str),
}

/// 入出力エラーの対象種別。診断では役割名のみを示し、パスは含めない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoTarget {
    /// プログラムファイル。
    ProgramFile,
    /// 入力ファイル。
    InputFile,
    /// 標準入力。
    StandardInput,
    /// 標準出力。
    StandardOutput,
}

/// CLI 全体の失敗。
#[derive(Debug)]
pub enum CliError {
    /// 引数の検証に失敗した。
    Args(ArgsError),
    /// ソースの括弧が対応していない。
    Parse(ParseError),
    /// 実行エンジンが実行エラーを返した。
    Runtime(RuntimeError),
    /// `run()` が `Halted` 以外の正常結果を返した(防御)。
    Incomplete,
    /// 入出力に失敗した。OS の原因を保持する。
    Io {
        /// 失敗した対象の種別。
        target: IoTarget,
        /// OS が報告したエラー。
        source: io::Error,
    },
}

impl CliError {
    /// 終了コードを返す。`Args` / `Parse` は 2、`Runtime` / `Incomplete` は 3、`Io` は 4。
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Args(_) | CliError::Parse(_) => EXIT_USAGE,
            CliError::Runtime(_) | CliError::Incomplete => EXIT_RUNTIME,
            CliError::Io { .. } => EXIT_IO,
        }
    }
}

/// 引数エラーの原因文(固定文面)。
fn args_reason(error: ArgsError) -> String {
    match error {
        ArgsError::MissingProgram => "missing program path".to_owned(),
        ArgsError::ExtraProgram => "unexpected extra argument".to_owned(),
        ArgsError::UnknownOption => "unknown option".to_owned(),
        ArgsError::MissingValue(option) => format!("missing value for {option}"),
        ArgsError::DuplicateOption(option) => format!("option {option} specified more than once"),
        ArgsError::ConflictingStepLimit => {
            "--max-steps and --no-step-limit cannot be used together".to_owned()
        }
        ArgsError::InvalidValue(option) => {
            format!("invalid value for {option} (expected a non-negative integer)")
        }
    }
}

/// 入出力エラーの操作説明(対象の役割名を含む)。
fn io_action(target: IoTarget) -> &'static str {
    match target {
        IoTarget::ProgramFile => "failed to read program file",
        IoTarget::InputFile => "failed to read input file",
        IoTarget::StandardInput => "failed to read standard input",
        IoTarget::StandardOutput => "failed to write standard output",
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Args(error) => write!(
                f,
                "error: {}\nTry 'brainfuck-cli --help' for usage.",
                args_reason(*error)
            ),
            CliError::Parse(error) => write!(f, "error: parse error: {error}"),
            CliError::Runtime(error) => write!(f, "error: runtime error: {error}"),
            CliError::Incomplete => f.write_str("error: execution stopped before completion"),
            CliError::Io { target, source } => {
                write!(f, "error: I/O error: {}: {source}", io_action(*target))
            }
        }
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;
    use brainfuck_core::PointerDirection;
    use std::error::Error as _;

    const ALL_ARGS_ERRORS: [ArgsError; 7] = [
        ArgsError::MissingProgram,
        ArgsError::ExtraProgram,
        ArgsError::UnknownOption,
        ArgsError::MissingValue("--input"),
        ArgsError::DuplicateOption("--input"),
        ArgsError::ConflictingStepLimit,
        ArgsError::InvalidValue("--max-steps"),
    ];

    fn parse_error() -> CliError {
        CliError::Parse(ParseError::UnmatchedClosingBracket {
            instruction_index: 3,
            source_offset: 7,
        })
    }

    fn runtime_error() -> CliError {
        CliError::Runtime(RuntimeError::PointerOutOfBounds {
            pc: 12,
            dp: 0,
            direction: PointerDirection::Left,
        })
    }

    fn io_error(target: IoTarget) -> CliError {
        CliError::Io {
            target,
            source: io::Error::new(io::ErrorKind::NotFound, "os says no"),
        }
    }

    #[test]
    fn exit_code_constants_are_the_four_contract_values() {
        assert_eq!(EXIT_SUCCESS, 0);
        assert_eq!(EXIT_USAGE, 2);
        assert_eq!(EXIT_RUNTIME, 3);
        assert_eq!(EXIT_IO, 4);
    }

    #[test]
    fn every_args_error_maps_to_usage_exit_code() {
        for error in ALL_ARGS_ERRORS {
            assert_eq!(CliError::Args(error).exit_code(), EXIT_USAGE, "{error:?}");
        }
    }

    #[test]
    fn parse_error_maps_to_usage_exit_code() {
        assert_eq!(parse_error().exit_code(), 2);
    }

    #[test]
    fn runtime_and_incomplete_map_to_runtime_exit_code() {
        assert_eq!(runtime_error().exit_code(), 3);
        assert_eq!(CliError::Incomplete.exit_code(), 3);
    }

    #[test]
    fn io_error_maps_to_io_exit_code_for_every_target() {
        for target in [
            IoTarget::ProgramFile,
            IoTarget::InputFile,
            IoTarget::StandardInput,
            IoTarget::StandardOutput,
        ] {
            assert_eq!(io_error(target).exit_code(), 4, "{target:?}");
        }
    }

    #[test]
    fn exit_codes_are_only_zero_two_three_or_four() {
        let mut errors: Vec<CliError> =
            ALL_ARGS_ERRORS.iter().map(|e| CliError::Args(*e)).collect();
        errors.push(parse_error());
        errors.push(runtime_error());
        errors.push(CliError::Incomplete);
        errors.push(io_error(IoTarget::ProgramFile));
        for error in errors {
            assert!(matches!(error.exit_code(), 2..=4), "{error:?}");
        }
    }

    #[test]
    fn args_diagnostics_use_fixed_templates() {
        let expected = [
            (ArgsError::MissingProgram, "missing program path"),
            (ArgsError::ExtraProgram, "unexpected extra argument"),
            (ArgsError::UnknownOption, "unknown option"),
            (
                ArgsError::MissingValue("--input"),
                "missing value for --input",
            ),
            (
                ArgsError::DuplicateOption("--input"),
                "option --input specified more than once",
            ),
            (
                ArgsError::ConflictingStepLimit,
                "--max-steps and --no-step-limit cannot be used together",
            ),
            (
                ArgsError::InvalidValue("--max-steps"),
                "invalid value for --max-steps (expected a non-negative integer)",
            ),
        ];
        for (error, reason) in expected {
            let text = CliError::Args(error).to_string();
            assert_eq!(
                text,
                format!("error: {reason}\nTry 'brainfuck-cli --help' for usage."),
                "{error:?}"
            );
        }
    }

    #[test]
    fn args_diagnostics_never_contain_supplied_values() {
        // 引数エラーは利用者の指定値を保持しないため、表示にも現れてはならない。
        let secret = "秘密めいた文字列";
        for error in ALL_ARGS_ERRORS {
            let text = CliError::Args(error).to_string();
            assert!(!text.contains(secret), "{error:?}: {text}");
            assert!(!text.contains("secret"), "{error:?}: {text}");
        }
    }

    #[test]
    fn parse_diagnostic_embeds_core_message_with_numbers() {
        let text = parse_error().to_string();
        assert_eq!(
            text,
            "error: parse error: unmatched closing bracket: instruction #3, source offset 7"
        );
    }

    #[test]
    fn runtime_diagnostic_embeds_core_message_with_numbers() {
        let text = runtime_error().to_string();
        assert_eq!(
            text,
            "error: runtime error: data pointer out of bounds: pc=12, dp=0, direction=left"
        );
    }

    #[test]
    fn step_limit_diagnostic_embeds_limit_and_pc() {
        let error = CliError::Runtime(RuntimeError::StepLimitExceeded {
            limit: 10_000_000,
            pc: 5,
        });
        let text = error.to_string();
        assert!(text.contains("step limit exceeded"), "{text}");
        assert!(text.contains("limit=10000000"), "{text}");
        assert!(text.contains("pc=5"), "{text}");
    }

    #[test]
    fn incomplete_diagnostic_states_early_stop() {
        assert_eq!(
            CliError::Incomplete.to_string(),
            "error: execution stopped before completion"
        );
    }

    #[test]
    fn io_diagnostics_name_target_role_and_os_cause() {
        let cases = [
            (IoTarget::ProgramFile, "failed to read program file"),
            (IoTarget::InputFile, "failed to read input file"),
            (IoTarget::StandardInput, "failed to read standard input"),
            (IoTarget::StandardOutput, "failed to write standard output"),
        ];
        for (target, action) in cases {
            assert_eq!(
                io_error(target).to_string(),
                format!("error: I/O error: {action}: os says no"),
                "{target:?}"
            );
        }
    }

    #[test]
    fn io_error_keeps_os_error_as_is() {
        let error = io_error(IoTarget::ProgramFile);
        match &error {
            CliError::Io { source, .. } => {
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn cli_error_implements_std_error_without_source() {
        let error = CliError::Incomplete;
        assert!(error.source().is_none());
    }
}
