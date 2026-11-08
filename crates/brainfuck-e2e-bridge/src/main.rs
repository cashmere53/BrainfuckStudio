//! テスト専用ブリッジの入口。標準入力の要求を受け、標準出力へプロトコルの行だけを書く。

mod dispatch;
mod document;
mod protocol;
mod server;

use std::io;
use std::sync::{Arc, Mutex};

use dispatch::BridgeState;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    server::serve(
        stdin.lock(),
        Arc::new(Mutex::new(stdout)),
        Arc::new(BridgeState::default()),
    );
}
