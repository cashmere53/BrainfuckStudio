//! 生成した TypeScript 型が、コミット済みの `apps/desktop/src/generated/` と一致することを確認する。
//!
//! 検査対象は「本 crate が新しく生成した各ファイル」だけ。コミット済みディレクトリに他 crate 由来の
//! ファイルがあっても失敗しない(設計書の Extension Points: ディレクトリ完全一致にしない)。
//! 不一致・欠落時は生成物の手編集か DTO の変更漏れ。更新手順は次のコマンド:
//! `cargo run -p brainfuck-gui-api --example export_bindings`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use brainfuck_gui_api::dto::export_bindings;
use ts_rs::Config;

const COMMITTED_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../apps/desktop/src/generated"
);
const UPDATE_HINT: &str =
    "run `cargo run -p brainfuck-gui-api --example export_bindings` and commit the result";

fn read_ts_files(dir: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path: PathBuf = entry.expect("dir entry").path();
        if path.extension().is_some_and(|ext| ext == "ts") {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            // Windows の autocrlf で CRLF になっても内容は同一とみなす。
            let text = fs::read_to_string(&path)
                .expect("read generated file")
                .replace("\r\n", "\n");
            files.insert(name, text);
        }
    }
    files
}

/// `fresh_dir` の各 `.ts` が `committed_dir` に同名で存在し、内容が一致することを確認する。
/// `committed_dir` 側の余分なファイルは無視する。失敗時は `UPDATE_HINT` を含むメッセージを返す。
fn check_generated(fresh_dir: &Path, committed_dir: &Path) -> Result<(), String> {
    let fresh = read_ts_files(fresh_dir);
    let committed = read_ts_files(committed_dir);

    if fresh.is_empty() {
        return Err(format!("no bindings were generated; {UPDATE_HINT}"));
    }

    for (name, text) in &fresh {
        match committed.get(name) {
            None => {
                return Err(format!(
                    "{name} is missing from the committed generated files; {UPDATE_HINT}"
                ))
            }
            Some(committed_text) if committed_text != text => {
                return Err(format!(
                    "{name} differs from the generated output; {UPDATE_HINT}"
                ))
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// テスト用の一時ディレクトリに `(ファイル名, 内容)` を書き出す。
fn write_files(dir: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        fs::write(dir.join(name), text).expect("write test file");
    }
}

#[test]
fn unrelated_typescript_files_in_committed_dir_are_ignored() {
    let fresh = tempfile::tempdir().expect("temp dir");
    let committed = tempfile::tempdir().expect("temp dir");
    write_files(fresh.path(), &[("A.ts", "export type A = number;\n")]);
    write_files(
        committed.path(),
        &[
            ("A.ts", "export type A = number;\n"),
            ("CompareDto.ts", "export type CompareDto = string;\n"),
        ],
    );

    assert_eq!(check_generated(fresh.path(), committed.path()), Ok(()));
}

#[test]
fn hand_edited_generated_file_fails_with_update_hint() {
    let fresh = tempfile::tempdir().expect("temp dir");
    let committed = tempfile::tempdir().expect("temp dir");
    write_files(fresh.path(), &[("A.ts", "export type A = number;\n")]);
    write_files(committed.path(), &[("A.ts", "export type A = string;\n")]);

    let error = check_generated(fresh.path(), committed.path())
        .expect_err("an edited file must fail the check");
    assert!(
        error.contains("A.ts differs"),
        "unexpected message: {error}"
    );
    assert!(error.contains(UPDATE_HINT), "missing update hint: {error}");
}

#[test]
fn missing_generated_file_fails_with_update_hint() {
    let fresh = tempfile::tempdir().expect("temp dir");
    let committed = tempfile::tempdir().expect("temp dir");
    write_files(
        fresh.path(),
        &[
            ("A.ts", "export type A = number;\n"),
            ("B.ts", "export type B = number;\n"),
        ],
    );
    write_files(committed.path(), &[("A.ts", "export type A = number;\n")]);

    let error = check_generated(fresh.path(), committed.path())
        .expect_err("a missing file must fail the check");
    assert!(
        error.contains("B.ts is missing"),
        "unexpected message: {error}"
    );
    assert!(error.contains(UPDATE_HINT), "missing update hint: {error}");
}

#[test]
fn committed_bindings_match_fresh_export() {
    let out = tempfile::tempdir().expect("temp dir");
    let cfg = Config::default().with_out_dir(out.path());
    export_bindings(&cfg).expect("export bindings");

    if let Err(error) = check_generated(out.path(), Path::new(COMMITTED_DIR)) {
        panic!("{error}");
    }
}
