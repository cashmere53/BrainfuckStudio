//! Tauri コマンドの登録一覧。後続の feature はここへ関数名を足すだけでよい。
//!
//! 登録の一覧は `register_commands!` の 1 か所だけに書く。そこから invoke ハンドラ
//! (`generate_handler!`)と登録名の定数 `REGISTERED_COMMANDS` を同時に作るため、両者がずれない。
//! 権限生成側の一覧 `names::COMMAND_NAMES` とは tests/commands.rs で集合の一致を検査する。

pub mod debugger;
pub mod names;

/// 登録するコマンドの一覧から、invoke ハンドラと登録名の定数を作る。
macro_rules! register_commands {
    ($($command:ident),+ $(,)?) => {
        /// 登録済みコマンド名(関数名と同じ文字列)。
        pub const REGISTERED_COMMANDS: &[&str] = &[$(stringify!($command)),+];

        /// 全コマンドを登録した invoke ハンドラ。
        pub fn invoke_handler<R: tauri::Runtime>(
        ) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$(debugger::$command),+]
        }
    };
}

register_commands!(
    load_source,
    start_session,
    run_batch,
    step,
    reset,
    get_snapshot,
    get_tape_window,
    set_breakpoints,
    pause,
);
