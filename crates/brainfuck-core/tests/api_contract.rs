//! 公開 API の契約テスト: 停止理由 `StopReason` と実行結果の現状の振る舞いを固定する。
//!
//! `StopReason` は 4 バリアントを持つが、`Machine::run` が `Ok` で返すのは `Halted` のみである。
//! `StepLimitReached`・`Breakpoint`・`Cancelled` は将来用の予約であり、現在はどの経路からも返らない。
//! 総実行数の上限到達は `Err(RuntimeError::StepLimitExceeded)` で表す。
//! 本ファイルは、この現状の振る舞いを具体値で固定する。

use brainfuck_core::{
    Machine, MachineConfig, PointerDirection, Program, RuntimeError, StepResult, StopReason,
};

/// 現時点で定義されている停止理由の全バリアント。
const ALL_STOP_REASONS: [StopReason; 4] = [
    StopReason::Halted,
    StopReason::StepLimitReached,
    StopReason::Breakpoint,
    StopReason::Cancelled,
];

/// 予約バリアントの一覧。`run` の `Ok` で返ってはならない値である。
const RESERVED_STOP_REASONS: [StopReason; 3] = [
    StopReason::StepLimitReached,
    StopReason::Breakpoint,
    StopReason::Cancelled,
];

/// 停止理由の説明文を返す。
///
/// `StopReason` は `#[non_exhaustive]` のため、外部 crate からの `match` では `_ =>` が必須である。
/// 将来バリアントが追加されると `_` の腕に落ち、下のテストで未知の値として検出される。
fn describe(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Halted => "正常終了",
        StopReason::StepLimitReached => "バッチ上限到達(予約)",
        StopReason::Breakpoint => "ブレークポイント(予約)",
        StopReason::Cancelled => "キャンセル(予約)",
        _ => "未知",
    }
}

/// `Copy` かつ `Clone`、`Eq`、`Debug` を満たすことをコンパイル時に要求する。
fn requires_copy_eq_debug<T: Copy + Eq + std::fmt::Debug>(value: T) -> T {
    value
}

/// `Clone` を満たすことをコンパイル時に要求する。
fn requires_clone<T: Clone>(value: &T) -> T {
    value.clone()
}

#[test]
fn stop_reason_debug_output_names_each_variant() {
    assert_eq!(format!("{:?}", StopReason::Halted), "Halted");
    assert_eq!(
        format!("{:?}", StopReason::StepLimitReached),
        "StepLimitReached"
    );
    assert_eq!(format!("{:?}", StopReason::Breakpoint), "Breakpoint");
    assert_eq!(format!("{:?}", StopReason::Cancelled), "Cancelled");
}

#[test]
fn stop_reason_variants_are_pairwise_distinct() {
    for (i, a) in ALL_STOP_REASONS.iter().enumerate() {
        for (j, b) in ALL_STOP_REASONS.iter().enumerate() {
            if i == j {
                assert_eq!(a, b, "同一バリアントは等しい");
            } else {
                assert_ne!(a, b, "異なるバリアントは等しくない");
            }
        }
    }
}

#[test]
fn stop_reason_is_copy_clone_and_eq() {
    let reason = requires_copy_eq_debug(StopReason::Breakpoint);
    // `Copy` のため、渡した後も元の値を使える。
    let copied = reason;
    assert_eq!(reason, copied);
    assert_eq!(
        requires_clone(&StopReason::Cancelled),
        StopReason::Cancelled
    );
}

#[test]
fn describe_maps_every_variant_and_never_falls_back_to_unknown() {
    assert_eq!(describe(StopReason::Halted), "正常終了");
    assert_eq!(
        describe(StopReason::StepLimitReached),
        "バッチ上限到達(予約)"
    );
    assert_eq!(describe(StopReason::Breakpoint), "ブレークポイント(予約)");
    assert_eq!(describe(StopReason::Cancelled), "キャンセル(予約)");
    for reason in ALL_STOP_REASONS {
        assert_ne!(
            describe(reason),
            "未知",
            "{reason:?} は未知として扱われない"
        );
    }
}

#[test]
fn run_on_terminating_program_returns_halted_and_keeps_output() {
    // `+` を 8 回、`[>++++++++<-]` で 64 を作り、`>+.` で `A` を出力する。
    let program = Program::parse("++++++++[>++++++++<-]>+.").expect("valid source");
    let mut machine = Machine::new(program, Vec::new(), MachineConfig::with_max_steps(1_000));

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), &b"A"[..]);
    assert!(machine.snapshot().halted);
}

#[test]
fn run_on_halted_machine_returns_halted_again_without_changing_state() {
    let program = Program::parse("+.").expect("valid source");
    let mut machine = Machine::new(program, Vec::new(), MachineConfig::unlimited());

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    let before = machine.snapshot();

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(
        machine.snapshot(),
        before,
        "停止済みの再実行は状態を変えない"
    );
}

#[test]
fn run_reaching_step_limit_returns_runtime_error_not_a_stop_reason() {
    // `+[]` は無限ループ。上限 3 では、3 命令目の `]` を実行した直後の pc 2 で上限超過となる。
    let program = Program::parse("+[]").expect("valid source");
    let mut machine = Machine::new(program, Vec::new(), MachineConfig::with_max_steps(3));

    assert_eq!(
        machine.run(),
        Err(RuntimeError::StepLimitExceeded { limit: 3, pc: 2 })
    );

    let snapshot = machine.snapshot();
    assert_eq!(
        snapshot.steps_executed, 3,
        "上限まで実行し、超過分は実行しない"
    );
    assert_eq!(snapshot.pc, 2);
    assert!(!snapshot.halted, "上限超過時は未停止のまま");
}

#[test]
fn step_after_step_limit_returns_same_error_without_executing() {
    let program = Program::parse("+[]").expect("valid source");
    let mut machine = Machine::new(program, Vec::new(), MachineConfig::with_max_steps(3));
    assert!(machine.run().is_err());
    let before = machine.snapshot();

    assert_eq!(
        machine.step(),
        Err(RuntimeError::StepLimitExceeded { limit: 3, pc: 2 })
    );
    assert_eq!(
        machine.snapshot(),
        before,
        "上限超過の step は状態を変えない"
    );
}

#[test]
fn step_on_finite_program_yields_executed_then_halted() {
    // `++.` は命令 3 個。3 回は Executed、4 回目で Halted になる。
    let program = Program::parse("++.").expect("valid source");
    let mut machine = Machine::new(program, Vec::new(), MachineConfig::unlimited());

    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Executed));
    assert_eq!(machine.step(), Ok(StepResult::Halted));
    assert_eq!(
        machine.step(),
        Ok(StepResult::Halted),
        "停止後も Halted が続く"
    );
    assert_eq!(machine.output(), &[2u8][..]);
}

#[test]
fn pointer_out_of_bounds_returns_runtime_error_not_a_stop_reason() {
    // 先頭で `<` を実行すると dp が 0 のまま左へ動けず、状態を変えずに失敗する。
    let program = Program::parse("<").expect("valid source");
    let mut machine = Machine::new(program, Vec::new(), MachineConfig::unlimited());

    assert_eq!(
        machine.run(),
        Err(RuntimeError::PointerOutOfBounds {
            pc: 0,
            dp: 0,
            direction: PointerDirection::Left,
        })
    );
    let snapshot = machine.snapshot();
    assert_eq!(snapshot.pc, 0);
    assert_eq!(snapshot.steps_executed, 0);
}

#[test]
fn run_never_returns_reserved_stop_reasons_for_representative_programs() {
    let cases: [(&str, MachineConfig); 4] = [
        ("+.", MachineConfig::unlimited()),
        (
            "++++++++[>++++++++<-]>+.",
            MachineConfig::with_max_steps(1_000),
        ),
        ("+[]", MachineConfig::with_max_steps(3)),
        ("<", MachineConfig::unlimited()),
    ];
    for (source, config) in cases {
        let program = Program::parse(source).expect("valid source");
        let mut machine = Machine::new(program, Vec::new(), config);
        let result = machine.run();
        for reserved in RESERVED_STOP_REASONS {
            assert_ne!(
                result,
                Ok(reserved),
                "{source:?} が予約理由 {reserved:?} を返してはならない"
            );
        }
    }
}
