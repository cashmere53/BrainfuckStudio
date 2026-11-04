//! DTO の TypeScript 型を `apps/desktop/src/generated/` へ書き出す(手動更新用)。
//!
//! 実行: `cargo run -p brainfuck-gui-api --example export_bindings`
//! 生成物はコミットする。生成物を手で編集しないこと(`tests/bindings_up_to_date.rs` が検査する)。

use std::path::Path;

use brainfuck_gui_api::dto::export_bindings;
use ts_rs::Config;

fn main() {
    let out = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/generated"
    ));
    let cfg = Config::default().with_out_dir(out);
    if let Err(error) = export_bindings(&cfg) {
        eprintln!("failed to export bindings: {error}");
        std::process::exit(1);
    }
    println!("exported bindings to {}", out.display());
}
