//! 実行機械の状態と観測。
//!
//! 機械は 1 つの `Program` の実行状態(テープ、ポインタ、実行数、入出力)を保持する。
//! 停止は `pc` が命令数と等しいことで表す。

use crate::error::RuntimeError;
use crate::instruction::Instruction;
use crate::parser::Program;

/// テープのセル数。データポインタの有効範囲は `0..TAPE_SIZE` である。
pub const TAPE_SIZE: usize = 30_000;

/// 実行設定。
///
/// 命令数の上限の有無を指定する。上限を省略した既定値は用意しない。
/// 構造体リテラルでの構築は禁じており、`unlimited` か `with_max_steps` を使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MachineConfig {
    /// 実行命令数の上限。`None` は制限なし。値 `N` は、`N` 個の命令を実行した後に
    /// 未停止であれば次の命令を実行せず、ステップ制限超過を返すことを意味する。
    pub max_steps: Option<u64>,
}

impl MachineConfig {
    /// 命令数の制限を持たない設定を返す。
    pub const fn unlimited() -> Self {
        MachineConfig { max_steps: None }
    }

    /// 実行命令数の上限を `limit` とする設定を返す。
    pub const fn with_max_steps(limit: u64) -> Self {
        MachineConfig {
            max_steps: Some(limit),
        }
    }
}

/// 1 ステップ実行の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    /// 命令を 1 個実行した。
    Executed,
    /// 機械は停止しており、状態は変更されなかった。
    Halted,
}

/// 完走実行(`Machine::run`)が返す停止理由。
///
/// `Machine::run` が `Ok` で返す停止理由は常に `Halted` である。
/// `StepLimitReached`・`Breakpoint`・`Cancelled` は将来の拡張用に予約しており、
/// 現在の `Machine::run` は返さない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    /// プログラムが終了状態に到達した(正常終了)。
    Halted,
    /// 命令数の上限に到達した。予約済みで、`Machine::run` は返さない。
    StepLimitReached,
    /// ブレークポイントで停止した。予約済みで、`Machine::run` は返さない。
    Breakpoint,
    /// キャンセルで停止した。予約済みで、`Machine::run` は返さない。
    Cancelled,
}

/// 機械の状態の観測結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MachineSnapshot {
    /// 命令ポインタ。命令数と等しいとき停止している。
    pub pc: usize,
    /// データポインタ(`0..TAPE_SIZE`)。
    pub data_pointer: usize,
    /// 現在のセルの値。
    pub current_cell: u8,
    /// 実行した命令の総数。
    pub steps_executed: u64,
    /// 停止しているかどうか。
    pub halted: bool,
}

/// 1 つのプログラムの実行状態を保持する機械。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    /// 実行対象のプログラム。
    program: Program,
    /// テープ。全セルが 0 で初期化される。
    tape: Box<[u8; TAPE_SIZE]>,
    /// 命令ポインタ。
    pc: usize,
    /// データポインタ。
    dp: usize,
    /// 実行した命令の総数。
    steps_executed: u64,
    /// 出力バイト列。
    output: Vec<u8>,
    /// 入力バイト列。
    input: Vec<u8>,
    /// 次に読み取る入力の位置。
    input_pos: usize,
    /// 実行設定。
    config: MachineConfig,
}

impl Machine {
    /// プログラム・入力バイト列・実行設定から、初期状態の機械を生成する。
    ///
    /// テープは全セル 0、`pc`・`dp`・実行数・入力読取位置は 0、出力は空となる。
    /// 生成は失敗しない。
    pub fn new(program: Program, input: Vec<u8>, config: MachineConfig) -> Machine {
        Machine {
            program,
            tape: Box::new([0; TAPE_SIZE]),
            pc: 0,
            dp: 0,
            steps_executed: 0,
            output: Vec::new(),
            input,
            input_pos: 0,
            config,
        }
    }

    /// 命令を 1 個実行する。
    ///
    /// 停止済みなら状態を変えず `StepResult::Halted` を返す。
    /// 上限に達していて未停止なら、命令を実行せず `RuntimeError::StepLimitExceeded` を返す。
    /// 範囲外のポインタ移動は `RuntimeError::PointerOutOfBounds` を返し、状態を変更しない。
    pub fn step(&mut self) -> Result<StepResult, RuntimeError> {
        // 判定順序: 停止済み → ステップ上限到達かつ未停止 → 命令実行。
        if self.is_halted() {
            return Ok(StepResult::Halted);
        }
        if let Some(limit) = self.config.max_steps {
            if self.steps_executed >= limit {
                return Err(RuntimeError::StepLimitExceeded { limit, pc: self.pc });
            }
        }
        // 未停止のため `pc` は命令添字の範囲内にある。
        match self.program.instructions()[self.pc] {
            Instruction::Increment => {
                let cell = &mut self.tape[self.dp];
                *cell = cell.wrapping_add(1);
            }
            Instruction::Decrement => {
                let cell = &mut self.tape[self.dp];
                *cell = cell.wrapping_sub(1);
            }
            // ポインタ移動・入出力・ループ命令は後続タスク(4.3〜4.5)で実装する。
            _ => unimplemented!("この命令の実行規則は後続タスクで実装する"),
        }
        // 検証と状態更新が成功した場合のみ、実行数と `pc` を進める。
        self.steps_executed = self.steps_executed.saturating_add(1);
        self.pc += 1;
        Ok(StepResult::Executed)
    }

    /// 停止するまで命令を繰り返し実行する。
    ///
    /// 停止すると `Ok(StopReason::Halted)` を返す。実行エラーが発生すると、そのエラーを
    /// 即座に返し、その時点までの状態を保持する。停止済みの機械では状態を変えず
    /// `Ok(StopReason::Halted)` を返す。
    ///
    /// 実行設定が命令数の上限を持たない場合、プログラムが終了しなければ戻らない可能性がある。
    /// 無限ループを含むプログラムに対しては、呼び出し側が上限を設定する責務を負う。
    pub fn run(&mut self) -> Result<StopReason, RuntimeError> {
        // 本体は未実装であり、呼び出すと panic する。
        unimplemented!("完走実行は未実装である")
    }

    /// 現在の状態を変更せずに観測する。
    pub fn snapshot(&self) -> MachineSnapshot {
        MachineSnapshot {
            pc: self.pc,
            data_pointer: self.dp,
            current_cell: self.tape[self.dp],
            steps_executed: self.steps_executed,
            halted: self.is_halted(),
        }
    }

    /// テープ全体(`TAPE_SIZE` 個のセル)を読み取り専用で返す。
    pub fn tape(&self) -> &[u8] {
        &self.tape[..]
    }

    /// これまでに出力されたバイト列全体を読み取り専用で返す。
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// 機械が停止しているかを返す。`pc` が命令数と等しいとき停止している。
    pub fn is_halted(&self) -> bool {
        self.pc == self.program.len()
    }

    /// 実行対象のプログラムを読み取り専用で返す。
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// 読み取り済みの入力バイト数を返す。
    pub fn input_position(&self) -> usize {
        self.input_pos
    }
}
