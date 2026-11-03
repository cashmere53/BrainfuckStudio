//! Brainfuck 実行エンジンの中核 crate。
//!
//! 構文解析と実行機械を提供する。標準入出力・ファイルシステム・UI には依存しない。
//! 入力は呼び出し元が供給するバイト列、出力は呼び出し元が取得するバイト列として扱う。
//!
//! 公開 API はすべて crate のルートから再公開しており、利用者はルート直下のパスで参照する。
//!
//! # 基本の流れ
//!
//! 解析 → 機械の生成 → 1 ステップ実行または完走実行 → 観測、の順に使う。
//!
//! ```
//! use brainfuck_core::{Machine, MachineConfig, Program, StepResult, StopReason};
//!
//! // `+` を 8 回、`[>++++++++<-]` で 64 を作り、`>+` で 65(`A`)を置いて出力する。
//! let program = Program::parse("++++++++[>++++++++<-]>+.").expect("valid source");
//! let mut machine = Machine::new(program, Vec::new(), MachineConfig::with_max_steps(1_000));
//!
//! // 1 ステップだけ実行し、状態を観測する。
//! assert_eq!(machine.step(), Ok(StepResult::Executed));
//! assert_eq!(machine.snapshot().steps_executed, 1);
//!
//! // 残りを完走させる。正常終了の停止理由が返り、出力は `A` である。
//! assert_eq!(machine.run(), Ok(StopReason::Halted));
//! assert_eq!(machine.output(), &b"A"[..]);
//! assert!(machine.snapshot().halted);
//! assert_eq!(machine.tape().len(), brainfuck_core::TAPE_SIZE);
//! ```
//!
//! # 公開しない契約
//!
//! 括弧対応表(`jump_table`)は crate 内部専用であり、外部からは読めない。
//! 括弧命令の対応先は [`Program::matching_bracket`] だけで問い合わせる。
//!
//! ```compile_fail,E0624
//! let program = brainfuck_core::Program::parse("[]").unwrap();
//! // 括弧対応表の参照は crate 内部専用(非公開メソッド)。
//! let _ = program.jump_table();
//! ```
//!
//! ```compile_fail,E0616
//! let program = brainfuck_core::Program::parse("[]").unwrap();
//! // 括弧対応表のフィールドは非公開。
//! let _ = program.jump_table;
//! ```
//!
//! 実行設定は [`MachineConfig::unlimited`] か [`MachineConfig::with_max_steps`] で作る。
//! 構造体リテラルでの構築は、非網羅的(`#[non_exhaustive]`)であるため外部からはできない。
//!
//! ```compile_fail,E0639
//! use brainfuck_core::MachineConfig;
//! let _ = MachineConfig { max_steps: None };
//! ```

#![forbid(unsafe_code)]

pub mod error;
pub mod instruction;
pub mod machine;
pub mod parser;

pub use error::{ParseError, PointerDirection, RuntimeError};
pub use instruction::Instruction;
pub use machine::{Machine, MachineConfig, MachineSnapshot, StepResult, StopReason, TAPE_SIZE};
pub use parser::Program;
