//! アプリの管理状態(`tauri::State` として登録する)。
//!
//! `service` はデバッガのセッションを持つ。`document` は開いているファイルの情報(パス・改行コード・未保存フラグ)で、
//! パスは Rust 側にのみ保持し、WebView へは表示名のみを返す(要求 2.11、13.4)。

use std::path::PathBuf;
use std::sync::Mutex;

use brainfuck_gui_api::service::DebuggerService;
use brainfuck_gui_api::source_file::LineEnding;

/// 開いている文書の状態。
#[derive(Debug)]
pub struct DocumentState {
    /// 紐づいているファイルのパス(新規文書や未保存の文書では `None`)。
    pub path: Option<PathBuf>,
    /// 保存時に復元する改行コード。
    pub line_ending: LineEnding,
    /// 未保存の変更があるかどうか。
    pub dirty: bool,
}

impl Default for DocumentState {
    fn default() -> Self {
        Self {
            path: None,
            line_ending: LineEnding::Lf,
            dirty: false,
        }
    }
}

/// アプリの管理状態。
#[derive(Default)]
pub struct AppState {
    /// デバッガのサービス(`Send + Sync`)。
    pub service: DebuggerService,
    /// 文書の状態。
    pub document: Mutex<DocumentState>,
}
