//! デバッグセッション(実行制御と停止理由)。
//!
//! [`DebugSession`] は [`Machine`] を所有し、ブレークポイント集合・直近の停止理由・再開フラグを併せ持つ。
//! 生成直後は core の機械の初期状態と同一で、リセットは同じプログラム・入力・実行設定の初期状態へ戻す。
//! 機械の状態を変更する経路は、後続タスクで追加するバッチ実行と 1 命令ステップ、およびリセットだけである。
//!
//! バッチ命令数 [`BatchSize`] は 0 を表現できない。[`StopReason::StepLimitReached`] は
//! 「バッチ命令数への到達」であり、機械の命令数上限(`MachineConfig::max_steps`)の超過とは別物である。
//! 後者は [`RuntimeError::StepLimitExceeded`] のエラーとして報告する(core の契約どおり)。

use std::num::NonZeroU64;

use crate::breakpoint::BreakpointSet;
use crate::cancel::CancelSignal;
use crate::error::RuntimeError;
use crate::machine::{Machine, MachineConfig, MachineSnapshot, StepResult, StopReason};
use crate::parser::Program;
use crate::view::{tape_window_of, StateView, TapeWindow};

/// バッチ実行の既定の命令数。
pub const DEFAULT_BATCH_SIZE: u64 = 1_000;

/// バッチ命令数。1 以上の値だけを表現できる。
///
/// 1 回のバッチ実行で実行する命令数の上限を表す。0 は [`BatchSize::new`] が拒否する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchSize(NonZeroU64);

impl BatchSize {
    /// 既定のバッチ命令数(1,000)。
    pub const DEFAULT: BatchSize = match NonZeroU64::new(DEFAULT_BATCH_SIZE) {
        Some(value) => BatchSize(value),
        None => panic!("DEFAULT_BATCH_SIZE は 0 ではない"),
    };

    /// 命令数 `n` のバッチ命令数を作る。0 の場合は `None` を返す。
    pub const fn new(n: u64) -> Option<BatchSize> {
        match NonZeroU64::new(n) {
            Some(value) => Some(BatchSize(value)),
            None => None,
        }
    }

    /// バッチ命令数の値を返す(常に 1 以上)。
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl Default for BatchSize {
    fn default() -> BatchSize {
        BatchSize::DEFAULT
    }
}

/// バッチ実行の結果。
///
/// `stop` が `Ok` のときは停止理由、`Err` のときは実行エラー(機械は core の規約どおり不変)。
/// `executed` はこの呼び出しで実行した命令数、`snapshot` は通知時点の機械の状態である。
///
/// [`StopReason::StepLimitReached`] は「バッチ命令数に到達した」ことを意味し、
/// 機械の命令数上限(`MachineConfig::max_steps`)の超過ではない。後者は
/// `Err(RuntimeError::StepLimitExceeded)` として `stop` に載る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct BatchReport {
    /// 停止理由、または実行エラー。
    ///
    /// `Ok(StopReason::StepLimitReached)` はバッチ命令数への到達(バッチ幅に達した)を表す。
    /// 機械の命令数上限の超過は `Err(RuntimeError::StepLimitExceeded)` として別に報告される。
    pub stop: Result<StopReason, RuntimeError>,
    /// この呼び出しで実際に実行した命令数。
    pub executed: u64,
    /// 通知時点のスナップショット。
    pub snapshot: MachineSnapshot,
}

/// 1 つのプログラムの実行を制御するデバッグセッション。
///
/// 値を所有し、共有状態を持たないため、別の実行コンテキストへ移動して利用できる。
#[derive(Debug, Clone)]
pub struct DebugSession {
    /// 実行中の機械。
    machine: Machine,
    /// リセット用の初期入力バイト列。
    initial_input: Vec<u8>,
    /// リセット用の実行設定。
    config: MachineConfig,
    /// ブレークポイント集合(命令添字)。リセットをまたいで保持する。
    breakpoints: BreakpointSet,
    /// 直近の停止理由。
    last_stop: Option<StopReason>,
    /// 次の 1 命令でブレークポイント照合を省略するかどうか(再開フラグ)。
    ///
    /// 真になるのは `Breakpoint` で停止したときと Step Into が命令を実行したときで、
    /// 次に 1 命令が実行されると偽に戻る。照合を省略することで、停止位置の命令は必ず前進し、
    /// 同じ位置で停止し続けない。キャンセル・停止・バッチ到達・エラーでは変更しない。
    skip_breakpoint_once: bool,
}

impl DebugSession {
    /// プログラム・入力バイト列・実行設定からセッションを生成する。
    ///
    /// 機械は core の `Machine::new` と同一の初期状態で、ブレークポイントと直近の停止理由は持たない。
    pub fn new(program: Program, input: Vec<u8>, config: MachineConfig) -> DebugSession {
        let breakpoints = BreakpointSet::new(program.len());
        DebugSession {
            machine: Machine::new(program, input.clone(), config),
            initial_input: input,
            config,
            breakpoints,
            last_stop: None,
            skip_breakpoint_once: false,
        }
    }

    /// 機械を読み取り専用で参照する。
    pub fn machine(&self) -> &Machine {
        &self.machine
    }

    /// 機械の状態のスナップショットを返す。
    pub fn snapshot(&self) -> MachineSnapshot {
        self.machine.snapshot()
    }

    /// 直近の停止理由を返す。
    pub fn last_stop(&self) -> Option<StopReason> {
        self.last_stop
    }

    /// ブレークポイント集合を読み取り専用で参照する。
    pub fn breakpoints(&self) -> &BreakpointSet {
        &self.breakpoints
    }

    /// ブレークポイント集合を可変で参照する。実行の前後およびリセットの前後で編集できる。
    pub fn breakpoints_mut(&mut self) -> &mut BreakpointSet {
        &mut self.breakpoints
    }

    /// Step Into: 有効な命令をちょうど 1 個実行する(タスク 3.2)。
    ///
    /// ブレークポイントとキャンセルの指示は参照しない。core の `step()` を 1 回だけ呼ぶ。
    /// - 停止済みなら機械を変えず `Ok(Halted)` を返し、直近の停止理由を `Halted` にする。
    /// - 命令を実行したら再開フラグを真にする。実行後に停止状態に達したなら直近の停止理由を
    ///   `Halted`、達しなければ消去する。
    /// - 実行エラー(ポインタ範囲外・ステップ制限超過)は core のエラーをそのまま返す。
    ///   機械は変えず、直近の停止理由を消去する。
    pub fn step_into(&mut self) -> Result<StepResult, RuntimeError> {
        match self.machine.step() {
            Ok(StepResult::Executed) => {
                self.skip_breakpoint_once = true;
                self.last_stop = if self.machine.is_halted() {
                    Some(StopReason::Halted)
                } else {
                    None
                };
                Ok(StepResult::Executed)
            }
            Ok(StepResult::Halted) => {
                // 停止済みの経路。機械は変更されていない。
                self.last_stop = Some(StopReason::Halted);
                Ok(StepResult::Halted)
            }
            Err(error) => {
                self.last_stop = None;
                Err(error)
            }
        }
    }

    /// バッチ実行: 最大 `size` 個の有効な命令を、core の `step()` と同じ意味論で実行する(タスク 3.3)。
    ///
    /// 各反復の判定順序は次のとおり(停止 → キャンセル → ブレークポイント → バッチ幅 → 実行)。
    /// 1. 停止状態なら実行せず `Halted`(実行数 0 の呼び出しでは状態不変)。
    /// 2. `cancel` が真を返したら実行せず `Cancelled`。命令の直前にちょうど 1 回だけ問い合わせる。
    /// 3. 再開フラグが偽かつ現在の `pc` がブレークポイントなら、実行せず `Breakpoint`。
    ///    照合はバッチ幅の判定より前に行うため、バッチ境界ちょうどの `pc` を見逃さない。
    ///    `Breakpoint` で停止するとき再開フラグを真にする。
    /// 4. 実行数がバッチ命令数に到達していれば `StepLimitReached`。
    /// 5. `step()` を 1 回呼ぶ。`Executed` なら実行数を加算し、再開フラグを偽に戻して 1 へ戻る。
    ///    `Err` ならそのエラーを返す。
    ///
    /// 再開フラグの規則: `Breakpoint` で停止した後、または Step Into が命令を実行した後の
    /// 次の 1 命令だけはブレークポイント照合を省略する。これにより停止位置の命令は必ず前進する。
    /// `Cancelled`・`Halted`・`StepLimitReached`・`Err` では再開フラグを変更しない。
    ///
    /// 最後の 1 命令の実行で停止に達した場合は、次の反復の 1 で `Halted` になる(`StepLimitReached` より優先)。
    /// 機械の命令数上限の超過とポインタ範囲外は `Err` として `stop` に載せ、機械は core の規約どおり不変である。
    /// 直近の停止理由は `Ok` で終わったら `Some(理由)`、`Err` で終わったら消去する。
    pub fn run_batch<C: CancelSignal + ?Sized>(
        &mut self,
        size: BatchSize,
        cancel: &C,
    ) -> BatchReport {
        let limit = size.get();
        let mut executed: u64 = 0;
        let stop = loop {
            if self.machine.is_halted() {
                break Ok(StopReason::Halted);
            }
            if cancel.is_cancelled() {
                break Ok(StopReason::Cancelled);
            }
            if !self.skip_breakpoint_once && self.breakpoints.contains(self.machine.snapshot().pc) {
                self.skip_breakpoint_once = true;
                break Ok(StopReason::Breakpoint);
            }
            if executed >= limit {
                break Ok(StopReason::StepLimitReached);
            }
            match self.machine.step() {
                Ok(StepResult::Executed) => {
                    executed += 1;
                    self.skip_breakpoint_once = false;
                }
                // 停止状態は上の判定で除外済みのため到達しない。防御的に `Halted` として扱う。
                Ok(StepResult::Halted) => break Ok(StopReason::Halted),
                Err(error) => break Err(error),
            }
        };
        self.last_stop = stop.as_ref().ok().copied();
        BatchReport {
            stop,
            executed,
            snapshot: self.machine.snapshot(),
        }
    }

    /// 機械を同じプログラム・入力・実行設定の初期状態へ戻す。
    ///
    /// 直近の停止理由を消去し、再開フラグを偽にする。ブレークポイントは保持する。
    pub fn reset(&mut self) {
        self.machine = Machine::new(
            self.machine.program().clone(),
            self.initial_input.clone(),
            self.config,
        );
        self.last_stop = None;
        self.skip_breakpoint_once = false;
    }

    /// `dp` を中心とする半径 `radius` のテープ表示窓を返す。機械版の `tape_window` へ委譲する。
    pub fn tape_window(&self, radius: usize) -> TapeWindow<'_> {
        crate::view::tape_window(&self.machine, radius)
    }

    /// 状態ビューを返す。スナップショット・窓・出力・直近の停止理由を同一の借用から一貫して作る。
    ///
    /// 出力はバイト列のまま返し、UTF-8 への変換や置換はしない。
    pub fn state_view(&self, radius: usize) -> StateView<'_> {
        let snapshot = self.machine.snapshot();
        StateView {
            snapshot,
            tape_window: tape_window_of(self.machine.tape(), snapshot.data_pointer, radius),
            output: self.machine.output(),
            last_stop: self.last_stop,
        }
    }
}
