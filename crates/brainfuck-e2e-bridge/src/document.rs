//! 文書状態(開いているファイルの紐づけ先・改行コード・未保存の旗)。
//!
//! 複製元: `apps/desktop/src-tauri/src/app_state.rs` の `DocumentState`(既定値と遷移)、
//! `commands/files.rs`(読み書き、2.1 で複製)、`window.rs`(閉じる判定)。
//! 複製元が変わったら `apps/desktop/e2e/support/bridge/tauriFingerprint.test.ts` の指紋が失敗する。
//! その場合はこのファイルと `dispatch.rs` を複製元に合わせ、単体テストを通してから指紋を更新する。

use std::path::{Path, PathBuf};

use brainfuck_gui_api::dto::{ApiError, ApiErrorKind, IoOperation};
use brainfuck_gui_api::limits::MAX_SOURCE_BYTES;
use brainfuck_gui_api::source_file::{decode_source, DecodedSource, LineEnding};

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

impl BridgeDocument {
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

/// ファイルを読み込んでデコードする。上限を超える場合は中身を読まずに拒否する。
///
/// 複製元は `files.rs::read_source`(同じ規則)。
pub fn read_source(path: &Path) -> Result<DecodedSource, ApiError> {
    let metadata =
        std::fs::metadata(path).map_err(|error| ApiError::io(IoOperation::Read, error))?;
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err(ApiError::new(ApiErrorKind::FileTooLarge, None));
    }
    let bytes = std::fs::read(path).map_err(|error| ApiError::io(IoOperation::Read, error))?;
    decode_source(&bytes)
}

/// バイト列をそのままファイルへ書き込む。失敗は `io`(書き込み)に写し、文面にパスを含めない。
///
/// 複製元は `files.rs::write_bytes`(同じ規則)。
pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), ApiError> {
    std::fs::write(path, bytes).map_err(|error| ApiError::io(IoOperation::Write, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("一時ディレクトリを作れること")
    }

    #[test]
    fn default_is_new_unnamed_lf_clean_document() {
        let document = BridgeDocument::default();
        assert_eq!(document.path, None);
        assert_eq!(document.line_ending, LineEnding::Lf);
        assert!(!document.dirty);
    }

    #[test]
    fn reset_to_new_clears_path_line_ending_and_dirty() {
        let mut document = BridgeDocument::default();
        document.opened(PathBuf::from("a.bf"), LineEnding::CrLf);
        document.set_dirty(true);
        document.reset_to_new();
        assert_eq!(document, BridgeDocument::default());
    }

    #[test]
    fn opened_records_path_and_line_ending_and_clears_dirty() {
        let mut document = BridgeDocument::default();
        document.set_dirty(true);
        document.opened(PathBuf::from("a.bf"), LineEnding::CrLf);
        assert_eq!(document.path, Some(PathBuf::from("a.bf")));
        assert_eq!(document.line_ending, LineEnding::CrLf);
        assert!(!document.dirty);
    }

    #[test]
    fn saved_updates_path_and_clears_dirty_but_keeps_line_ending() {
        let mut document = BridgeDocument::default();
        document.opened(PathBuf::from("a.bf"), LineEnding::CrLf);
        document.set_dirty(true);
        document.saved(PathBuf::from("b.bf"));
        assert_eq!(document.path, Some(PathBuf::from("b.bf")));
        assert_eq!(document.line_ending, LineEnding::CrLf);
        assert!(!document.dirty);
    }

    #[test]
    fn set_dirty_records_the_flag() {
        let mut document = BridgeDocument::default();
        document.set_dirty(true);
        assert!(document.dirty);
        document.set_dirty(false);
        assert!(!document.dirty);
    }

    #[test]
    fn read_source_rejects_a_file_over_one_mib_as_too_large() {
        let dir = temp_dir();
        let path = dir.path().join("big.bf");
        std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES + 1]).unwrap();
        let error = read_source(&path).expect_err("1 MiB を超えるファイルは拒否される");
        assert_eq!(error.kind, ApiErrorKind::FileTooLarge);
    }

    /// 上限超過は中身を読む前に拒否される(事前検査が効いていること)。
    ///
    /// 読み込みを失敗させる環境を作り、`read` まで進んだ場合は I/O エラーになって区別できる。
    #[cfg(windows)]
    #[test]
    fn read_source_rejects_an_oversized_file_without_reading_it() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = temp_dir();
        let path = dir.path().join("locked-big.bf");
        std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES + 1]).unwrap();
        // 共有モード 0(排他)で開いたままにする。メタデータ取得は成功し、読み込みは共有違反で失敗する。
        let _locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        let error =
            read_source(&path).expect_err("1 MiB を超えるファイルは中身を読まずに拒否される");
        assert_eq!(error.kind, ApiErrorKind::FileTooLarge);
    }

    /// 上限超過は中身を読む前に拒否される(事前検査が効いていること)。
    ///
    /// 読み取り権限を外し、`read` まで進んだ場合は I/O エラーになって区別できる。
    /// root など権限を無視できる環境では読めてしまうので、そのときは検証せずに終える。
    #[cfg(unix)]
    #[test]
    fn read_source_rejects_an_oversized_file_without_reading_it() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir();
        let path = dir.path().join("locked-big.bf");
        std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES + 1]).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&path).is_ok() {
            // 権限が効かない環境(root など)では区別できないため、検証を省く。
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            return;
        }
        let error =
            read_source(&path).expect_err("1 MiB を超えるファイルは中身を読まずに拒否される");
        assert_eq!(error.kind, ApiErrorKind::FileTooLarge);
    }

    #[test]
    fn read_source_accepts_a_file_of_exactly_one_mib() {
        let dir = temp_dir();
        let path = dir.path().join("edge.bf");
        std::fs::write(&path, vec![b'+'; MAX_SOURCE_BYTES]).unwrap();
        let decoded = read_source(&path).expect("1 MiB ちょうどは読める");
        assert_eq!(decoded.text.len(), MAX_SOURCE_BYTES);
    }

    #[test]
    fn read_source_replaces_invalid_utf8_and_reports_lossy() {
        let dir = temp_dir();
        let path = dir.path().join("sjis.bf");
        // Shift_JIS の「あ」(0x82 0xA0)は UTF-8 として不正
        std::fs::write(&path, [b'+', 0x82, 0xA0, b'-']).unwrap();
        let decoded = read_source(&path).unwrap();
        assert!(decoded.lossy);
        assert!(decoded.text.contains('\u{FFFD}'));
        assert!(decoded.text.starts_with('+') && decoded.text.ends_with('-'));
    }

    #[test]
    fn read_source_records_crlf_and_normalizes_text_to_lf() {
        let dir = temp_dir();
        let path = dir.path().join("crlf.bf");
        std::fs::write(&path, b"+>\r\n-<\r\n").unwrap();
        let decoded = read_source(&path).unwrap();
        assert_eq!(decoded.line_ending, LineEnding::CrLf);
        assert_eq!(decoded.text, "+>\n-<\n");
        assert!(!decoded.lossy);
    }

    #[test]
    fn read_source_reports_missing_file_as_io_read_error_without_path() {
        let dir = temp_dir();
        let path = dir.path().join("missing.bf");
        let error = read_source(&path).expect_err("存在しないファイルは失敗する");
        assert_eq!(error.kind, ApiErrorKind::Io);
        assert!(error.message.starts_with("failed to read file: "));
        assert!(!error.message.contains("missing.bf"));
    }

    #[test]
    fn write_bytes_writes_the_given_bytes() {
        let dir = temp_dir();
        let path = dir.path().join("out.bf");
        write_bytes(&path, b"+[>+<-]").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"+[>+<-]");
    }

    #[test]
    fn write_bytes_into_missing_directory_is_io_write_error_without_path() {
        let dir = temp_dir();
        let path = dir.path().join("no-such-dir").join("out.bf");
        let error =
            write_bytes(&path, b"+").expect_err("存在しないディレクトリへの書き込みは失敗する");
        assert_eq!(error.kind, ApiErrorKind::Io);
        assert!(error.message.starts_with("failed to write file: "));
        assert!(!error.message.contains("out.bf"));
        assert!(!error.message.contains("no-such-dir"));
    }
}
