//! キャンセル指示の契約(`CancelSignal`)。
//!
//! 呼び出し元が供給する「現在キャンセルが指示されているか」を、スレッド・UI 非依存の形で受け取る。
//! 問い合わせ(`is_cancelled`)は副作用を持たず、呼び出しごとに現在の指示状態を返す。
//! デバッガ(`run_batch`)は各命令を実行する直前にちょうど 1 回この問い合わせを行い、
//! 真を観測した時点で、その命令を実行せずに中断する。
//!
//! 指示の設定側は、共有される真偽値(`AtomicBool`)を `Release` または `SeqCst` で書き込むこと。
//! 問い合わせ側はこの実装で `Acquire` により読むため、設定側の書き込みより前の出来事が見える。
//! 取り下げ(偽への書き戻し)も次の問い合わせから反映される。
//!
//! 本モジュールは標準ライブラリの原子型のみに依存し、外部 crate やスレッド生成は使わない。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// キャンセル指示の供給元の契約。
///
/// 一度 `true` を返した後に `false` を返してもよい(呼び出し側が指示を取り下げる場合)。
/// デバッガは `true` を観測した時点で中断する。
pub trait CancelSignal {
    /// 現在キャンセルが指示されているかを返す。副作用を持たない観測として扱う。
    fn is_cancelled(&self) -> bool;
}

/// キャンセルを指示しない供給元。常に偽を返す。
///
/// キャンセルを使わない呼び出しで用いる。
#[derive(Debug, Clone, Copy, Default)]
pub struct NeverCancel;

impl CancelSignal for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// 共有される真偽値。取得時は `Acquire` で読む。
///
/// 設定側は `Release` または `SeqCst` で書き込むこと。
impl CancelSignal for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Acquire)
    }
}

/// クロージャを包むキャンセル指示の供給元。
///
/// クロージャは呼び出しごとに現在の指示状態を返す。
/// 呼び出し回数に応じて応答を変えるクロージャは、決定的なキャンセルの再現(テスト)に用いる。
#[derive(Debug, Clone, Copy)]
pub struct CancelFn<F>(pub F);

impl<F: Fn() -> bool> CancelSignal for CancelFn<F> {
    fn is_cancelled(&self) -> bool {
        (self.0)()
    }
}

/// 参照越しの供給元は、参照先の応答をそのまま返す。
impl<T: CancelSignal + ?Sized> CancelSignal for &T {
    fn is_cancelled(&self) -> bool {
        (**self).is_cancelled()
    }
}

/// 共有ポインタ越しの供給元は、中身の応答をそのまま返す。
impl<T: CancelSignal + ?Sized> CancelSignal for Arc<T> {
    fn is_cancelled(&self) -> bool {
        (**self).is_cancelled()
    }
}
