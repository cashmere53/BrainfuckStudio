//! キャンセル指示(`CancelSignal`)の単体テスト。
//!
//! 常に偽の実装、共有される真偽値、参照・共有ポインタ・動的ディスパッチ経由の応答、
//! クロージャの呼び出し回数に応じた応答を確認する。別スレッドは使わず決定的に検証する。

use brainfuck_core::{CancelFn, CancelSignal, NeverCancel};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// 任意の `CancelSignal` 実装の現在の応答を返す(検証用の局所ヘルパ)。
fn answer<C: CancelSignal + ?Sized>(signal: &C) -> bool {
    signal.is_cancelled()
}

#[test]
fn never_cancel_always_answers_false() {
    let signal = NeverCancel;
    for _ in 0..100 {
        assert!(!answer(&signal));
    }
    assert!(!NeverCancel.is_cancelled());
}

#[test]
fn atomic_bool_follows_the_shared_flag_without_becoming_sticky() {
    let flag = AtomicBool::new(false);
    assert!(!answer(&flag));

    flag.store(true, Ordering::Release);
    assert!(answer(&flag));
    assert!(answer(&flag), "指示が有効な間は繰り返し真を返す");

    // 指示の取り下げ(偽への書き戻し)は次の確認で反映される。
    flag.store(false, Ordering::SeqCst);
    assert!(!answer(&flag));
}

#[test]
fn reference_to_atomic_bool_delegates() {
    let flag = AtomicBool::new(false);
    let by_ref: &AtomicBool = &flag;
    let by_double_ref: &&AtomicBool = &by_ref;

    assert!(!answer(by_ref));
    assert!(!answer(by_double_ref));

    flag.store(true, Ordering::Release);
    assert!(answer(by_ref));
    assert!(answer(by_double_ref));
}

#[test]
fn arc_shares_the_flag_between_clones() {
    let flag = Arc::new(AtomicBool::new(false));
    let controller = Arc::clone(&flag);
    let observer = Arc::clone(&flag);

    assert!(!answer(&observer));

    controller.store(true, Ordering::Release);
    assert!(answer(&observer), "設定側の書き込みが観測側に見える");

    controller.store(false, Ordering::SeqCst);
    assert!(!answer(&observer));
}

#[test]
fn arc_of_trait_object_delegates() {
    let flag = Arc::new(AtomicBool::new(false));
    let shared: Arc<dyn CancelSignal> = Arc::new(Arc::clone(&flag));

    assert!(!answer(shared.as_ref()));
    assert!(!answer(&shared));

    flag.store(true, Ordering::Release);
    assert!(answer(shared.as_ref()));
    assert!(answer(&shared));
}

#[test]
fn trait_object_reference_delegates() {
    let flag = AtomicBool::new(true);
    let dynamic: &dyn CancelSignal = &flag;
    assert!(answer(dynamic));

    let never: &dyn CancelSignal = &NeverCancel;
    assert!(!answer(never));
}

#[test]
fn closure_wrapper_answers_by_call_count() {
    // 3 回目の確認から有効になる指示を、呼び出し回数だけで決定的に再現する。
    let calls = Cell::new(0_usize);
    let signal = CancelFn(|| {
        let n = calls.get() + 1;
        calls.set(n);
        n >= 3
    });

    assert!(!answer(&signal));
    assert!(!answer(&signal));
    assert!(answer(&signal));
    assert!(answer(&signal));
    assert_eq!(calls.get(), 4, "確認ごとにちょうど 1 回呼ばれる");
}

#[test]
fn closure_wrapper_can_be_shared_behind_arc_and_reference() {
    let calls = AtomicUsize::new(0);
    let signal = Arc::new(CancelFn(|| {
        calls.fetch_add(1, Ordering::SeqCst);
        false
    }));

    assert!(!answer(&signal));
    assert!(!answer(&*signal));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn closure_wrapper_over_shared_flag_reads_the_flag_each_time() {
    let flag = AtomicBool::new(false);
    let signal = CancelFn(|| flag.load(Ordering::Acquire));

    assert!(!answer(&signal));
    flag.store(true, Ordering::Release);
    assert!(answer(&signal));
}
