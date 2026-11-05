//! Brainfuck Studio の Tauri 層。起動・プラグイン登録・コマンド登録のみを行い、ロジックは `brainfuck-gui-api` へ置く。

mod commands;

/// アプリを起動する。
pub fn run() {
    tauri::Builder::default()
        // ダイアログは Rust 側からのみ使う。WebView への権限は capabilities で与えない。
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(commands::invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running Brainfuck Studio");
}
