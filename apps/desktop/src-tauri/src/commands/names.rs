//! 権限生成用のコマンド名の一覧。
//!
//! `build.rs` が `#[path]` でこのファイルを読み、`AppManifest::commands` を作って `allow-<command>` の権限を生成する。
//! そのため、このファイルは他のモジュールに依存させない。登録の一覧は `commands/mod.rs` の `register_commands!` にあり、
//! 両者が一致することは tests/commands.rs で検査する。コマンドを追加するときは両方へ名前を足す。

/// 権限を生成するコマンド名(関数名と一致させる)。デバッガの操作(4.1)。
pub const COMMAND_NAMES: &[&str] = &[
    "load_source",
    "start_session",
    "run_batch",
    "step",
    "reset",
    "get_snapshot",
    "get_tape_window",
    "set_breakpoints",
    "pause",
];
