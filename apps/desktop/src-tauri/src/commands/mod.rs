//! Tauri コマンドの登録一覧。後続の feature はここへ関数を足すだけでよい。
//!
//! 登録の一覧は `register_commands!` の 1 か所だけに書く。そこから invoke ハンドラ
//! (`generate_handler!`)と登録名の定数 `REGISTERED_COMMANDS` を同時に作るため、両者がずれない。
//! 権限生成側の一覧 `names::COMMAND_NAMES` とは tests/commands.rs で集合の一致を検査する。
//! 一覧の各項目は `モジュール::関数名` の形で書く(例: `files::new_document`)。

pub mod debugger;
pub mod files;
pub mod names;

/// 登録するコマンドの一覧から、invoke ハンドラと登録名の定数を作る。
macro_rules! register_commands {
    ($($module:ident :: $command:ident),+ $(,)?) => {
        /// 登録済みコマンド名(関数名と同じ文字列)。
        pub const REGISTERED_COMMANDS: &[&str] = &[$(stringify!($command)),+];

        /// 全コマンドを登録した invoke ハンドラ。
        pub fn invoke_handler<R: tauri::Runtime>(
        ) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$($module::$command),+]
        }
    };
}

register_commands!(
    debugger::load_source,
    debugger::start_session,
    debugger::run_batch,
    debugger::step,
    debugger::reset,
    debugger::get_snapshot,
    debugger::get_tape_window,
    debugger::set_breakpoints,
    debugger::pause,
    files::new_document,
    files::open_source_file,
    files::save_source_file,
    files::set_document_dirty,
    files::confirm_exit,
);
