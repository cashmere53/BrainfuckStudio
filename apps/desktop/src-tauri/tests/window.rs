//! ウィンドウを閉じる確認の判定を検査する(要件 2.7)。
//!
//! 実ウィンドウの `CloseRequested` は単体テストしにくいため、判定を純粋な関数として切り出して検査する。
//! フックの発火とイベントの送出は `window.rs` の薄い部分にだけ置く。

use brainfuck_studio::app_state::AppState;
use brainfuck_studio::window::{close_requires_confirmation, CLOSE_REQUESTED_EVENT};

#[test]
fn a_clean_document_closes_without_confirmation() {
    let state = AppState::default();

    assert!(!close_requires_confirmation(&state));
}

#[test]
fn a_dirty_document_holds_the_close_for_confirmation() {
    let state = AppState::default();
    state
        .document
        .lock()
        .expect("document lock")
        .set_dirty(true);

    assert!(close_requires_confirmation(&state));
}

#[test]
fn saving_clears_the_hold_so_the_window_closes() {
    let state = AppState::default();
    state
        .document
        .lock()
        .expect("document lock")
        .set_dirty(true);
    state
        .document
        .lock()
        .expect("document lock")
        .set_dirty(false);

    assert!(!close_requires_confirmation(&state));
}

#[test]
fn a_poisoned_document_lock_still_decides_from_the_flag() {
    // 別スレッドのパニックで Mutex が汚染されても、閉じる判定はパニックせず旗の値で決まる。
    let state = std::sync::Arc::new(AppState::default());
    let poisoner = std::sync::Arc::clone(&state);
    let _ = std::thread::spawn(move || {
        let mut guard = poisoner.document.lock().expect("document lock");
        guard.set_dirty(true);
        panic!("poison the document lock");
    })
    .join();

    assert!(state.document.is_poisoned());
    assert!(close_requires_confirmation(&state));
}

#[test]
fn the_close_request_event_name_matches_the_frontend_contract() {
    assert_eq!(CLOSE_REQUESTED_EVENT, "close-requested");
}
