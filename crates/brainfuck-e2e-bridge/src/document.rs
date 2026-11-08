//! 文書状態(開いているファイルの紐づけ先・改行コード・未保存の旗)。
//!
//! 複製元: `apps/desktop/src-tauri/src/app_state.rs` の `DocumentState`(既定値と遷移)、
//! `commands/files.rs`(読み書き、2.1 で複製)、`window.rs`(閉じる判定)。
//! 複製元が変わったら `apps/desktop/e2e/support/bridge/tauriFingerprint.test.ts` の指紋が失敗する。
//! その場合はこのファイルと `dispatch.rs` を複製元に合わせ、単体テストを通してから指紋を更新する。

use std::path::PathBuf;

use brainfuck_gui_api::source_file::LineEnding;

/// 文書状態。既定はパスなし・LF・未保存なし(複製元の `DocumentState` と同じ)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeDocument {
    /// 紐づけ先のパス(新規のときは `None`)。
    pub path: Option<PathBuf>,
    /// 保存時に使う改行コード。
    pub line_ending: LineEnding,
    /// 未保存の変更があるか。
    pub dirty: bool,
}

impl Default for BridgeDocument {
    fn default() -> Self {
        Self {
            path: None,
            line_ending: LineEnding::Lf,
            dirty: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_new_unnamed_lf_clean_document() {
        let document = BridgeDocument::default();
        assert_eq!(document.path, None);
        assert_eq!(document.line_ending, LineEnding::Lf);
        assert!(!document.dirty);
    }
}
