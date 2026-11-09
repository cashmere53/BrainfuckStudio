//! 生成物と実行側が共有する取り決め(import 名と署名、export 名と署名、メモリ配置、状態コード)。
//!
//! 生成物の形は次のとおりで、監査(`audit`)はこれ以外を拒否する。
//!
//! - import: `bf.read : () -> i32`(未読バイトがあれば 1 バイト、尽きていれば 0 を返し位置を進めない)、
//!   `bf.write : (i32) -> ()`(出力量の上限を超えれば実行を中断する)
//! - export: `run : (i64) -> (i32, i32, i32, i64)`(引数は命令数上限。戻り値は `(状態コード, pc, dp, 実行数)`)、
//!   `memory`(1 ページ、拡張不可)
//!
//! メモリ配置: 線形メモリ先頭 `TAPE_SIZE`(30,000)バイトがテープ。残りは未使用。

#![allow(dead_code)] // 生成・監査・実行の配線は後続タスク(2.x)で行う

use brainfuck_core::TAPE_SIZE;

/// import のモジュール名。
pub(crate) const IMPORT_MODULE: &str = "bf";
/// 入力取得関数の名前(`bf.read`)。
pub(crate) const IMPORT_READ: &str = "read";
/// 出力送出関数の名前(`bf.write`)。
pub(crate) const IMPORT_WRITE: &str = "write";
/// 実行関数の export 名。
pub(crate) const EXPORT_RUN: &str = "run";
/// メモリの export 名。
pub(crate) const EXPORT_MEMORY: &str = "memory";

/// メモリのページ数(最小 = 最大 = 1、拡張不可)。
pub(crate) const MEMORY_PAGES: u64 = 1;
/// テープの長さ(線形メモリ先頭のバイト数)。素朴インタプリタの `TAPE_SIZE` と同一。
pub(crate) const TAPE_BYTES: usize = TAPE_SIZE;

/// 状態コード: 停止(`pc` = 命令数)。
pub(crate) const STATUS_HALTED: i32 = 0;
/// 状態コード: 離脱(`pc` = 離脱した操作の先頭の命令添字)。
pub(crate) const STATUS_BAILOUT: i32 = 1;
