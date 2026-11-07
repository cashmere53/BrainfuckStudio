//! Brainfuck Studio の Tauri 層。起動・プラグイン登録・コマンド登録のみを行い、ロジックは `brainfuck-gui-api` へ置く。

pub mod app_state;
pub mod commands;
pub mod window;

/// アプリを起動する。
pub fn run() {
    tauri::Builder::default()
        // ダイアログは Rust 側からのみ使う。WebView への権限は capabilities で与えない。
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state::AppState::default())
        .on_window_event(window::on_window_event)
        .invoke_handler(commands::invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running Brainfuck Studio");
}
