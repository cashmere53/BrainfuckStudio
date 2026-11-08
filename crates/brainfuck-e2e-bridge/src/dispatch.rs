//! 共有の型(ブリッジの状態、ダイアログの問い合わせ口)。コマンド名から呼び出しへの対応は 2.2 で追加する。

use std::path::PathBuf;
use std::sync::Mutex;

use brainfuck_gui_api::dto::ApiError;
use brainfuck_gui_api::service::DebuggerService;

use crate::document::BridgeDocument;
use crate::protocol::DialogKind;

/// ブリッジの状態: デバッガのサービスと文書状態。要求ごとのスレッドから共有する。
#[derive(Default)]
pub struct BridgeState {
    pub service: DebuggerService,
    pub document: Mutex<BridgeDocument>,
}

/// ダイアログの問い合わせ口。ワーカースレッドから呼び、返事が届くまでブロックする。
/// 取消は `Ok(None)`、選ばれたパスは `Ok(Some(path))` になる。
pub trait DialogPort {
    fn pick(&self, kind: DialogKind) -> Result<Option<PathBuf>, ApiError>;
}
