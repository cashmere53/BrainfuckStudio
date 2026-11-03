//! プログラムへ渡す入力バイト列の取得。
//!
//! 取得元は `--input` のファイル、標準入力、または空のいずれか。バイト列は加工せず core へ渡す。
//! 標準入力は「入力命令があり、かつ `--input` が無い」ときに限って読む。

// 後続タスク(4.x)で実行経路に接続するまで未使用のため許可する。
#![allow(dead_code)]

use std::io::Read;

use brainfuck_core::{Instruction, Program};

use crate::args::RunOptions;
use crate::error::{CliError, IoTarget};

/// 解析済みプログラムが入力命令(`,`)を 1 個以上含むかを返す。命令列の線形走査のみを行う。
pub fn requires_stdin(program: &Program) -> bool {
    program.instructions().contains(&Instruction::Input)
}

/// プログラムへ渡す入力バイト列を取得する。判定順は次のとおり。
///
/// 1. `--input` の指定がある → そのファイルの全バイトを無変換で読む。標準入力には触れない。
///    失敗は入力ファイルを対象とする入出力エラー。
/// 2. 指定がなく、プログラムが入力命令を含む → 標準入力を EOF まで無変換で読む。
///    失敗は標準入力を対象とする入出力エラー。
/// 3. それ以外 → 空の入力を返す。標準入力には触れない。
pub fn acquire_input(
    options: &RunOptions,
    program: &Program,
    stdin: &mut dyn Read,
) -> Result<Vec<u8>, CliError> {
    if let Some(path) = &options.input {
        return std::fs::read(path).map_err(|source| CliError::Io {
            target: IoTarget::InputFile,
            source,
        });
    }
    if requires_stdin(program) {
        let mut bytes = Vec::new();
        stdin
            .read_to_end(&mut bytes)
            .map_err(|source| CliError::Io {
                target: IoTarget::StandardInput,
                source,
            })?;
        return Ok(bytes);
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::RunOptions;
    use crate::error::IoTarget;
    use crate::source::parse_source;
    use std::io::{self, Cursor, Read};
    use std::path::PathBuf;

    /// 呼ばれた時点で失敗させる標準入力。「標準入力に触れないこと」の検証に使う。
    struct PanicReader;

    impl Read for PanicReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            panic!("standard input must not be read");
        }
    }

    /// 読み出すと常に失敗する標準入力。読み取り失敗の検証に使う。
    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "os says no"))
        }
    }

    /// 1 バイトずつ返す標準入力。EOF まで繰り返し読まれることの検証に使う。
    struct ChunkedReader {
        data: Vec<u8>,
        position: usize,
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.position >= self.data.len() || buf.is_empty() {
                return Ok(0);
            }
            buf[0] = self.data[self.position];
            self.position += 1;
            Ok(1)
        }
    }

    /// テスト用の一時ファイルパス。プロセス ID で他プロセスと衝突しないようにする。
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "brainfuck-cli-input-test-{}-{name}",
            std::process::id()
        ))
    }

    /// 一時ファイルを作成し、テスト終了時に削除する前提でパスを返す。
    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path = temp_path(name);
        std::fs::write(&path, bytes).expect("create temp input file");
        path
    }

    fn options(input: Option<PathBuf>) -> RunOptions {
        RunOptions {
            program: PathBuf::from("program.bf"),
            input,
            max_steps: None,
            dump_state: false,
        }
    }

    fn program(source: &[u8]) -> brainfuck_core::Program {
        parse_source(source).expect("test program must parse")
    }

    fn assert_input_io_error(result: Result<Vec<u8>, CliError>, label: &str) {
        match result {
            Err(CliError::Io {
                target: IoTarget::InputFile,
                ..
            }) => {}
            other => panic!("{label}: expected input file I/O error, got {other:?}"),
        }
    }

    // --- requires_stdin -------------------------------------------------

    #[test]
    fn program_with_input_instruction_requires_stdin() {
        assert!(requires_stdin(&program(b",")));
        assert!(requires_stdin(&program(b"+,.")));
        // コメント文字の中の `,` 相当ではなく、命令として解析された `,` だけを数える。
        assert!(requires_stdin(&program(b"hello, world")));
    }

    #[test]
    fn program_without_input_instruction_does_not_require_stdin() {
        assert!(!requires_stdin(&program(b"")));
        assert!(!requires_stdin(&program(b"+-<>[].")));
        assert!(!requires_stdin(&program(b"no instructions here")));
    }

    // --- acquire_input: 入力ファイル指定時 ---------------------------------

    #[test]
    fn input_file_bytes_are_returned_verbatim_including_nul_and_ff() {
        let bytes = [0x00u8, 0xFF, 0x0A, 0x0D, 0x80];
        let path = write_temp("verbatim.bin", &bytes);
        let result = acquire_input(
            &options(Some(path.clone())),
            &program(b"+"),
            &mut PanicReader,
        );
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.expect("input file must be read"), bytes.to_vec());
    }

    #[test]
    fn input_file_is_read_without_touching_stdin_when_program_needs_input() {
        let path = write_temp("with-input-instruction.bin", b"abc");
        let result = acquire_input(
            &options(Some(path.clone())),
            &program(b",,,"),
            &mut PanicReader,
        );
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.expect("input file must be read"), b"abc".to_vec());
    }

    #[test]
    fn input_file_is_read_without_touching_stdin_when_program_has_no_input() {
        let path = write_temp("without-input-instruction.bin", b"xyz");
        let result = acquire_input(
            &options(Some(path.clone())),
            &program(b"+[-]."),
            &mut PanicReader,
        );
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.expect("input file must be read"), b"xyz".to_vec());
    }

    #[test]
    fn empty_input_file_yields_empty_bytes() {
        let path = write_temp("empty.bin", b"");
        let result = acquire_input(
            &options(Some(path.clone())),
            &program(b","),
            &mut PanicReader,
        );
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.expect("empty input file is valid"), Vec::<u8>::new());
    }

    #[test]
    fn missing_input_file_is_input_io_error_with_input_instruction() {
        let path = temp_path("does-not-exist-with-input");
        let _ = std::fs::remove_file(&path);
        let result = acquire_input(&options(Some(path)), &program(b",.,"), &mut PanicReader);
        assert_input_io_error(result, "with input instruction");
    }

    #[test]
    fn missing_input_file_is_input_io_error_without_input_instruction() {
        let path = temp_path("does-not-exist-without-input");
        let _ = std::fs::remove_file(&path);
        let result = acquire_input(&options(Some(path)), &program(b"+."), &mut PanicReader);
        assert_input_io_error(result, "without input instruction");
    }

    // --- acquire_input: 入力ファイル未指定 ----------------------------------

    #[test]
    fn stdin_is_read_to_eof_verbatim_when_program_has_input_instruction() {
        let bytes = vec![0x00u8, 0xFF, 0x0A, b'a', 0x0D, 0x80];
        let mut stdin = ChunkedReader {
            data: bytes.clone(),
            position: 0,
        };
        let result = acquire_input(&options(None), &program(b",."), &mut stdin);
        assert_eq!(result.expect("stdin must be read"), bytes);
        assert_eq!(
            stdin.position,
            stdin.data.len(),
            "stdin must be read to EOF"
        );
    }

    #[test]
    fn empty_stdin_yields_empty_bytes_when_program_has_input_instruction() {
        let mut stdin = Cursor::new(Vec::<u8>::new());
        let result = acquire_input(&options(None), &program(b","), &mut stdin);
        assert_eq!(result.expect("empty stdin is valid"), Vec::<u8>::new());
    }

    #[test]
    fn stdin_is_not_read_when_program_has_no_input_instruction() {
        let result = acquire_input(&options(None), &program(b"+++."), &mut PanicReader);
        assert_eq!(result.expect("no input needed"), Vec::<u8>::new());
    }

    #[test]
    fn empty_program_does_not_read_stdin() {
        let result = acquire_input(&options(None), &program(b""), &mut PanicReader);
        assert_eq!(result.expect("no input needed"), Vec::<u8>::new());
    }

    #[test]
    fn stdin_read_failure_is_stdin_io_error_when_program_has_input_instruction() {
        let result = acquire_input(&options(None), &program(b","), &mut FailingReader);
        match result {
            Err(CliError::Io {
                target: IoTarget::StandardInput,
                source,
            }) => assert_eq!(source.kind(), io::ErrorKind::BrokenPipe),
            other => panic!("expected standard input I/O error, got {other:?}"),
        }
    }

    #[test]
    fn stdin_failure_is_not_reported_when_program_has_no_input_instruction() {
        // 入力命令が無ければ標準入力に触れないため、読み取り失敗も起きない。
        let result = acquire_input(&options(None), &program(b"+."), &mut FailingReader);
        assert_eq!(result.expect("stdin must not be read"), Vec::<u8>::new());
    }

    #[test]
    fn input_file_takes_precedence_over_stdin_failure() {
        let path = write_temp("precedence.bin", b"ok");
        let result = acquire_input(
            &options(Some(path.clone())),
            &program(b","),
            &mut FailingReader,
        );
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.expect("file is read, stdin is not"), b"ok".to_vec());
    }

    #[test]
    fn acquire_input_is_deterministic_for_same_stdin_bytes() {
        let first = acquire_input(
            &options(None),
            &program(b","),
            &mut Cursor::new(b"same".to_vec()),
        )
        .expect("first read");
        let second = acquire_input(
            &options(None),
            &program(b","),
            &mut Cursor::new(b"same".to_vec()),
        )
        .expect("second read");
        assert_eq!(first, second);
    }
}
