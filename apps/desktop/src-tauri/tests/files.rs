//! ファイル操作の純粋な部分を検査する(読み込み・書き込みの変換、I/O エラーの文面、文書状態の遷移)。
//! ダイアログは実行時にのみ呼ぶため、ここでは扱わない。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, IoOperation, SaveFileRequest, SaveFileResult,
};
use brainfuck_gui_api::limits::MAX_SOURCE_BYTES;
use brainfuck_gui_api::source_file::LineEnding;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};

use brainfuck_studio::app_state::{AppState, DocumentState};
use brainfuck_studio::commands::files::{
    io_failure, read_source, save_needs_dialog, save_source_file, write_source,
};

/// テスト用の一時ディレクトリ。終了時に削除する。
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "bfstudio-files-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir is created");
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// エラーの文面と項目名のどこにもパス(全体・ファイル名)が現れないことを検査する。
fn assert_no_path(error: &ApiError, path: &Path) {
    let file_name = path
        .file_name()
        .expect("path has a file name")
        .to_string_lossy()
        .into_owned();
    let full = path.display().to_string();
    let text = format!(
        "{} {}",
        error.message,
        error.field.clone().unwrap_or_default()
    );
    assert!(!text.contains(&full), "error leaks the path: {text}");
    assert!(
        !text.contains(&file_name),
        "error leaks the file name: {text}"
    );
}

#[test]
fn read_source_decodes_lossy_and_reports_the_first_line_ending() {
    let dir = TempDir::new("decode");
    let path = dir.path("a.bf");
    std::fs::write(&path, [b'+', 0xFF, b'\r', b'\n', b'.']).expect("fixture is written");

    let decoded = read_source(&path).expect("read succeeds");

    assert_eq!(decoded.text, "+\u{FFFD}\n.");
    assert!(decoded.lossy);
    assert_eq!(decoded.line_ending, LineEnding::CrLf);
}

#[test]
fn read_source_rejects_a_file_over_the_limit_without_leaking_the_path() {
    let dir = TempDir::new("too-large");
    let path = dir.path("big.bf");
    std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES + 1]).expect("fixture is written");

    let error = read_source(&path).expect_err("an oversized file is rejected");

    assert_eq!(error.kind, ApiErrorKind::FileTooLarge);
    assert_no_path(&error, &path);
}

#[test]
fn read_failure_reports_the_os_cause_without_the_path() {
    let dir = TempDir::new("read-missing");
    let path = dir.path("missing.bf");

    let error = read_source(&path).expect_err("a missing file is rejected");

    assert_eq!(error.kind, ApiErrorKind::Io);
    assert!(
        error.message.starts_with("failed to read file: "),
        "unexpected message: {}",
        error.message
    );
    assert_no_path(&error, &path);
}

#[test]
fn read_source_accepts_a_file_of_exactly_the_limit() {
    let dir = TempDir::new("exact-limit");
    let path = dir.path("exact.bf");
    std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES]).expect("fixture is written");

    let decoded = read_source(&path).expect("a file of exactly the limit is accepted");

    assert_eq!(decoded.text.len(), MAX_SOURCE_BYTES);
}

#[test]
fn read_failure_after_metadata_succeeds_reports_the_os_cause_without_the_path() {
    // ディレクトリはメタデータの取得に成功し、`fs::read` の段階で失敗する。
    let dir = TempDir::new("read-dir");
    let target = dir.path("as-dir");
    std::fs::create_dir(&target).expect("fixture directory is created");

    let error = read_source(&target).expect_err("a directory is not readable as a file");

    assert_eq!(error.kind, ApiErrorKind::Io);
    assert!(
        error.message.starts_with("failed to read file: "),
        "unexpected message: {}",
        error.message
    );
    assert_no_path(&error, &target);
}

#[test]
fn write_source_stores_the_text_with_the_recorded_line_ending_and_no_temp_file() {
    let dir = TempDir::new("write");
    let path = dir.path("out.bf");

    write_source(&path, "+\n.", LineEnding::CrLf).expect("write succeeds");

    assert_eq!(std::fs::read(&path).expect("file is readable"), b"+\r\n.");
    let entries = std::fs::read_dir(&dir.0).expect("dir is readable").count();
    assert_eq!(entries, 1, "only the target file must exist");
}

#[test]
fn write_source_rejects_text_that_grows_over_the_limit_before_touching_the_disk() {
    let dir = TempDir::new("grow");
    let path = dir.path("grown.bf");
    // 本文は上限以内だが、CRLF へ戻すと上限を超える。
    let text = "\n".repeat(MAX_SOURCE_BYTES / 2 + 1);

    let error = write_source(&path, &text, LineEnding::CrLf).expect_err("the output is too large");

    assert_eq!(error.kind, ApiErrorKind::SourceTooLarge);
    assert!(!path.exists(), "nothing must be written");
}

#[test]
fn write_failure_reports_the_os_cause_without_the_path() {
    let dir = TempDir::new("write-missing-dir");
    let path = dir.path("no-such-dir").join("out.bf");

    let error = write_source(&path, "+", LineEnding::Lf).expect_err("the write fails");

    assert_eq!(error.kind, ApiErrorKind::Io);
    assert!(
        error.message.starts_with("failed to write file: "),
        "unexpected message: {}",
        error.message
    );
    assert_no_path(&error, &path);
}

#[test]
fn io_failure_keeps_only_the_operation_and_the_os_text() {
    let os_error = std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "Access is denied. (os error 5)",
    );

    let error = io_failure(IoOperation::Write, &os_error);

    assert_eq!(error.kind, ApiErrorKind::Io);
    assert_eq!(
        error.message,
        "failed to write file: Access is denied. (os error 5)"
    );
    assert_eq!(error.field, None);
}

#[test]
fn save_asks_for_a_destination_only_when_unbound_or_saving_as() {
    let path = Path::new("x.bf");

    assert!(save_needs_dialog(None, false));
    assert!(save_needs_dialog(None, true));
    assert!(save_needs_dialog(Some(path), true));
    assert!(!save_needs_dialog(Some(path), false));
}

#[test]
fn document_state_follows_open_save_dirty_and_new() {
    let mut document = DocumentState::default();

    document.set_dirty(true);
    assert!(document.dirty);

    document.opened(PathBuf::from("a.bf"), LineEnding::CrLf);
    assert_eq!(document.path.as_deref(), Some(Path::new("a.bf")));
    assert_eq!(document.line_ending, LineEnding::CrLf);
    assert!(!document.dirty);

    document.set_dirty(true);
    document.saved(PathBuf::from("b.bf"));
    assert_eq!(document.path.as_deref(), Some(Path::new("b.bf")));
    assert_eq!(
        document.line_ending,
        LineEnding::CrLf,
        "saving keeps the recorded line ending"
    );
    assert!(!document.dirty);

    document.set_dirty(true);
    document.reset_to_new();
    assert_eq!(document.path, None);
    assert_eq!(document.line_ending, LineEnding::Lf);
    assert!(!document.dirty);
}

// ---- `save_source_file` を直接呼ぶ(`saveAs=false` かつ紐づけ先ありの経路。ダイアログは開かない) ----

fn mock_app() -> App<MockRuntime> {
    mock_builder()
        .manage(AppState::default())
        .build(mock_context(noop_assets()))
        .expect("mock app builds")
}

/// 紐づけ先のパスを記録し、未保存の変更ありの状態にする。
fn bound_dirty_document(app: &App<MockRuntime>, path: &Path, line_ending: LineEnding) {
    let state = app.state::<AppState>();
    let mut document = state.document.lock().expect("document lock is free");
    document.opened(path.to_path_buf(), line_ending);
    document.set_dirty(true);
}

fn is_dirty(app: &App<MockRuntime>) -> bool {
    app.state::<AppState>()
        .document
        .lock()
        .expect("document lock is free")
        .dirty
}

fn save(app: &App<MockRuntime>, source: &str) -> Result<SaveFileResult, ApiError> {
    tauri::async_runtime::block_on(save_source_file(
        app.handle().clone(),
        SaveFileRequest {
            source: source.to_owned(),
            save_as: false,
        },
        app.state::<AppState>(),
    ))
}

#[test]
fn save_failure_keeps_the_document_dirty() {
    // 紐づけ先の親ディレクトリが無いため書き込みは失敗する。失敗時は未保存のままでなければならない。
    let dir = TempDir::new("save-missing-dir");
    let path = dir.path("no-such-dir").join("out.bf");
    let app = mock_app();
    bound_dirty_document(&app, &path, LineEnding::Lf);

    let error = save(&app, "+\n.").expect_err("the write fails");

    assert_eq!(error.kind, ApiErrorKind::Io);
    assert!(
        error.message.starts_with("failed to write file: "),
        "unexpected message: {}",
        error.message
    );
    assert_no_path(&error, &path);
    assert!(
        is_dirty(&app),
        "a failed save must not clear the dirty flag"
    );
}

#[test]
fn save_writes_with_the_recorded_line_ending_and_clears_the_dirty_flag() {
    let dir = TempDir::new("save-ok");
    let path = dir.path("out.bf");
    let app = mock_app();
    bound_dirty_document(&app, &path, LineEnding::CrLf);

    match save(&app, "+\n.").expect("save succeeds") {
        SaveFileResult::Saved { file_name } => assert_eq!(file_name, "out.bf"),
        SaveFileResult::Cancelled => panic!("a bound document must not ask for a destination"),
    }

    assert_eq!(std::fs::read(&path).expect("file is readable"), b"+\r\n.");
    assert!(!is_dirty(&app), "a successful save clears the dirty flag");
}
