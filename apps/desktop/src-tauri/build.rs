// tauri-build のフック。コマンド一覧は `src/commands/names.rs` の 1 か所に置き、
// ここで `AppManifest::commands` へ渡してコマンドごとの権限(`allow-<command>`)を生成する。

#[path = "src/commands/names.rs"]
mod command_names;

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/commands/names.rs");

    // `tauri.conf.json` の frontendDist(`../dist`)は compile 時に存在が要求される。
    // フロントのビルド前でも通るよう、無ければ空ディレクトリを仮に作る(中身は vite build が出力する)。
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(manifest_dir.join("..").join("dist"))
        .expect("failed to create the placeholder frontendDist directory");

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(command_names::COMMAND_NAMES)),
    )
    .expect("failed to run tauri-build");
}
