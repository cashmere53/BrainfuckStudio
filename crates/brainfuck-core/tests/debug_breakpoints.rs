//! ブレークポイント集合(`BreakpointSet`)の単体テスト。
//!
//! 命令添字のみを扱い、範囲外の追加・トグルはエラーで集合を変更しない。
//! 問い合わせと削除は範囲外でも異常終了せず偽を返す。

use brainfuck_core::{BreakpointError, BreakpointSet};
use std::error::Error;

/// 集合の内容を昇順の添字列として取り出す(検証用の局所ヘルパ)。
fn indices(set: &BreakpointSet) -> Vec<usize> {
    set.iter().collect()
}

#[test]
fn new_set_is_empty_and_contains_nothing() {
    let set = BreakpointSet::new(5);
    assert_eq!(set.instruction_count(), 5);
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    assert!(indices(&set).is_empty());
    for index in 0..5 {
        assert!(!set.contains(index));
    }
}

#[test]
fn insert_new_index_reports_true_and_adds_it() {
    let mut set = BreakpointSet::new(5);
    assert_eq!(set.insert(2), Ok(true));
    assert!(set.contains(2));
    assert_eq!(set.len(), 1);
    assert!(!set.is_empty());
}

#[test]
fn insert_existing_index_reports_false_and_keeps_count() {
    let mut set = BreakpointSet::new(5);
    assert_eq!(set.insert(3), Ok(true));
    assert_eq!(set.insert(3), Ok(false));
    assert!(set.contains(3));
    assert_eq!(set.len(), 1);
    assert_eq!(indices(&set), vec![3]);
}

#[test]
fn insert_out_of_range_is_error_and_leaves_set_unchanged() {
    let mut set = BreakpointSet::new(4);
    assert_eq!(set.insert(1), Ok(true));
    let before = set.clone();

    assert_eq!(
        set.insert(4),
        Err(BreakpointError::OutOfRange {
            instruction_index: 4,
            instruction_count: 4,
        })
    );
    assert_eq!(
        set.insert(100),
        Err(BreakpointError::OutOfRange {
            instruction_index: 100,
            instruction_count: 4,
        })
    );
    assert_eq!(set, before);
    assert_eq!(set.len(), 1);
    assert_eq!(indices(&set), vec![1]);
    assert!(!set.contains(4));
}

#[test]
fn out_of_range_error_message_contains_only_numbers_and_fixed_text() {
    let error = BreakpointError::OutOfRange {
        instruction_index: 7,
        instruction_count: 3,
    };
    let message = error.to_string();
    assert!(message.contains('7'));
    assert!(message.contains('3'));
    // `std::error::Error` を実装していること。
    let _: &dyn Error = &error;
}

#[test]
fn remove_existing_reports_true_and_missing_reports_false() {
    let mut set = BreakpointSet::new(5);
    set.insert(0).unwrap();
    set.insert(4).unwrap();

    assert!(set.remove(4));
    assert!(!set.contains(4));
    assert_eq!(set.len(), 1);

    assert!(!set.remove(4));
    assert!(!set.remove(2));
    assert_eq!(indices(&set), vec![0]);
}

#[test]
fn remove_out_of_range_returns_false_without_panic() {
    let mut set = BreakpointSet::new(2);
    set.insert(1).unwrap();
    let before = set.clone();

    assert!(!set.remove(2));
    assert!(!set.remove(usize::MAX));
    assert_eq!(set, before);
}

#[test]
fn toggle_round_trip_restores_original_state() {
    let mut set = BreakpointSet::new(3);

    assert_eq!(set.toggle(1), Ok(true));
    assert!(set.contains(1));
    assert_eq!(set.len(), 1);

    assert_eq!(set.toggle(1), Ok(false));
    assert!(!set.contains(1));
    assert_eq!(set.len(), 0);
    assert!(set.is_empty());
}

#[test]
fn toggle_out_of_range_is_error_and_leaves_set_unchanged() {
    let mut set = BreakpointSet::new(3);
    set.insert(0).unwrap();
    let before = set.clone();

    assert_eq!(
        set.toggle(3),
        Err(BreakpointError::OutOfRange {
            instruction_index: 3,
            instruction_count: 3,
        })
    );
    assert_eq!(set, before);
    assert_eq!(indices(&set), vec![0]);
}

#[test]
fn clear_removes_all_indices() {
    let mut set = BreakpointSet::new(10);
    for index in [0, 3, 9] {
        set.insert(index).unwrap();
    }
    assert_eq!(set.len(), 3);

    set.clear();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    assert!(indices(&set).is_empty());
    assert!(!set.contains(0));
    assert!(!set.contains(3));
    assert!(!set.contains(9));
}

#[test]
fn iter_is_ascending_and_without_duplicates() {
    let mut set = BreakpointSet::new(10);
    for index in [5, 1, 3, 1, 0, 9, 5] {
        let _ = set.insert(index);
    }
    assert_eq!(indices(&set), vec![0, 1, 3, 5, 9]);
    assert_eq!(set.len(), 5);
}

#[test]
fn zero_instruction_set_rejects_every_index() {
    let mut set = BreakpointSet::new(0);
    assert_eq!(set.instruction_count(), 0);
    assert!(set.is_empty());
    assert!(indices(&set).is_empty());

    assert_eq!(
        set.insert(0),
        Err(BreakpointError::OutOfRange {
            instruction_index: 0,
            instruction_count: 0,
        })
    );
    assert_eq!(
        set.toggle(0),
        Err(BreakpointError::OutOfRange {
            instruction_index: 0,
            instruction_count: 0,
        })
    );
    assert!(!set.contains(0));
    assert!(!set.remove(0));
    assert!(set.is_empty());
}

#[test]
fn contains_and_remove_out_of_range_do_not_panic() {
    let set = BreakpointSet::new(2);
    assert!(!set.contains(2));
    assert!(!set.contains(usize::MAX));
}

#[test]
fn set_is_comparable_and_cloneable_for_value_semantics() {
    let mut left = BreakpointSet::new(4);
    left.insert(2).unwrap();
    let right = left.clone();
    assert_eq!(left, right);

    left.insert(0).unwrap();
    assert_ne!(left, right);
    assert_eq!(indices(&right), vec![2]);
}
