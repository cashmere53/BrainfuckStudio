//! Brainfuck 実行エンジンの中核 crate。
//!
//! 構文解析と実行機械を提供する。標準入出力・ファイルシステム・UI には依存しない。

#![forbid(unsafe_code)]

pub mod error;
pub mod instruction;
pub mod machine;
pub mod parser;

pub use error::{ParseError, PointerDirection, RuntimeError};
pub use instruction::Instruction;
pub use machine::{Machine, MachineConfig, MachineSnapshot, StepResult, StopReason, TAPE_SIZE};
pub use parser::Program;
