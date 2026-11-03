//! 回帰テスト(12.10)。
//!
//! 規約: 不具合が見つかったら、(1) 最小の `.bf` を `tests/regression/` に置き、
//! (2) それを実行して期待する終了コード・出力を検証するテストをこのファイルに追加し、
//! (3) テストの直前に事象コメント(何が起きたか、期待される挙動、関連コミットや Issue)を書く。
//! 1 不具合につき 1 ケース以上を、このファイルの中で独立したテストとして置く。
//!
//! 初期ケース: 空のソースファイル `tests/regression/empty_program.bf` は終了コード 0、標準出力空。

mod common;

use common::{fixture_path, run_cli, StdinMode};

/// 事象: (初期ケース、不具合なし)空のソースファイルがエラーになったり出力を書いたりしないこと。
/// 期待: 終了コード 0、標準出力空、標準エラー空。プログラムは入力命令を持たないため標準入力を読まない。
#[test]
fn empty_program_file_exits_zero_with_no_output() {
    let path = fixture_path("regression/empty_program.bf");
    let output = run_cli(&[path.as_os_str()], StdinMode::OpenUnused);

    assert_eq!(
        output.code,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
