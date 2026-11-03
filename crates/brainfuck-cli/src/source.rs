//! ソースファイルの読み込みと、バイト列の解析。
//!
//! バイト列は加工せず core の `Program::parse_bytes` へ渡す。UTF-8 検証・置換・BOM 除去は行わない。

use std::path::Path;

use brainfuck_core::Program;

use crate::error::{CliError, IoTarget};

/// プログラムファイルを全バイトとして読み込む。失敗は `IoTarget::ProgramFile` の入出力エラーになり、パスは保持しない。
pub fn read_source_file(path: &Path) -> Result<Vec<u8>, CliError> {
    std::fs::read(path).map_err(|source| CliError::Io {
        target: IoTarget::ProgramFile,
        source,
    })
}

/// バイト列を core で解析する。括弧エラーのみが失敗となり、それ以外のバイトはコメントとして扱われる。
pub fn parse_source(bytes: &[u8]) -> Result<Program, CliError> {
    Program::parse_bytes(bytes).map_err(CliError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use brainfuck_core::{Instruction, ParseError};
    use std::fs;
    use std::path::PathBuf;

    /// テスト用の一時ファイルパス。プロセス ID で他プロセスと衝突しないようにする。
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "brainfuck-cli-source-test-{}-{name}",
            std::process::id()
        ))
    }

    fn is_bracket_error(error: &CliError) -> bool {
        matches!(
            error,
            CliError::Parse(
                ParseError::UnmatchedClosingBracket { .. }
                    | ParseError::UnmatchedOpeningBracket { .. }
            )
        )
    }

    #[test]
    fn every_single_byte_value_only_fails_for_bracket_errors() {
        for byte in 0u8..=255 {
            let result = parse_source(&[byte]);
            match byte {
                b'[' => assert!(
                    matches!(
                        result,
                        Err(CliError::Parse(ParseError::UnmatchedOpeningBracket {
                            instruction_index: 0,
                            source_offset: 0,
                        }))
                    ),
                    "byte {byte:#04x}"
                ),
                b']' => assert!(
                    matches!(
                        result,
                        Err(CliError::Parse(ParseError::UnmatchedClosingBracket {
                            instruction_index: 0,
                            source_offset: 0,
                        }))
                    ),
                    "byte {byte:#04x}"
                ),
                _ => assert!(result.is_ok(), "byte {byte:#04x}"),
            }
        }
    }

    #[test]
    fn all_256_byte_values_parse_when_brackets_are_balanced() {
        // 0x5B `[` が先、0x5D `]` が後なので括弧は対応する。
        let all: Vec<u8> = (0u8..=255).collect();
        let program = parse_source(&all).expect("balanced brackets must parse");
        assert_eq!(program.len(), 8);
        // 命令は入力バイトの出現順(0x2B 以降の昇順)に並ぶ: + , - . < > [ ]
        assert_eq!(
            program.instructions(),
            &[
                Instruction::Increment,
                Instruction::Input,
                Instruction::Decrement,
                Instruction::Output,
                Instruction::MoveLeft,
                Instruction::MoveRight,
                Instruction::LoopStart,
                Instruction::LoopEnd,
            ]
        );
    }

    #[test]
    fn all_256_byte_values_in_reverse_fail_only_with_bracket_error() {
        // 0x5D `]` が先に現れるため括弧エラーになる。それ以外の理由で失敗しないこと。
        let reversed: Vec<u8> = (0u8..=255).rev().collect();
        let error = parse_source(&reversed).expect_err("reversed brackets must fail");
        assert!(is_bracket_error(&error), "{error:?}");
    }

    #[test]
    fn invalid_utf8_byte_before_bracket_error_reports_real_offset() {
        // 0xFF は 1 バイトのコメント。`]` はファイル先頭から 2 バイト目にある。
        let bytes = [0xFFu8, b'+', b']'];
        let error = parse_source(&bytes).expect_err("unmatched closing bracket");
        assert!(
            matches!(
                error,
                CliError::Parse(ParseError::UnmatchedClosingBracket {
                    instruction_index: 1,
                    source_offset: 2,
                })
            ),
            "{error:?}"
        );
    }

    #[test]
    fn japanese_utf8_before_bracket_error_reports_real_byte_offset() {
        // 「日本語」は UTF-8 で 9 バイト。`+` は位置 9、`]` は位置 10。
        let source = "日本語+]";
        assert_eq!(source.len(), 11);
        let error = parse_source(source.as_bytes()).expect_err("unmatched closing bracket");
        assert!(
            matches!(
                error,
                CliError::Parse(ParseError::UnmatchedClosingBracket {
                    instruction_index: 1,
                    source_offset: 10,
                })
            ),
            "{error:?}"
        );
    }

    #[test]
    fn unmatched_opening_bracket_after_non_utf8_bytes_reports_real_offset() {
        let bytes = [0x80u8, 0xFF, b'[', b'+'];
        let error = parse_source(&bytes).expect_err("unmatched opening bracket");
        assert!(
            matches!(
                error,
                CliError::Parse(ParseError::UnmatchedOpeningBracket {
                    instruction_index: 0,
                    source_offset: 2,
                })
            ),
            "{error:?}"
        );
    }

    #[test]
    fn empty_source_parses_to_zero_instructions() {
        let program = parse_source(b"").expect("empty source must parse");
        assert_eq!(program.len(), 0);
    }

    #[test]
    fn comment_only_source_parses_to_zero_instructions() {
        let program = parse_source("これはコメント\r\n".as_bytes()).expect("comment only");
        assert_eq!(program.len(), 0);
    }

    #[test]
    fn bom_is_treated_as_comment() {
        let bytes = [0xEFu8, 0xBB, 0xBF];
        let program = parse_source(&bytes).expect("BOM only");
        assert_eq!(program.len(), 0);

        let with_code = [0xEFu8, 0xBB, 0xBF, b'+'];
        let program = parse_source(&with_code).expect("BOM followed by code");
        assert_eq!(program.instructions(), &[Instruction::Increment]);
        assert_eq!(program.source_offset(0), Some(3));
    }

    #[test]
    fn read_source_file_returns_bytes_unchanged() {
        let path = temp_path("unchanged.bf");
        let content: Vec<u8> = vec![0x00, 0xFF, 0xEF, 0xBB, 0xBF, b'+', b'\r', b'\n', 0x80];
        fs::write(&path, &content).expect("write fixture");
        let result = read_source_file(&path);
        let _ = fs::remove_file(&path);
        assert_eq!(result.expect("read"), content);
    }

    #[test]
    fn read_missing_source_file_is_program_file_io_error_without_path() {
        let path = PathBuf::from("no-such-dir-bf-cli-3-2/missing-program.bf");
        let error = read_source_file(&path).expect_err("missing file");
        assert!(
            matches!(
                error,
                CliError::Io {
                    target: IoTarget::ProgramFile,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(error.exit_code(), 4);
        let text = error.to_string();
        assert!(text.contains("failed to read program file"), "{text}");
        assert!(!text.contains("missing-program"), "{text}");
        assert!(!text.contains("no-such-dir"), "{text}");
    }

    #[test]
    fn read_directory_is_program_file_io_error() {
        let dir = std::env::temp_dir();
        let error = read_source_file(&dir).expect_err("directory is not a file");
        assert!(
            matches!(
                error,
                CliError::Io {
                    target: IoTarget::ProgramFile,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(error.exit_code(), 4);
    }
}
