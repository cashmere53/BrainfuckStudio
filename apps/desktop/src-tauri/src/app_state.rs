//! アプリの管理状態(`tauri::State` として登録する)。
//!
//! `service` はデバッガのセッションを持つ。`document` は開いているファイルの情報(パス・改行コード・未保存フラグ)で、
//! パスは Rust 側にのみ保持し、WebView へは表示名のみを返す(要求 2.11、13.4)。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use brainfuck_compare_api::ComparisonService;
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

impl DocumentState {
    /// 空の新規文書へ戻す(パスなし・LF・未保存なし)。
    pub fn reset_to_new(&mut self) {
        *self = Self::default();
    }

    /// ファイルを読み込んだ状態へ移す。改行コードを記録し、未保存の変更は無い。
    pub fn opened(&mut self, path: PathBuf, line_ending: LineEnding) {
        self.path = Some(path);
        self.line_ending = line_ending;
        self.dirty = false;
    }

    /// ファイルへ書き込んだ後の状態へ移す。紐づけ先を更新し、未保存の変更は無い。
    pub fn saved(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.dirty = false;
    }

    /// 未保存の変更の有無を記録する。
    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }
}

/// アプリの管理状態。
#[derive(Default)]
pub struct AppState {
    /// デバッガのサービス(`Send + Sync`)。
    pub service: DebuggerService,
    /// 文書の状態。
    pub document: Mutex<DocumentState>,
    /// 比較のサービス(`Send + Sync`)。デバッガのサービスとは共有せず、状態も持たない。
    pub comparison: Arc<ComparisonService>,
}
