use brainfuck_compare::limits::{
    DEFAULT_REPETITIONS, MAX_COMPARISON_STEPS, MAX_REPETITIONS, REPEAT_TIME_BUDGET,
};
use brainfuck_compare::{Field, Observation};
use brainfuck_core::{PointerDirection, RuntimeError, StopReason, TAPE_SIZE};

fn base() -> Observation {
    Observation {
        result: Ok(StopReason::Halted),
        pc: 3,
        data_pointer: 1,
        current_cell: 7,
        steps_executed: 42,
        input_position: 2,
        tape: vec![0u8; TAPE_SIZE].into_boxed_slice(),
        output: vec![b'A'],
    }
}

fn err(pc: usize) -> Result<StopReason, RuntimeError> {
    Err(RuntimeError::PointerOutOfBounds {
        pc,
        dp: 0,
        direction: PointerDirection::Left,
    })
}

#[test]
fn identical_observations_have_no_diff() {
    assert!(base().diff(&base()).is_empty());
    assert!(base().halted());
}

#[test]
fn changing_one_field_reports_only_that_field() {
    let a = base();
    let cases: Vec<(Field, Observation)> = vec![
        (
            Field::Termination,
            Observation {
                result: Err(RuntimeError::StepLimitExceeded { limit: 1, pc: 0 }),
                ..base()
            },
        ),
        (Field::Pc, Observation { pc: 4, ..base() }),
        (
            Field::DataPointer,
            Observation {
                data_pointer: 2,
                ..base()
            },
        ),
        (
            Field::CurrentCell,
            Observation {
                current_cell: 8,
                ..base()
            },
        ),
        (
            Field::StepsExecuted,
            Observation {
                steps_executed: 43,
                ..base()
            },
        ),
        (
            Field::InputPosition,
            Observation {
                input_position: 3,
                ..base()
            },
        ),
        (
            Field::Output,
            Observation {
                output: vec![b'B'],
                ..base()
            },
        ),
        (Field::Tape, {
            let mut tape = vec![0u8; TAPE_SIZE].into_boxed_slice();
            tape[100] = 1;
            Observation { tape, ..base() }
        }),
    ];
    for (field, b) in cases {
        assert_eq!(
            a.diff(&b),
            vec![field],
            "項目 {field:?} だけが差分になるべき"
        );
    }
}

#[test]
fn only_error_value_difference_reports_error_value() {
    let a = Observation {
        result: err(1),
        ..base()
    };
    let b = Observation {
        result: err(2),
        ..base()
    };
    assert_eq!(a.diff(&b), vec![Field::ErrorValue]);
    assert!(!a.halted());
}

#[test]
fn diff_is_independent_per_field_when_several_change() {
    let a = base();
    let b = Observation {
        pc: 9,
        output: vec![],
        ..base()
    };
    assert_eq!(a.diff(&b), vec![Field::Pc, Field::Output]);
}

#[test]
fn tape_digest_is_deterministic_fnv1a_64() {
    // 空のテープ 30,000 バイトの指紋は同一観測で常に同じ値。
    assert_eq!(base().tape_digest(), base().tape_digest());
    // FNV-1a 64 の既知値: 空入力のオフセット基底。
    let mut tape_a = base();
    tape_a.tape = Vec::new().into_boxed_slice();
    assert_eq!(tape_a.tape_digest(), 0xcbf2_9ce4_8422_2325);
    // 1 バイト変えると指紋が変わる。
    let mut tape_b = base();
    tape_b.tape[0] = 1;
    assert_ne!(base().tape_digest(), tape_b.tape_digest());
}

#[test]
fn comparison_limits_match_design() {
    assert_eq!(MAX_COMPARISON_STEPS, 50_000_000);
    assert_eq!(MAX_REPETITIONS, 5);
    assert_eq!(DEFAULT_REPETITIONS, 3);
    assert_eq!(REPEAT_TIME_BUDGET.as_secs(), 2);
    assert!(MAX_COMPARISON_STEPS <= brainfuck_wasm::limits::MAX_OUTPUT_BYTES as u64);
}

fn with_result(result: Result<StopReason, RuntimeError>) -> Observation {
    Observation { result, ..base() }
}

fn pointer_error(pc: usize) -> Result<StopReason, RuntimeError> {
    Err(RuntimeError::PointerOutOfBounds {
        pc,
        dp: 0,
        direction: PointerDirection::Right,
    })
}

fn step_limit_error(pc: usize) -> Result<StopReason, RuntimeError> {
    Err(RuntimeError::StepLimitExceeded { limit: 10, pc })
}

#[test]
fn different_error_kinds_are_termination() {
    let a = with_result(err(1));
    let b = with_result(step_limit_error(1));
    assert_eq!(a.diff(&b), vec![Field::Termination]);
    assert_eq!(b.diff(&a), vec![Field::Termination]);
}

#[test]
fn same_error_kind_with_different_value_is_error_value() {
    let a = with_result(pointer_error(1));
    let b = with_result(pointer_error(2));
    assert_eq!(a.diff(&b), vec![Field::ErrorValue]);
    let c = with_result(step_limit_error(1));
    let d = with_result(step_limit_error(2));
    assert_eq!(c.diff(&d), vec![Field::ErrorValue]);
}

#[test]
fn different_stop_reasons_on_success_are_termination() {
    let a = with_result(Ok(StopReason::Halted));
    let b = with_result(Ok(StopReason::StepLimitReached));
    assert_eq!(a.diff(&b), vec![Field::Termination]);
}

#[test]
fn success_versus_error_is_termination() {
    let ok = with_result(Ok(StopReason::Halted));
    let e = with_result(step_limit_error(1));
    assert_eq!(ok.diff(&e), vec![Field::Termination]);
    assert_eq!(e.diff(&ok), vec![Field::Termination]);
}

#[test]
fn fnv1a_64_known_vectors() {
    let digest = |bytes: &[u8]| {
        let mut o = base();
        o.tape = bytes.to_vec().into_boxed_slice();
        o.tape_digest()
    };
    assert_eq!(digest(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(digest(b"foobar"), 0x8594_4171_f739_67e8);
}
