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
//! # デバッグの基本の流れ
//!
//! 解析 → ソース位置変換の構築 → セッション生成 → ブレークポイント設定 → バッチ実行 → 状態ビュー、の順に使う。
//! 停止位置は位置変換の範囲でソース上の場所に対応付けられる。キャンセル指示は使わない例である。
//!
//! ```
//! use brainfuck_core::{
//!     BatchSize, DebugSession, MachineConfig, NeverCancel, Program, SourceMap, StopReason,
//!     DEFAULT_WINDOW_RADIUS,
//! };
//!
//! // コメント文字(`comment` の部分)は命令ではない。命令は `+` `+` `+` `.` の 4 個である。
//! let source = "++ comment\n+.";
//! let program = Program::parse(source).expect("valid source");
//! let source_map = SourceMap::new(&program, source).expect("source matches program");
//! let mut session = DebugSession::new(program, Vec::new(), MachineConfig::with_max_steps(1_000));
//!
//! // 3 番目の `+`(命令添字 2)にブレークポイントを置く。
//! assert_eq!(session.breakpoints_mut().insert(2), Ok(true));
//!
//! // 1 回目のバッチ実行は命令添字 2 の直前で停止する。
//! let report = session.run_batch(BatchSize::DEFAULT, &NeverCancel);
//! assert_eq!(report.stop, Ok(StopReason::Breakpoint));
//! assert_eq!(report.executed, 2);
//! assert_eq!(report.snapshot.pc, 2);
//!
//! // 停止位置の命令は、ソース上の 2 行目 1 列目にある。
//! let range = source_map.range_of(report.snapshot.pc).expect("instruction in range");
//! assert_eq!((range.start.line, range.start.column), (2, 1));
//!
//! // 状態ビューは窓・出力・直近の停止理由を一度に返す。
//! let view = session.state_view(DEFAULT_WINDOW_RADIUS);
//! assert_eq!(view.last_stop, Some(StopReason::Breakpoint));
//! assert_eq!(view.snapshot.data_pointer, 0);
//! assert_eq!(view.tape_window.cells[view.tape_window.pointer_offset], 2);
//!
//! // 再開すると同じ位置で止まらず、残りを実行して正常終了する。出力は 3 のバイトである。
//! let report = session.run_batch(BatchSize::DEFAULT, &NeverCancel);
//! assert_eq!(report.stop, Ok(StopReason::Halted));
//! assert_eq!(session.state_view(DEFAULT_WINDOW_RADIUS).output, &[3u8][..]);
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
//!
//! # 非網羅的な型の契約
//!
//! `#[non_exhaustive]` の付いた型は、外部 crate から構造体リテラルで作れず、
//! ワイルドカードなしの網羅的な `match` も書けない。将来の追加を破壊的変更にしないための規約である。
//!
//! ```compile_fail,E0639
//! use brainfuck_core::MachineSnapshot;
//! // 観測結果の構造体リテラルは非網羅的のため外部からは作れない。
//! let _ = MachineSnapshot {
//!     pc: 0,
//!     data_pointer: 0,
//!     current_cell: 0,
//!     steps_executed: 0,
//!     halted: true,
//! };
//! ```
//!
//! ```compile_fail,E0004
//! use brainfuck_core::StopReason;
//! // 予約バリアントを含め全て列挙しても、非網羅的な enum はワイルドカードを要求される。
//! fn label(reason: StopReason) -> &'static str {
//!     match reason {
//!         StopReason::Halted => "halted",
//!         StopReason::StepLimitReached => "step limit",
//!         StopReason::Breakpoint => "breakpoint",
//!         StopReason::Cancelled => "cancelled",
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use brainfuck_core::ParseError;
//! // 解析エラーの enum も非網羅的のため、ワイルドカードなしの match はできない。
//! fn label(error: ParseError) -> &'static str {
//!     match error {
//!         ParseError::UnmatchedClosingBracket { .. } => "closing",
//!         ParseError::UnmatchedOpeningBracket { .. } => "opening",
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use brainfuck_core::RuntimeError;
//! // 実行エラーの enum も非網羅的のため、ワイルドカードなしの match はできない。
//! fn label(error: RuntimeError) -> &'static str {
//!     match error {
//!         RuntimeError::PointerOutOfBounds { .. } => "pointer",
//!         RuntimeError::StepLimitExceeded { .. } => "limit",
//!     }
//! }
//! ```

#![forbid(unsafe_code)]

pub mod breakpoint;
pub mod cancel;
pub mod error;
pub mod instruction;
pub mod machine;
pub mod parser;
pub mod session;
pub mod source_map;
pub mod view;

pub use error::{ParseError, PointerDirection, RuntimeError};
pub use instruction::Instruction;
pub use machine::{Machine, MachineConfig, MachineSnapshot, StepResult, StopReason, TAPE_SIZE};
pub use parser::Program;

// デバッグ拡張の公開項目は各モジュールから glob で再公開する(crate ルート直下のパスで使える)。
pub use breakpoint::*;
pub use cancel::*;
pub use session::*;
pub use source_map::*;
pub use view::*;
