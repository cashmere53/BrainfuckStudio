//! 公開 replay(`brainfuck_optimized::replay`)の契約を、外部 crate の視点で公開 API のみを使って検証する。
//!
//! 対象: 途中状態の検証付き生成と読み戻し(14.2)、判定順序とエラー時の状態不変(14.3)、
//! 初期状態・途中状態からの再生が core の完走実行と一致すること(14.4)、
//! 終了 pc と結果の返却、上限の扱い、開始位置が命令数以上の場合の停止(14.5)。
//! オラクルは core の `Machine` である。共通基盤(`tests/common`)は使わず、このファイル内で完結する。

use brainfuck_core::{
    Machine, MachineConfig, PointerDirection, Program, RuntimeError, StopReason, TAPE_SIZE,
};
use brainfuck_optimized::replay::{replay, ReplayParts, ReplayState, ReplayStateError};

/// 停止の有無を判定するために core を実行する打ち切り上限。
const CAP: u64 = 256;

/// 比較に使う入力。EOF だけの入力と、0・非 0・最大値を含む入力。
const INPUTS: [&[u8]; 2] = [&[], &[0x41, 0x00, 0xFF]];

fn parse(source: &str) -> Program {
    Program::parse(source).expect("テスト用ソースは解析に成功する必要がある")
}

/// 長さ 4 以下の全文字列(命令文字 8 種)のうち、解析に成功したものを返す。
fn short_valid_programs() -> Vec<String> {
    const ALPHABET: [char; 8] = ['+', '-', '<', '>', '[', ']', '.', ','];
    let mut all = vec![String::new()];
    let mut frontier = vec![String::new()];
    for _ in 0..4 {
        let mut next = Vec::new();
        for prefix in &frontier {
            for &c in &ALPHABET {
                let mut extended = prefix.clone();
                extended.push(c);
                next.push(extended);
            }
        }
        all.extend(next.iter().cloned());
        frontier = next;
    }
    all.into_iter()
        .filter(|source| Program::parse(source).is_ok())
        .collect()
}

/// core を上限付きで完走実行し、実行結果と実行後の機械を返す。
fn run_core(
    source: &str,
    input: &[u8],
    limit: Option<u64>,
) -> (Result<StopReason, RuntimeError>, Machine) {
    let config = match limit {
        Some(max) => MachineConfig::with_max_steps(max),
        None => MachineConfig::unlimited(),
    };
    let mut machine = Machine::new(parse(source), input.to_vec(), config);
    let result = machine.run();
    (result, machine)
}

/// 再生の状態の各項目が core の観測と一致することを確認する。
fn assert_state_matches_core(label: &str, state: &ReplayState, core: &Machine) {
    let snapshot = core.snapshot();
    assert_eq!(state.tape(), core.tape(), "tape: {label}");
    assert_eq!(
        state.data_pointer(),
        snapshot.data_pointer,
        "data pointer: {label}"
    );
    assert_eq!(
        state.steps_executed(),
        snapshot.steps_executed,
        "steps: {label}"
    );
    assert_eq!(
        state.input_position(),
        core.input_position(),
        "input position: {label}"
    );
    assert_eq!(state.output(), core.output(), "output: {label}");
}

/// 初期状態から再生した結果が、同じ上限の core の完走実行と一致することを確認する。
fn assert_fresh_replay_matches_core(source: &str, input: &[u8], limit: Option<u64>) {
    let label = format!("source {source:?}, input {input:?}, limit {limit:?}");
    let (core_result, core) = run_core(source, input, limit);

    let mut state = ReplayState::new(input.to_vec());
    let replayed = replay(&parse(source), limit, 0, &mut state);

    assert_eq!(replayed.result, core_result, "result: {label}");
    assert_eq!(replayed.pc, core.snapshot().pc, "pc: {label}");
    assert_eq!(state.input(), input, "input: {label}");
    assert_state_matches_core(&label, &state, &core);
}

/// 上限 `first` の core 実行で止めた途中状態から、上限 `second`(`first <= second`)で再生した結果が、
/// 上限 `second` の core の完走実行と一致することを確認する。
/// 開始時点でエラーになった場合は、状態が変わらないことも確認する。
fn assert_resume_matches_core(source: &str, input: &[u8], first: u64, second: u64) {
    assert!(first <= second, "first must not exceed second");
    let label = format!("source {source:?}, input {input:?}, first {first}, second {second}");

    let (_, paused) = run_core(source, input, Some(first));
    let paused_snapshot = paused.snapshot();
    let parts = ReplayParts {
        tape: paused.tape().to_vec(),
        dp: paused_snapshot.data_pointer,
        steps_executed: paused_snapshot.steps_executed,
        input: input.to_vec(),
        input_pos: paused.input_position(),
        output: paused.output().to_vec(),
    };
    let mut state = ReplayState::from_parts(parts).expect("core の観測は不変条件を満たす");
    let before = state.clone();
    let replayed = replay(&parse(source), Some(second), paused_snapshot.pc, &mut state);

    let (core_result, core) = run_core(source, input, Some(second));
    assert_eq!(replayed.result, core_result, "result: {label}");
    assert_eq!(replayed.pc, core.snapshot().pc, "pc: {label}");
    assert_state_matches_core(&label, &state, &core);
    if replayed.result.is_err() && state.steps_executed() == before.steps_executed() {
        assert_eq!(
            state, before,
            "state changed on an error at the start: {label}"
        );
    }
}

#[test]
fn new_state_exposes_the_initial_items() {
    let state = ReplayState::new(vec![1, 2]);
    assert_eq!(state.tape().len(), TAPE_SIZE);
    assert!(state.tape().iter().all(|&cell| cell == 0));
    assert_eq!(state.data_pointer(), 0);
    assert_eq!(state.steps_executed(), 0);
    assert_eq!(state.input(), &[1, 2]);
    assert_eq!(state.input_position(), 0);
    assert!(state.output().is_empty());
    assert_eq!(
        state.into_parts(),
        ReplayParts {
            tape: vec![0; TAPE_SIZE],
            dp: 0,
            steps_executed: 0,
            input: vec![1, 2],
            input_pos: 0,
            output: Vec::new(),
        }
    );
}

#[test]
fn from_parts_round_trips_every_item() {
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
fn from_parts_accepts_the_boundary_values() {
    let parts = ReplayParts {
        tape: vec![0; TAPE_SIZE],
        dp: TAPE_SIZE - 1,
        steps_executed: u64::MAX,
        input: vec![7],
        input_pos: 1,
        output: Vec::new(),
    };
    let state = ReplayState::from_parts(parts).expect("boundary values are valid");
    assert_eq!(state.data_pointer(), TAPE_SIZE - 1);
    assert_eq!(state.input_position(), 1);
    assert_eq!(state.steps_executed(), u64::MAX);
}

#[test]
fn from_parts_reports_each_invariant_violation_as_an_error() {
    let valid = || ReplayParts {
        tape: vec![0; TAPE_SIZE],
        dp: 0,
        steps_executed: 0,
        input: vec![1],
        input_pos: 0,
        output: Vec::new(),
    };

    let mut short_tape = valid();
    short_tape.tape.pop();
    assert_eq!(
        ReplayState::from_parts(short_tape),
        Err(ReplayStateError::InvalidTapeLength {
            actual: TAPE_SIZE - 1,
        })
    );

    let mut long_tape = valid();
    long_tape.tape.push(0);
    assert_eq!(
        ReplayState::from_parts(long_tape),
        Err(ReplayStateError::InvalidTapeLength {
            actual: TAPE_SIZE + 1,
        })
    );

    for dp in [TAPE_SIZE, usize::MAX] {
        let mut out_of_range = valid();
        out_of_range.dp = dp;
        assert_eq!(
            ReplayState::from_parts(out_of_range),
            Err(ReplayStateError::DataPointerOutOfRange { dp })
        );
    }

    let mut past_input = valid();
    past_input.input_pos = 2;
    assert_eq!(
        ReplayState::from_parts(past_input),
        Err(ReplayStateError::InputPositionOutOfRange {
            input_pos: 2,
            input_len: 1,
        })
    );

    // 検証順はテープ長、データポインタ、入力読取位置。複数が破られていれば先の検査の結果を返す。
    let mut tape_and_dp = valid();
    tape_and_dp.tape.clear();
    tape_and_dp.dp = TAPE_SIZE;
    assert_eq!(
        ReplayState::from_parts(tape_and_dp),
        Err(ReplayStateError::InvalidTapeLength { actual: 0 })
    );

    let mut dp_and_input = valid();
    dp_and_input.dp = TAPE_SIZE;
    dp_and_input.input_pos = 5;
    assert_eq!(
        ReplayState::from_parts(dp_and_input),
        Err(ReplayStateError::DataPointerOutOfRange { dp: TAPE_SIZE })
    );

    let error = ReplayStateError::InputPositionOutOfRange {
        input_pos: 2,
        input_len: 1,
    };
    assert!(error.to_string().contains('2'));
    assert!(error.to_string().contains('1'));
}

#[test]
fn replayed_items_read_back_and_rebuild_the_same_state() {
    // `,` で 0x41 を読み、`>` `++` `.` で出力、`<` `-` で 0x40 にする。
    let source = ",>++.<-";
    let mut state = ReplayState::new(vec![0x41]);
    let replayed = replay(&parse(source), None, 0, &mut state);
    assert_eq!(replayed.result, Ok(StopReason::Halted));
    assert_eq!(replayed.pc, 7);

    assert_eq!(state.tape()[0], 0x40);
    assert_eq!(state.tape()[1], 2);
    assert_eq!(state.data_pointer(), 0);
    assert_eq!(state.steps_executed(), 7);
    assert_eq!(state.input(), &[0x41]);
    assert_eq!(state.input_position(), 1);
    assert_eq!(state.output(), &[2]);

    let parts = state.clone().into_parts();
    let rebuilt = ReplayState::from_parts(parts.clone()).expect("read back parts are valid");
    assert_eq!(rebuilt, state);
    assert_eq!(rebuilt.into_parts(), parts);

    assert_fresh_replay_matches_core(source, &[0x41], None);
}

#[test]
fn fresh_replay_matches_core_for_every_short_valid_program() {
    let programs = short_valid_programs();
    for source in &programs {
        for &input in &INPUTS {
            let (core_result, core) = run_core(source, input, Some(CAP));
            match core_result {
                Err(RuntimeError::StepLimitExceeded { .. }) => {
                    // 打ち切り上限までに停止しないプログラム: 上限 0 から打ち切り上限 + 2 まで。
                    for limit in 0..=CAP + 2 {
                        assert_fresh_replay_matches_core(source, input, Some(limit));
                    }
                }
                _ => {
                    // 停止またはエラーで終わるプログラム: 上限なしと、上限 0 から必要実行数 + 2 まで。
                    let needed = core.snapshot().steps_executed;
                    assert_fresh_replay_matches_core(source, input, None);
                    for limit in 0..=needed + 2 {
                        assert_fresh_replay_matches_core(source, input, Some(limit));
                    }
                }
            }
        }
    }
    assert!(
        programs.len() > 1000,
        "enumeration covers too few programs: {}",
        programs.len()
    );
}

#[test]
fn non_halting_programs_match_the_core_at_every_limit() {
    // `+[]` は無限ループ、`+[+]` は 256 反復を超えて続き、`+[>+]` は右へ進み続ける。
    for source in ["+[]", "+[+]", "+[>+]"] {
        for &input in &INPUTS {
            for limit in 0..=CAP + 2 {
                assert_fresh_replay_matches_core(source, input, Some(limit));
            }
        }
    }
}

#[test]
fn explicit_programs_match_the_core_with_and_without_a_limit() {
    let sources = [
        "+>+<<",
        "++++++++[>++++++++<-]>+.",
        ",[.,]",
        "+++++[-]",
        "+[>+.<-]",
    ];
    for source in sources {
        for &input in &INPUTS {
            assert_fresh_replay_matches_core(source, input, None);
            for limit in 0..=40 {
                assert_fresh_replay_matches_core(source, input, Some(limit));
            }
        }
    }
}

#[test]
fn resumed_replay_matches_core_for_every_short_valid_program() {
    for source in short_valid_programs() {
        for &input in &INPUTS {
            for first in 0..=6 {
                for second in first..=12 {
                    assert_resume_matches_core(&source, input, first, second);
                }
            }
        }
    }
}

#[test]
fn resumed_replay_matches_core_for_longer_runs() {
    let sources = ["++++++++[>++++++++<-]>+.", "+++[->++<]", "+++++[-]", "+[]"];
    let pairs = [(0, 16), (5, 5), (10, 11), (10, 200), (30, 100), (50, 300)];
    for source in sources {
        for (first, second) in pairs {
            for &input in &INPUTS {
                assert_resume_matches_core(source, input, first, second);
            }
        }
    }
}

#[test]
fn start_at_or_past_the_end_halts_without_changing_state() {
    let program = parse("+-<>");
    let parts = ReplayParts {
        tape: vec![3; TAPE_SIZE],
        dp: 5,
        steps_executed: 9,
        input: vec![1],
        input_pos: 0,
        output: vec![8],
    };
    for start in [program.len(), program.len() + 1, usize::MAX] {
        for limit in [None, Some(0), Some(1)] {
            let mut state = ReplayState::from_parts(parts.clone()).expect("valid parts");
            let before = state.clone();
            let replayed = replay(&program, limit, start, &mut state);
            assert_eq!(replayed.pc, program.len(), "start {start}, limit {limit:?}");
            assert_eq!(replayed.result, Ok(StopReason::Halted));
            assert_eq!(state, before, "start {start}, limit {limit:?}");
        }
    }

    let empty = parse("");
    let mut state = ReplayState::new(Vec::new());
    let before = state.clone();
    let replayed = replay(&empty, Some(0), 0, &mut state);
    assert_eq!(replayed.pc, 0);
    assert_eq!(replayed.result, Ok(StopReason::Halted));
    assert_eq!(state, before);
}

#[test]
fn errors_at_the_starting_point_leave_the_state_unchanged() {
    // 上限超過: 開始時点で実行数が上限に達している。
    let parts = ReplayParts {
        tape: vec![0; TAPE_SIZE],
        dp: 0,
        steps_executed: 5,
        input: Vec::new(),
        input_pos: 0,
        output: vec![1],
    };
    let mut state = ReplayState::from_parts(parts).expect("valid parts");
    let before = state.clone();
    let replayed = replay(&parse("+"), Some(5), 0, &mut state);
    assert_eq!(replayed.pc, 0);
    assert_eq!(
        replayed.result,
        Err(RuntimeError::StepLimitExceeded { limit: 5, pc: 0 })
    );
    assert_eq!(state, before);

    // 範囲外: 右端のセルで `>` を実行する。
    let parts = ReplayParts {
        tape: vec![0; TAPE_SIZE],
        dp: TAPE_SIZE - 1,
        steps_executed: 0,
        input: Vec::new(),
        input_pos: 0,
        output: Vec::new(),
    };
    let mut state = ReplayState::from_parts(parts).expect("valid parts");
    let before = state.clone();
    let replayed = replay(&parse(">"), None, 0, &mut state);
    assert_eq!(replayed.pc, 0);
    assert_eq!(
        replayed.result,
        Err(RuntimeError::PointerOutOfBounds {
            pc: 0,
            dp: TAPE_SIZE - 1,
            direction: PointerDirection::Right,
        })
    );
    assert_eq!(state, before);
}
