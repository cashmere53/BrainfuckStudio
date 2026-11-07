//! ウィンドウを閉じる要求の確認(要件 2.7)。
//!
//! 未保存の変更があるときだけ閉じるのを止め、フロントへ `close-requested` を送って確認ダイアログを出させる。
//! 未保存でなければそのまま閉じる(フロントが応答しなくても閉じられる)。確定はフロントが `confirm_exit` で行う。
//! 判定は純粋な関数に切り出して検査し、フックの発火と送出は薄く保つ。

use tauri::{Emitter, Manager, Runtime, Window, WindowEvent};

use crate::app_state::AppState;

/// フロントへ閉じる要求を知らせるイベント名。フロントの購読名(`onCloseRequested`)と一致させる。
pub const CLOSE_REQUESTED_EVENT: &str = "close-requested";

/// 閉じる前に確認が必要か。未保存の変更があるときだけ真。
///
/// 汚染された Mutex でも旗の値で判定する(パニックさせない)。確認なしで閉じて未保存の内容を失わないよう、
/// 旗が立っていれば必ず確認へ回す。
pub fn close_requires_confirmation(state: &AppState) -> bool {
    match state.document.lock() {
        Ok(guard) => guard.dirty,
        Err(poisoned) => poisoned.into_inner().dirty,
    }
}

/// `CloseRequested` のフック。確認が必要なら閉じるのを止めて `close-requested` を送る。
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let state = window.app_handle().state::<AppState>();
    if !close_requires_confirmation(&state) {
        return;
    }
    api.prevent_close();
    if window.emit(CLOSE_REQUESTED_EVENT, ()).is_err() {
        // 送れなくても閉じるのは止めたままにする(未保存の内容を失わないことを優先する)。
        eprintln!("failed to notify the frontend about the close request");
    }
}
