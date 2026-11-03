//! 実行機械の状態と観測。
//!
//! 機械は 1 つの `Program` の実行状態(テープ、ポインタ、実行数、入出力)を保持する。
//! 停止は `pc` が命令数と等しいことで表す。

use crate::error::{PointerDirection, RuntimeError};
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
/// 「命令数の上限に達した」という出来事の通知は層ごとに 2 つの規約があり、形が異なる。
/// 取り違えないよう次の対応表に従う。
///
/// | 層 | 上限の種類 | 通知の形 |
/// |----|-----------|----------|
/// | core `Machine::run` | `MachineConfig.max_steps`(総実行数の上限) | `Err(RuntimeError::StepLimitExceeded)` |
/// | debugger-state のバッチ | バッチ幅(1 回の呼び出しの実行数) | `Ok(StopReason::StepLimitReached)` |
///
/// したがって、ステップ上限到達の完走実行は停止理由ではなくエラーで返す。
/// `Machine::run` が `Ok` で返す停止理由は常に `Halted`(正常終了)である。
/// `StepLimitReached`・`Breakpoint`・`Cancelled` は将来の拡張用に予約しており、
/// 現在の `Machine::run` は返さない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    /// プログラムが終了状態に到達した(正常終了)。`Machine::run` が返す唯一の停止理由。
    Halted,
    /// バッチ幅などの上限に到達した。予約済みで、`Machine::run` は返さない。
    /// 総実行数の上限到達は `Err(RuntimeError::StepLimitExceeded)` で表す。
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
            Instruction::MoveRight => {
                // 検証段: 範囲外なら状態を変更せずに返す。
                if self.dp >= TAPE_SIZE - 1 {
                    return Err(RuntimeError::PointerOutOfBounds {
                        pc: self.pc,
                        dp: self.dp,
                        direction: PointerDirection::Right,
                    });
                }
                self.dp += 1;
            }
            Instruction::MoveLeft => {
                // 検証段: 範囲外なら状態を変更せずに返す。
                if self.dp == 0 {
                    return Err(RuntimeError::PointerOutOfBounds {
                        pc: self.pc,
                        dp: self.dp,
                        direction: PointerDirection::Left,
                    });
                }
                self.dp -= 1;
            }
            Instruction::Output => {
                // 出力は現在セルの値を 1 バイトとして末尾に追加する。状態検証は不要。
                self.output.push(self.tape[self.dp]);
            }
            Instruction::Input => {
                // 未読の入力があれば次の 1 バイトを格納し、読取位置を進める。
                // 入力が尽きていれば現在セルに 0 を格納し、読取位置は進めない。
                match self.input.get(self.input_pos) {
                    Some(&byte) => {
                        self.tape[self.dp] = byte;
                        self.input_pos += 1;
                    }
                    None => self.tape[self.dp] = 0,
                }
            }
            // ループ開始: セルが 0 なら対応する `]` の次へ進む。非 0 なら次の命令へ進む。
            // 括弧対応表の参照のみを使い、括弧を探索しない。セルと dp は変更しない。
            // 対応する `]` の添字を `pc` に設定し、末尾の `pc += 1` で `]` の次へ進ませる。
            Instruction::LoopStart => {
                if self.tape[self.dp] == 0 {
                    self.pc = self.program.jump_table()[self.pc];
                }
            }
            // ループ終了: セルが非 0 なら対応する `[` の次へ戻る。0 なら次の命令へ進む。
            // 対応する `[` の添字を `pc` に設定し、末尾の `pc += 1` で `[` の次へ戻らせる。
            Instruction::LoopEnd => {
                if self.tape[self.dp] != 0 {
                    self.pc = self.program.jump_table()[self.pc];
                }
            }
        }
        // 検証と状態更新が成功した場合のみ、実行数と `pc` を進める。
        // ループ命令は `pc` を対応先に設定しているため、ここでの加算により対応先の次へ進む。
        self.steps_executed = self.steps_executed.saturating_add(1);
        self.pc += 1;
        Ok(StepResult::Executed)
    }

    /// 停止するまで命令を繰り返し実行する。
    ///
    /// 停止すると `Ok(StopReason::Halted)` を返す。実行エラーが発生すると、そのエラーを
    /// 即座に返し、その時点までの出力・テープ・`pc`・`dp`・実行数を保持する。
    /// 停止済みの機械では状態を変えず `Ok(StopReason::Halted)` を返す。
    ///
    /// 実行設定の命令数の上限に到達して未停止のまま終わる場合は、停止理由ではなく
    /// `Err(RuntimeError::StepLimitExceeded)` を返す。`Ok` で返す停止理由は常に `Halted` である。
    /// 上限の種類ごとの通知の形は [`StopReason`] の対応表を参照。
    ///
    /// 実行設定が命令数の上限を持たない場合、プログラムが終了しなければ戻らない可能性がある。
    /// 無限ループを含むプログラムに対しては、呼び出し側が上限を設定する責務を負う。
    ///
    /// # Examples
    ///
    /// 上限 3 のとき、無限ループは実行数が 3 に達した時点でステップ制限超過により中断する。
    ///
    /// ```
    /// use brainfuck_core::{Machine, MachineConfig, Program, RuntimeError};
    ///
    /// let program = Program::parse("+[]").expect("valid");
    /// let mut machine = Machine::new(program, Vec::new(), MachineConfig::with_max_steps(3));
    /// let error = machine.run().unwrap_err();
    /// assert!(matches!(error, RuntimeError::StepLimitExceeded { limit: 3, .. }));
    /// assert_eq!(machine.snapshot().steps_executed, 3);
    /// ```
    pub fn run(&mut self) -> Result<StopReason, RuntimeError> {
        // `step` の結果が `Executed` の間だけ繰り返す。実行エラーは `?` で即座に返し、
        // その時点の状態(出力・テープ・pc・dp・実行数)をそのまま保持する。
        loop {
            match self.step()? {
                StepResult::Executed => continue,
                StepResult::Halted => return Ok(StopReason::Halted),
            }
        }
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
