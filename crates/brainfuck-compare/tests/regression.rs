//! 回帰規約(13.10): 最小再現は `tests/regression/*.bf` に置き、1 ファイルにつき 1 テストで読む。
//!
//! 各テストは素朴と最適化・WASM を、上限 0 から必要数 + 2 までの全数で比較する。
//! 新しい最小再現を加えるときは、ファイルを置いてこのファイルにテストを 1 つ足す。

mod common;

use brainfuck_compare::Method;
use common::{needed_steps, sweep_limits};

/// 規約の実証用の初期ケース。命令を含まないコメントだけのファイルで、空プログラムとして扱う。
#[test]
fn regression_comment_only_file_is_the_empty_program() {
    let source = include_str!("regression/comment_only.bf");
    assert_eq!(needed_steps(source, b""), 0, "命令を含まないので必要数は 0");
    for method in [Method::Optimized, Method::Wasm] {
        sweep_limits(source, b"", method);
    }
}
