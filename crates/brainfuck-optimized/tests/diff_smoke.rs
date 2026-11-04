//! 差分比較の共通基盤(`tests/common`)の動作確認。
//!
//! 手書きの小さなプログラムで、加算のみ・クリア・乗算・走査・範囲外エラー・ステップ制限超過の
//! 比較が一致することを確認する。加えて、基盤が食い違いを検出し、不一致の項目名を報告することを確認する。

mod common;

use brainfuck_core::MachineConfig;
use common::{
    assert_equivalent, assert_equivalent_across_limits, compare_observations, observe_core,
};

#[test]
fn addition_only_matches_the_core_machine() {
    assert_equivalent("+++++", &[], MachineConfig::with_max_steps(100));
    assert_equivalent_across_limits("+++++", &[], None);
}

#[test]
fn clear_loop_matches_the_core_machine() {
    assert_equivalent("+++++[-]", &[], MachineConfig::unlimited());
    assert_equivalent_across_limits("+++++[-]", &[], None);
}

#[test]
fn multiplication_loop_matches_the_core_machine() {
    assert_equivalent("++++++++[>++++++++<-]>+.", &[], MachineConfig::unlimited());
    assert_equivalent_across_limits("+++[->++<]", &[], None);
}

#[test]
fn scan_loop_matches_the_core_machine() {
    assert_equivalent("+>+>+<<[>]", &[], MachineConfig::unlimited());
    assert_equivalent_across_limits("+>+>+<<[>]", &[], None);
}

#[test]
fn out_of_bounds_error_matches_the_core_machine() {
    assert_equivalent("<", &[], MachineConfig::unlimited());
    assert_equivalent("+++<", &[], MachineConfig::unlimited());
    assert_equivalent_across_limits("+++<", &[], None);
}

#[test]
fn step_limit_exceeded_matches_the_core_machine() {
    assert_equivalent("+[]", &[], MachineConfig::with_max_steps(5));
    assert_equivalent_across_limits("+[]", &[], Some(64));
}

#[test]
fn input_and_output_match_the_core_machine() {
    assert_equivalent(",[.,]", &[0x00, 0xFF, 7], MachineConfig::unlimited());
    assert_equivalent_across_limits(",[.,]", &[0x00, 0xFF, 7], None);
}

#[test]
fn identical_observations_have_no_mismatch() {
    let base = observe_core("+.", &[], MachineConfig::unlimited());
    let same = base.clone();
    assert_eq!(compare_observations(&base, &same), "");
}

#[test]
fn mismatched_items_are_named_in_the_report() {
    let base = observe_core("+.", &[], MachineConfig::unlimited());
    let mut other = base.clone();
    other.pc += 1;
    other.tape[0] = 9;

    let diff = compare_observations(&base, &other);
    assert!(!diff.is_empty(), "a mismatch must be detected");
    assert!(diff.contains("pc"), "report should name pc: {diff}");
    assert!(diff.contains("tape"), "report should name tape: {diff}");
    assert!(
        !diff.contains("output"),
        "report should not name equal items: {diff}"
    );
}

#[test]
fn bracket_balanced_enumeration_yields_only_balanced_programs() {
    let mut balanced = common::enumerate_programs(b"[]", 4, true);
    balanced.sort();
    assert_eq!(balanced, vec!["", "[[]]", "[]", "[][]"]);

    let mut all = common::enumerate_programs(b"+", 2, false);
    all.sort();
    assert_eq!(all, vec!["", "+", "++"]);
}

#[test]
fn xorshift_is_reproducible_from_a_fixed_seed_and_never_sticks_at_zero() {
    let mut first = common::XorShift64::new(42);
    let mut second = common::XorShift64::new(42);
    let a: Vec<u64> = (0..8).map(|_| first.next_u64()).collect();
    let b: Vec<u64> = (0..8).map(|_| second.next_u64()).collect();
    assert_eq!(a, b);

    let mut zero_seed = common::XorShift64::new(0);
    assert_ne!(zero_seed.next_u64(), 0);
    assert!((0..100).all(|_| zero_seed.below(7) < 7));
}

#[test]
#[should_panic(expected = "steps executed: core=")]
fn mismatched_step_count_makes_the_assertion_panic() {
    let core = observe_core("+.", &[], MachineConfig::unlimited());
    let mut optimized = core.clone();
    optimized.steps_executed += 1;
    common::assert_observations_equivalent("label", &core, &optimized, 1);
}

#[test]
#[should_panic(expected = "exceed naive steps")]
fn ir_operations_above_naive_steps_make_the_assertion_panic() {
    let core = observe_core("+.", &[], MachineConfig::unlimited());
    let ir_ops = core.steps_executed + 1;
    common::assert_observations_equivalent("label", &core, &core.clone(), ir_ops);
}
