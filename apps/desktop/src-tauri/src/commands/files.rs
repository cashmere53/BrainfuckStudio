//! 文書とファイルのコマンド。パスは Rust 側にだけ保持し、WebView へはファイル名のみを返す(要件 2.11, 13.4)。
//!
//! ダイアログは Rust 側でのみ開く(WebView の dialog 権限は与えない)。ファイル I/O は `std::fs` で行い、
//! バイト列の検査・デコード・改行コードの扱いは `brainfuck-gui-api` の `source_file` に任せる。
//! 失敗の文面にはパスも内容も含めない(要件 10.9, 13.7)。

use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

use brainfuck_gui_api::dto::{
    ApiError, ApiErrorKind, IoOperation, OpenFileResult, SaveFileRequest, SaveFileResult,
    SetDocumentDirtyRequest,
};
use brainfuck_gui_api::limits::MAX_SOURCE_BYTES;
use brainfuck_gui_api::source_file::{
    decode_source, display_name, encode_source, DecodedSource, LineEnding,
};
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder, FilePath};

use crate::app_state::{AppState, DocumentState};

/// 読み込み・保存のダイアログで出すフィルタ(`.bf` / `.b` / `.txt` と全ファイル)。
const TEXT_EXTENSIONS: &[&str] = &["bf", "b", "txt"];

/// I/O の失敗を応答用のエラーへ変換する。文面は操作の種類と OS の文のみで、パスを含まない。
pub fn io_failure(operation: IoOperation, error: &std::io::Error) -> ApiError {
    ApiError::io(operation, error)
}

/// 保存先を尋ねる必要があるか。紐づけ先が無いとき、または「名前を付けて保存」のときに尋ねる。
pub fn save_needs_dialog(path: Option<&Path>, save_as: bool) -> bool {
    save_as || path.is_none()
}

/// ファイルを読み込んでデコードする。上限を超える場合は中身を読まずに拒否する。
pub fn read_source(path: &Path) -> Result<DecodedSource, ApiError> {
    let metadata =
        std::fs::metadata(path).map_err(|error| io_failure(IoOperation::Read, &error))?;
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err(ApiError::new(ApiErrorKind::FileTooLarge, None));
    }
    let bytes = std::fs::read(path).map_err(|error| io_failure(IoOperation::Read, &error))?;
    decode_source(&bytes)
}

/// 本文を記録した改行コードへ戻して書き込む。一時ファイルは作らない。
///
/// 変換が上限を超える場合は、ファイルに触れる前に拒否する。
pub fn write_source(path: &Path, text: &str, line_ending: LineEnding) -> Result<(), ApiError> {
    let bytes = encode_source(text, line_ending)?;
    write_bytes(path, &bytes)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), ApiError> {
    std::fs::write(path, bytes).map_err(|error| io_failure(IoOperation::Write, &error))
}

fn document(state: &AppState) -> Result<MutexGuard<'_, DocumentState>, ApiError> {
    state
        .document
        .lock()
        .map_err(|_| ApiError::new(ApiErrorKind::Internal, None))
}

/// ダイアログの結果をパスへ変換する。取消は `None`。
fn to_path(picked: Option<FilePath>) -> Result<Option<PathBuf>, ApiError> {
    picked
        .map(|file| {
            file.into_path()
                .map_err(|_| ApiError::new(ApiErrorKind::Io, None))
        })
        .transpose()
}

/// 文書用のファイルダイアログを作る。親はメインウィンドウにする(取得できないときは親なしで開く)。
fn document_dialog<R: Runtime>(app: &AppHandle<R>) -> FileDialogBuilder<R> {
    let builder = app.dialog().file();
    match app.get_webview_window("main") {
        Some(window) => builder.set_parent(&window),
        None => builder,
    }
}

/// 開くファイルを選ばせる。取消は `None`。
fn pick_open_path<R: Runtime>(app: &AppHandle<R>) -> Result<Option<PathBuf>, ApiError> {
    to_path(
        document_dialog(app)
            .add_filter("Brainfuck / テキスト", TEXT_EXTENSIONS)
            .add_filter("すべてのファイル", &["*"])
            .blocking_pick_file(),
    )
}

/// 保存先を選ばせる。取消は `None`。
fn pick_save_path<R: Runtime>(app: &AppHandle<R>) -> Result<Option<PathBuf>, ApiError> {
    to_path(
        document_dialog(app)
            .add_filter("Brainfuck / テキスト", TEXT_EXTENSIONS)
            .add_filter("すべてのファイル", &["*"])
            .blocking_save_file(),
    )
}

/// 空の新規文書へ戻す。
#[tauri::command]
pub async fn new_document(state: State<'_, AppState>) -> Result<(), ApiError> {
    document(&state)?.reset_to_new();
    Ok(())
}

/// ダイアログでファイルを選び、読み込んで文書にする。取消のときは何も変えない。
#[tauri::command]
pub async fn open_source_file<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<OpenFileResult, ApiError> {
    let Some(path) = pick_open_path(&app)? else {
        return Ok(OpenFileResult::Cancelled);
    };
    let decoded = read_source(&path)?;
    let file_name = display_name(&path);
    document(&state)?.opened(path, decoded.line_ending);
    Ok(OpenFileResult::Opened {
        source: decoded.text,
        file_name,
        lossy: decoded.lossy,
    })
}

/// 保存する。紐づけ先が無いか「名前を付けて保存」なら保存先を選ばせる。取消は書き込まない。
#[tauri::command]
pub async fn save_source_file<R: Runtime>(
    app: AppHandle<R>,
    args: SaveFileRequest,
    state: State<'_, AppState>,
) -> Result<SaveFileResult, ApiError> {
    let (current, line_ending) = {
        let guard = document(&state)?;
        (guard.path.clone(), guard.line_ending)
    };
    // 保存先を選ぶ前に変換を検査し、変換できない内容で利用者に保存先を選ばせない。
    let bytes = encode_source(&args.source, line_ending)?;
    let path = match current {
        Some(path) if !save_needs_dialog(Some(&path), args.save_as) => path,
        _ => match pick_save_path(&app)? {
            Some(path) => path,
            None => return Ok(SaveFileResult::Cancelled),
        },
    };
    write_bytes(&path, &bytes)?;
    let file_name = display_name(&path);
    document(&state)?.saved(path);
    Ok(SaveFileResult::Saved { file_name })
}

/// 未保存の変更の有無を記録する(閉じる確認に使う)。
#[tauri::command]
pub async fn set_document_dirty(
    args: SetDocumentDirtyRequest,
    state: State<'_, AppState>,
) -> Result<(), ApiError> {
    document(&state)?.set_dirty(args.dirty);
    Ok(())
}

/// 未保存の確認を終えたあとにアプリを終了する。
#[tauri::command]
pub async fn confirm_exit<R: Runtime>(app: AppHandle<R>) -> Result<(), ApiError> {
    app.exit(0);
    Ok(())
}
