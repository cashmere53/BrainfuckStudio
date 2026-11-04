//! 最適化インタプリタ(`brainfuck-optimized`)。
//!
//! `brainfuck-core` の素朴インタプリタと同じ意味論を保ったまま、連続命令の統合と代表的ループの一括適用によって
//! 実行を速くする。素朴インタプリタを唯一のオラクルとする差分テストで、観測的な一致を確認している。
//!
//! # 実行数の 2 つの定義
//!
//! このクレートは実行数を 2 種類報告する。名称が示すとおり、両者は**別の定義**である。
//!
//! - **素朴換算の実行数**(`steps_executed`): 素朴実装が同じ実行で数える実行数と同じ値。
//!   一括適用した操作は、それが表す素朴実装の命令の実行回数だけ加算する。ステップ制限はこの値に対して適用する。
//! - **IR 操作の実行回数**(`ir_ops_executed`): 一括適用した IR 操作の回数(方式固有の実行単位数)。
//!   縮退して素朴な 1 命令ずつの再生に回った命令は数えない。常に `ir_ops_executed <= steps_executed` が成り立つ。
//!
//! # 観測的一致の範囲
//!
//! 同じプログラム・入力・設定で素朴実装と完走まで実行したとき、次の項目は完全に一致する。
//! 正常停止の場合だけでなく、エラーで中断した場合(ポインタ範囲外、ステップ制限超過)も同様である。
//!
//! - 終了種別とエラーの値(位置・数値情報を含む)
//! - 出力バイト列、テープ全体(30,000 セル)、データポインタ、入力読取位置、停止状態
//! - 素朴換算の実行数
//!
//! `ir_ops_executed` は素朴実装に対応する値がないため、比較対象外である。
//!
//! # 命令位置 `pc`
//!
//! 観測 API(`OptimizedSnapshot::pc`)と実行エラーが報告する位置は、常に **core の命令添字**である。
//! 最適化後の IR 添字は観測 API とエラーには現れない。IR 添字は `OptimizedProgram::ops()` の添字と、
//! `OpKind::LoopStart`/`LoopEnd` のフィールドにだけ現れる。
//!
//! # 制限なし設定
//!
//! 上限を持たない設定(`MachineConfig::unlimited()`)で終了しないプログラムを実行すると、
//! 実行は終了せず戻らない。素朴実装と同じ扱いであり、上限を設定するかどうかは呼び出し側が決める。
//!
//! # 使用例
//!
//! 解析・変換・機械の生成・実行・観測・素朴実装との比較を行う。最適化側の型はルート直下から利用する。
//! 解析と素朴実装は `brainfuck-core` の型を使う。
//!
//! ```
//! use brainfuck_core::{Machine, MachineConfig, Program, StopReason};
//! use brainfuck_optimized::{OptimizedMachine, OptimizedProgram};
//!
//! // 解析と変換
//! let program = Program::parse("++++++++[>++++++++<-]>+.").expect("括弧は整合している");
//! let optimized = OptimizedProgram::compile(&program);
//!
//! // 最適化機械の生成と実行
//! let config = MachineConfig::with_max_steps(10_000);
//! let mut machine = OptimizedMachine::new(&optimized, Vec::new(), config);
//! assert_eq!(machine.run(), Ok(StopReason::Halted));
//!
//! // 観測(読み取り専用)
//! let snapshot = machine.snapshot();
//! assert!(snapshot.halted);
//! assert_eq!(machine.output(), b"A");
//!
//! // 素朴実装との比較
//! let mut naive = Machine::new(program, Vec::new(), config);
//! assert_eq!(naive.run(), Ok(StopReason::Halted));
//! assert_eq!(naive.output(), machine.output());
//! assert_eq!(naive.snapshot().steps_executed, snapshot.steps_executed);
//! assert!(snapshot.ir_ops_executed <= snapshot.steps_executed);
//! ```

#![forbid(unsafe_code)]

mod compile;
mod fast;
mod ir;
mod machine;
pub mod replay;
mod state;

pub use ir::{Op, OpKind, OptimizedProgram, Term};
pub use machine::{OptimizedMachine, OptimizedSnapshot};
