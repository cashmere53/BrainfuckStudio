//! Tauri コマンドの登録一覧。後続の feature はここへモジュールを追加し、`invoke_handler` の一覧に関数を足す。
//! `generate_handler!` はこのファイルの一覧からのみ構築する(`lib.rs` に個別のコマンド名を書かない)。

#[allow(dead_code)]
mod names;

/// 全コマンドを登録した invoke ハンドラ。現時点ではコマンド未実装のため空の一覧。
pub fn invoke_handler<R: tauri::Runtime>(
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![]
}
