//! アプリのコマンド名の一覧(唯一の登録元)。
//!
//! `build.rs` がこの一覧から `AppManifest::commands` を作り、コマンドごとの権限(`allow-<command>`)を生成する。
//! コマンドを追加するときは、ここへ名前を足し、`commands/mod.rs` の `generate_handler!` にも同じ関数を足す。
//! ライブラリ側では直接参照しないため `commands/mod.rs` で `allow(dead_code)` にしている。

/// 登録済みコマンド名。現時点ではコマンド未実装のため空。
pub const COMMAND_NAMES: &[&str] = &[];
