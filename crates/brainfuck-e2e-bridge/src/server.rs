//! 受信ループ、要求ごとのスレッド、ダイアログの返事の振り分け、行単位の書き込み。
//! 骨格(1.1)では入力を読み捨てて EOF で戻るだけ。実装は 2.4 で行う。

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use crate::dispatch::BridgeState;

/// 入力が終わる(EOF)まで受信する。
pub fn serve<R: BufRead, W: Write + Send + 'static>(
    input: R,
    output: Arc<Mutex<W>>,
    state: Arc<BridgeState>,
) {
    let _ = (&output, &state);
    for _line in input.lines() {}
}
