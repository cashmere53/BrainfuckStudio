//! 素朴意味論の再生(replay)。
//!
//! `start_pc` から core の `step()` と同一の判定順序(停止 → 上限 → 命令の検証 → 状態更新)で
//! 命令を 1 個ずつ再生し、停止またはエラーまで進める。最適化機械の縮退経路であり、下流 crate が
//! 離脱後の再生に再利用する単一の実装である。素朴意味論の基準は core の `Machine` が所有する。

use brainfuck_core::{Instruction, PointerDirection, Program, RuntimeError, StopReason, TAPE_SIZE};
use std::error::Error;
use std::fmt;

/// 素朴再生の実行状態。
///
/// 不変条件: `tape.len() == TAPE_SIZE`、`dp < TAPE_SIZE`、`input_pos <= input.len()`。
/// 構築は [`ReplayState::new`] と [`ReplayState::from_parts`] に限り、不変条件を破る値は作れない。
/// フィールドは crate 内可視で、同 crate の最適化経路(`fast`)が直接更新する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayState {
    /// テープ。長さは `TAPE_SIZE` に固定する。
    pub(crate) tape: Box<[u8]>,
    /// データポインタ(`0..TAPE_SIZE`)。
    pub(crate) dp: usize,
    /// 素朴換算の実行数(実行した命令の総数)。
    pub(crate) steps_executed: u64,
    /// 入力バイト列。
    pub(crate) input: Vec<u8>,
    /// 次に読み取る入力の位置(`0..=input.len()`)。
    pub(crate) input_pos: usize,
    /// 出力バイト列。
    pub(crate) output: Vec<u8>,
}

/// 途中状態の各項目。生成時の入力と、取り出し時の出力に使うプレーンな値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayParts {
    /// テープ。長さは `TAPE_SIZE` でなければならない。
    pub tape: Vec<u8>,
    /// データポインタ。`TAPE_SIZE` 未満でなければならない。
    pub dp: usize,
    /// 素朴換算の実行数。
    pub steps_executed: u64,
    /// 入力バイト列。
    pub input: Vec<u8>,
    /// 次に読み取る入力の位置。`input.len()` 以下でなければならない。
    pub input_pos: usize,
    /// 出力バイト列。
    pub output: Vec<u8>,
}

/// 途中状態の生成時に不変条件が破られたことを示すエラー。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayStateError {
    /// テープ長が `TAPE_SIZE` と一致しない。
    InvalidTapeLength {
        /// 与えられたテープ長。
        actual: usize,
    },
    /// データポインタが `TAPE_SIZE` 以上。
    DataPointerOutOfRange {
        /// 与えられたデータポインタ。
        dp: usize,
    },
    /// 入力読取位置が入力長を超える。
    InputPositionOutOfRange {
        /// 与えられた入力読取位置。
        input_pos: usize,
        /// 入力の長さ。
        input_len: usize,
    },
}

impl fmt::Display for ReplayStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ReplayStateError::InvalidTapeLength { actual } => write!(
                f,
                "invalid tape length: expected {TAPE_SIZE}, actual {actual}"
            ),
            ReplayStateError::DataPointerOutOfRange { dp } => write!(
                f,
                "data pointer out of range: dp={dp}, tape size {TAPE_SIZE}"
            ),
            ReplayStateError::InputPositionOutOfRange {
                input_pos,
                input_len,
            } => write!(
                f,
                "input position out of range: input_pos={input_pos}, input length {input_len}"
            ),
        }
    }
}

impl Error for ReplayStateError {}

impl ReplayState {
    /// 初期状態を生成する。テープは全セル 0、`dp = 0`、実行数 0、入力読取位置 0、出力は空。
    pub fn new(input: Vec<u8>) -> ReplayState {
        ReplayState {
            tape: vec![0; TAPE_SIZE].into_boxed_slice(),
            dp: 0,
            steps_executed: 0,
            input,
            input_pos: 0,
            output: Vec::new(),
        }
    }

    /// 途中状態の各項目から生成する。不変条件を満たさなければ、panic せずエラーを返す。
    ///
    /// 検証順は、テープ長、データポインタ、入力読取位置の順。
    pub fn from_parts(parts: ReplayParts) -> Result<ReplayState, ReplayStateError> {
        if parts.tape.len() != TAPE_SIZE {
            return Err(ReplayStateError::InvalidTapeLength {
                actual: parts.tape.len(),
            });
        }
        if parts.dp >= TAPE_SIZE {
            return Err(ReplayStateError::DataPointerOutOfRange { dp: parts.dp });
        }
        if parts.input_pos > parts.input.len() {
            return Err(ReplayStateError::InputPositionOutOfRange {
                input_pos: parts.input_pos,
                input_len: parts.input.len(),
            });
        }
        Ok(ReplayState {
            tape: parts.tape.into_boxed_slice(),
            dp: parts.dp,
            steps_executed: parts.steps_executed,
            input: parts.input,
            input_pos: parts.input_pos,
            output: parts.output,
        })
    }

    /// 所有権を返して各項目を取り出す。テープは再コピーしない。
    pub fn into_parts(self) -> ReplayParts {
        ReplayParts {
            tape: self.tape.into_vec(),
            dp: self.dp,
            steps_executed: self.steps_executed,
            input: self.input,
            input_pos: self.input_pos,
            output: self.output,
        }
    }

    /// テープ全体(長さ `TAPE_SIZE`)を読み取る。
    pub fn tape(&self) -> &[u8] {
        &self.tape
    }

    /// データポインタを返す。
    pub fn data_pointer(&self) -> usize {
        self.dp
    }

    /// 素朴換算の実行数(実行した命令の総数)を返す。
    pub fn steps_executed(&self) -> u64 {
        self.steps_executed
    }

    /// 入力バイト列を返す。
    pub fn input(&self) -> &[u8] {
        &self.input
    }

    /// 次に読み取る入力の位置を返す。
    pub fn input_position(&self) -> usize {
        self.input_pos
    }

    /// 出力バイト列を返す。
    pub fn output(&self) -> &[u8] {
        &self.output
    }
}

/// 再生の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Replayed {
    /// 終了時の命令添字。停止は命令数、エラーはエラーが報告する `pc`。
    pub pc: usize,
    /// 停止(`Ok(StopReason::Halted)`)またはエラー。上限到達は `StepLimitExceeded`。
    pub result: Result<StopReason, RuntimeError>,
}

/// `start_pc` から core の `step()` と同一の判定順序で、停止またはエラーまで命令を再生する。
///
/// 判定順序は、停止(`pc >= 命令数`)→ 上限(`steps_executed >= limit`)→ 命令の検証 → 状態更新。
/// `limit` が `None` のときは上限を課さない(プログラムが終了しなければ戻らない)。
/// `start_pc` が命令数以上なら状態を変更せず停止を返す(上限 0 でも停止)。
/// エラーは検証段で返し、そのとき状態は変更しない。成功した命令のみ実行数を加算する。
/// 括弧のジャンプは `Program::matching_bracket` の参照のみで行い、探索しない。
/// 本関数は IR 操作の実行回数を扱わない。
pub fn replay(
    program: &Program,
    limit: Option<u64>,
    start_pc: usize,
    state: &mut ReplayState,
) -> Replayed {
    let len = program.len();
    let mut pc = start_pc;
    loop {
        if pc >= len {
            return Replayed {
                pc: len,
                result: Ok(StopReason::Halted),
            };
        }
        if let Some(max) = limit {
            if state.steps_executed >= max {
                return Replayed {
                    pc,
                    result: Err(RuntimeError::StepLimitExceeded { limit: max, pc }),
                };
            }
        }
        match program.instructions()[pc] {
            Instruction::Increment => {
                let cell = &mut state.tape[state.dp];
                *cell = cell.wrapping_add(1);
            }
            Instruction::Decrement => {
                let cell = &mut state.tape[state.dp];
                *cell = cell.wrapping_sub(1);
            }
            Instruction::MoveRight => {
                if state.dp >= TAPE_SIZE - 1 {
                    return Replayed {
                        pc,
                        result: Err(RuntimeError::PointerOutOfBounds {
                            pc,
                            dp: state.dp,
                            direction: PointerDirection::Right,
                        }),
                    };
                }
                state.dp += 1;
            }
            Instruction::MoveLeft => {
                if state.dp == 0 {
                    return Replayed {
                        pc,
                        result: Err(RuntimeError::PointerOutOfBounds {
                            pc,
                            dp: state.dp,
                            direction: PointerDirection::Left,
                        }),
                    };
                }
                state.dp -= 1;
            }
            Instruction::Output => {
                let byte = state.tape[state.dp];
                state.output.push(byte);
            }
            Instruction::Input => match state.input.get(state.input_pos) {
                Some(&byte) => {
                    state.tape[state.dp] = byte;
                    state.input_pos += 1;
                }
                None => state.tape[state.dp] = 0,
            },
            Instruction::LoopStart => {
                if state.tape[state.dp] == 0 {
                    pc = bracket_next(program, pc);
                    state.steps_executed = state.steps_executed.saturating_add(1);
                    continue;
                }
            }
            Instruction::LoopEnd => {
                if state.tape[state.dp] != 0 {
                    pc = bracket_next(program, pc);
                    state.steps_executed = state.steps_executed.saturating_add(1);
                    continue;
                }
            }
        }
        state.steps_executed = state.steps_executed.saturating_add(1);
        pc += 1;
    }
}

/// 括弧命令 `pc` の対応先の次の命令添字を返す。
///
/// 対応先は `Program::matching_bracket` の参照のみで得る。括弧命令では常に `Some` であり、
/// `None` は解析結果の不変条件違反を示す。release では `pc + 1` へ進む。
fn bracket_next(program: &Program, pc: usize) -> usize {
    match program.matching_bracket(pc) {
        Some(matching) => matching + 1,
        None => {
            debug_assert!(false, "bracket at pc {pc} has no matching bracket");
            pc + 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{replay, ReplayParts, ReplayState, ReplayStateError, Replayed};
    use brainfuck_core::{PointerDirection, Program, RuntimeError, StopReason, TAPE_SIZE};

    fn program(source: &str) -> Program {
        Program::parse(source).expect("valid program")
    }

    fn parts_with(tape_cell0: u8, input: Vec<u8>) -> ReplayParts {
        let mut tape = vec![0u8; TAPE_SIZE];
        tape[0] = tape_cell0;
        ReplayParts {
            tape,
            dp: 0,
            steps_executed: 0,
            input,
            input_pos: 0,
            output: Vec::new(),
        }
    }

    #[test]
    fn new_state_is_all_zero_and_empty() {
        let state = ReplayState::new(vec![1, 2]);
        assert_eq!(state.tape().len(), TAPE_SIZE);
        assert!(state.tape().iter().all(|&cell| cell == 0));
        assert_eq!(state.data_pointer(), 0);
        assert_eq!(state.steps_executed(), 0);
        assert_eq!(state.input(), &[1, 2]);
        assert_eq!(state.input_position(), 0);
        assert!(state.output().is_empty());
    }

    #[test]
    fn increment_and_decrement_wrap_modulo_256() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("-"), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state.tape()[0], 255);

        let result = replay(&program("+"), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state.tape()[0], 0);
    }

    #[test]
    fn move_right_and_left_change_the_data_pointer() {
        let mut state = ReplayState::new(Vec::new());
        replay(&program(">>"), None, 0, &mut state);
        assert_eq!(state.data_pointer(), 2);
        replay(&program("<"), None, 0, &mut state);
        assert_eq!(state.data_pointer(), 1);
        assert_eq!(state.steps_executed(), 3);
    }

    #[test]
    fn output_appends_the_current_cell() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("+++."), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state.output(), &[3]);
    }

    #[test]
    fn input_stores_the_next_byte_and_advances() {
        let mut state = ReplayState::new(vec![0x41, 0xFF]);
        replay(&program(",>,"), None, 0, &mut state);
        assert_eq!(state.tape()[0], 0x41);
        assert_eq!(state.tape()[1], 0xFF);
        assert_eq!(state.input_position(), 2);
    }

    #[test]
    fn input_at_eof_stores_zero_without_advancing() {
        let mut state = ReplayState::from_parts(parts_with(7, Vec::new())).expect("valid parts");
        let result = replay(&program(","), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state.tape()[0], 0);
        assert_eq!(state.input_position(), 0);
    }

    #[test]
    fn loop_skips_body_when_cell_is_zero() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("[-]"), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(result.pc, 3);
        assert_eq!(state.steps_executed(), 1);
    }

    #[test]
    fn loop_runs_until_cell_is_zero() {
        let mut state = ReplayState::from_parts(parts_with(3, Vec::new())).expect("valid parts");
        let result = replay(&program("[-]"), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state.tape()[0], 0);
        // 初回の `[` が 1 回、各反復で本体 `-` と末尾 `]` が 1 回ずつ(3 反復)。合計 1 + 3 + 3 = 7。
        assert_eq!(state.steps_executed(), 7);
    }

    #[test]
    fn nested_loop_jumps_use_the_matching_bracket() {
        // 外側 `[` の対応は末尾の `]`、内側 `[` の対応は `]` の直前。
        let mut state = ReplayState::from_parts(parts_with(2, Vec::new())).expect("valid parts");
        let result = replay(&program("[>[-]<-]"), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(result.pc, 8);
        assert_eq!(state.tape()[0], 0);
        assert_eq!(state.data_pointer(), 0);
    }

    #[test]
    fn pointer_out_of_bounds_on_left_leaves_state_unchanged() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("<"), None, 0, &mut state);
        assert_eq!(
            result,
            Replayed {
                pc: 0,
                result: Err(RuntimeError::PointerOutOfBounds {
                    pc: 0,
                    dp: 0,
                    direction: PointerDirection::Left,
                }),
            }
        );
        assert_eq!(state.data_pointer(), 0);
        assert_eq!(state.steps_executed(), 0);
        assert_eq!(state.tape()[0], 0);
    }

    #[test]
    fn pointer_out_of_bounds_on_right_at_last_cell() {
        let parts = ReplayParts {
            tape: vec![0u8; TAPE_SIZE],
            dp: TAPE_SIZE - 1,
            steps_executed: 0,
            input: Vec::new(),
            input_pos: 0,
            output: Vec::new(),
        };
        let mut state = ReplayState::from_parts(parts).expect("valid parts");
        let result = replay(&program(">"), None, 0, &mut state);
        assert_eq!(
            result.result,
            Err(RuntimeError::PointerOutOfBounds {
                pc: 0,
                dp: TAPE_SIZE - 1,
                direction: PointerDirection::Right,
            })
        );
        assert_eq!(state.data_pointer(), TAPE_SIZE - 1);
        assert_eq!(state.steps_executed(), 0);
    }

    #[test]
    fn moving_to_the_last_cell_succeeds() {
        let parts = ReplayParts {
            tape: vec![0u8; TAPE_SIZE],
            dp: TAPE_SIZE - 2,
            steps_executed: 0,
            input: Vec::new(),
            input_pos: 0,
            output: Vec::new(),
        };
        let mut state = ReplayState::from_parts(parts).expect("valid parts");
        let result = replay(&program(">"), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state.data_pointer(), TAPE_SIZE - 1);
    }

    #[test]
    fn error_in_the_middle_reports_the_failing_pc_and_keeps_prior_effects() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("+>+<<"), None, 0, &mut state);
        assert_eq!(
            result,
            Replayed {
                pc: 4,
                result: Err(RuntimeError::PointerOutOfBounds {
                    pc: 4,
                    dp: 0,
                    direction: PointerDirection::Left,
                }),
            }
        );
        assert_eq!(state.steps_executed(), 4);
        assert_eq!(state.tape()[0], 1);
        assert_eq!(state.tape()[1], 1);
    }

    #[test]
    fn empty_program_halts_immediately() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program(""), Some(0), 0, &mut state);
        assert_eq!(
            result,
            Replayed {
                pc: 0,
                result: Ok(StopReason::Halted),
            }
        );
        assert_eq!(state.steps_executed(), 0);
    }

    #[test]
    fn limit_zero_on_a_non_empty_program_fails_on_the_first_step() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("+"), Some(0), 0, &mut state);
        assert_eq!(
            result,
            Replayed {
                pc: 0,
                result: Err(RuntimeError::StepLimitExceeded { limit: 0, pc: 0 }),
            }
        );
        assert_eq!(state.steps_executed(), 0);
        assert_eq!(state.tape()[0], 0);
    }

    #[test]
    fn limit_equal_to_the_program_length_halts_exactly() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("+"), Some(1), 0, &mut state);
        assert_eq!(
            result,
            Replayed {
                pc: 1,
                result: Ok(StopReason::Halted),
            }
        );
        assert_eq!(state.steps_executed(), 1);
    }

    #[test]
    fn exceeding_the_limit_fails_before_the_next_instruction() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("++"), Some(1), 0, &mut state);
        assert_eq!(
            result,
            Replayed {
                pc: 1,
                result: Err(RuntimeError::StepLimitExceeded { limit: 1, pc: 1 }),
            }
        );
        assert_eq!(state.steps_executed(), 1);
        assert_eq!(state.tape()[0], 1);
    }

    #[test]
    fn infinite_loop_stops_at_the_limit_with_step_limit_error() {
        let mut state = ReplayState::new(Vec::new());
        // `+`(1) `[`(2) `]`(3, 非 0 のため `[` の次へ戻る)。上限 3 到達時の pc は `]` の添字 2。
        let result = replay(&program("+[]"), Some(3), 0, &mut state);
        assert_eq!(
            result,
            Replayed {
                pc: 2,
                result: Err(RuntimeError::StepLimitExceeded { limit: 3, pc: 2 }),
            }
        );
        assert_eq!(state.steps_executed(), 3);
    }

    #[test]
    fn limit_is_checked_against_the_accumulated_state_of_a_partial_run() {
        let mut state = ReplayState::new(Vec::new());
        // 1 回目: 上限 2 で `++` の途中まで。
        let first = replay(&program("++++"), Some(2), 0, &mut state);
        assert_eq!(
            first.result,
            Err(RuntimeError::StepLimitExceeded { limit: 2, pc: 2 })
        );
        // 2 回目: 同じ状態から上限 4 で続行すると残りの 2 命令が実行される。
        let second = replay(&program("++++"), Some(4), 2, &mut state);
        assert_eq!(second.result, Ok(StopReason::Halted));
        assert_eq!(state.tape()[0], 4);
        assert_eq!(state.steps_executed(), 4);
    }

    #[test]
    fn start_past_the_end_is_a_halt_without_state_change() {
        let mut state = ReplayState::from_parts(parts_with(5, Vec::new())).expect("valid parts");
        let before = state.clone();
        let result = replay(&program("+++"), Some(0), 3, &mut state);
        assert_eq!(
            result,
            Replayed {
                pc: 3,
                result: Ok(StopReason::Halted),
            }
        );
        assert_eq!(state, before);

        let result = replay(&program("+++"), None, 10, &mut state);
        assert_eq!(result.pc, 3);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state, before);
    }

    #[test]
    fn replay_from_the_middle_starts_at_the_given_pc() {
        let mut state = ReplayState::new(Vec::new());
        let result = replay(&program("+++"), None, 1, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state.tape()[0], 2);
        assert_eq!(state.steps_executed(), 2);
    }

    #[test]
    fn saturating_step_count_does_not_panic() {
        let parts = ReplayParts {
            tape: vec![0u8; TAPE_SIZE],
            dp: 0,
            steps_executed: u64::MAX,
            input: Vec::new(),
            input_pos: 0,
            output: Vec::new(),
        };
        let mut state = ReplayState::from_parts(parts).expect("valid parts");
        let result = replay(&program("+"), None, 0, &mut state);
        assert_eq!(result.result, Ok(StopReason::Halted));
        assert_eq!(state.steps_executed(), u64::MAX);
        assert_eq!(state.tape()[0], 1);
    }

    #[test]
    fn from_parts_round_trips_through_into_parts() {
        let mut tape = vec![0u8; TAPE_SIZE];
        tape[0] = 9;
        tape[TAPE_SIZE - 1] = 200;
        let parts = ReplayParts {
            tape,
            dp: 12,
            steps_executed: 34,
            input: vec![1, 2, 3],
            input_pos: 2,
            output: vec![4, 5],
        };
        let state = ReplayState::from_parts(parts.clone()).expect("valid parts");
        assert_eq!(state.tape(), parts.tape.as_slice());
        assert_eq!(state.data_pointer(), 12);
        assert_eq!(state.steps_executed(), 34);
        assert_eq!(state.input(), &[1, 2, 3]);
        assert_eq!(state.input_position(), 2);
        assert_eq!(state.output(), &[4, 5]);
        assert_eq!(state.into_parts(), parts);
    }

    #[test]
    fn from_parts_rejects_a_wrong_tape_length() {
        let parts = ReplayParts {
            tape: vec![0u8; TAPE_SIZE - 1],
            dp: 0,
            steps_executed: 0,
            input: Vec::new(),
            input_pos: 0,
            output: Vec::new(),
        };
        assert_eq!(
            ReplayState::from_parts(parts),
            Err(ReplayStateError::InvalidTapeLength {
                actual: TAPE_SIZE - 1,
            })
        );
    }

    #[test]
    fn from_parts_rejects_a_data_pointer_outside_the_tape() {
        let parts = ReplayParts {
            tape: vec![0u8; TAPE_SIZE],
            dp: TAPE_SIZE,
            steps_executed: 0,
            input: Vec::new(),
            input_pos: 0,
            output: Vec::new(),
        };
        assert_eq!(
            ReplayState::from_parts(parts),
            Err(ReplayStateError::DataPointerOutOfRange { dp: TAPE_SIZE })
        );
    }

    #[test]
    fn from_parts_rejects_an_input_position_past_the_input() {
        let parts = ReplayParts {
            tape: vec![0u8; TAPE_SIZE],
            dp: 0,
            steps_executed: 0,
            input: vec![1],
            input_pos: 2,
            output: Vec::new(),
        };
        assert_eq!(
            ReplayState::from_parts(parts),
            Err(ReplayStateError::InputPositionOutOfRange {
                input_pos: 2,
                input_len: 1,
            })
        );
    }

    #[test]
    fn state_errors_have_fixed_messages_with_numbers() {
        let error = ReplayStateError::InputPositionOutOfRange {
            input_pos: 2,
            input_len: 1,
        };
        let message = error.to_string();
        assert!(message.contains('2'));
        assert!(message.contains('1'));
        let as_std: &dyn std::error::Error = &error;
        assert_eq!(as_std.to_string(), message);
    }
}
