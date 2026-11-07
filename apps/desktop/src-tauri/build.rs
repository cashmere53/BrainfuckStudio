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

    embed_common_controls_for_tests();
}

/// 単体テストの実行ファイルへ Common Controls v6 のマニフェストを埋め込む。
///
/// tauri-build が埋め込むマニフェストは bin ターゲットにしか付かない。マニフェストが無いと
/// comctl32 v5 が読まれ、tao/wry が使う `TaskDialogIndirect` が見つからず、テスト実行ファイルが
/// `STATUS_ENTRYPOINT_NOT_FOUND`(0xC0000139)で起動前に落ちる。MSVC 以外では何もしない。
fn embed_common_controls_for_tests() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest = out_dir.join("common-controls-v6.manifest");
    std::fs::write(
        &manifest,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
"#,
    )
    .expect("failed to write the common controls manifest");
    // 統合テスト(tests/)の実行ファイルにだけ付ける。bin のマニフェストと重複させないため。
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
