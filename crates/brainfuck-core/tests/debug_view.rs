//! テープ表示窓(`tape_window_of` / `tape_window`)の単体テスト。
//!
//! 機械を持たないスライス版と、機械経由の版が同じ規則で同じ結果を返すことを確認する。
//! 窓は `dp - 半径` から `dp + 半径` を有効範囲に切り詰めた借用であり、状態を変えない。

mod debug_common;

use brainfuck_core::{
    tape_window, tape_window_of, MachineConfig, DEFAULT_WINDOW_RADIUS, TAPE_SIZE,
};
use debug_common::{machine_from, session_from};

/// 指定回数の `>` を含むソースを作り、完走させた機械を返す(`dp` を任意の位置へ動かす検証用)。
fn machine_with_pointer_at(dp: usize) -> brainfuck_core::Machine {
    let source = ">".repeat(dp);
    let mut machine = machine_from(&source, &[], MachineConfig::unlimited());
    machine.run().expect("`>` だけのプログラムは完走する");
    assert_eq!(machine.snapshot().data_pointer, dp);
    machine
}

#[test]
fn default_radius_is_sixteen() {
    assert_eq!(DEFAULT_WINDOW_RADIUS, 16);
}

#[test]
fn initial_state_window_starts_at_zero_with_seventeen_cells() {
    let machine = machine_from("+", &[], MachineConfig::unlimited());
    let window = tape_window(&machine, DEFAULT_WINDOW_RADIUS);
    assert_eq!(window.start, 0);
    assert_eq!(window.cells.len(), 17);
    assert_eq!(window.pointer_offset, 0);
}

#[test]
fn middle_window_has_pointer_at_the_center() {
    let machine = machine_with_pointer_at(100);
    let window = tape_window(&machine, DEFAULT_WINDOW_RADIUS);
    assert_eq!(window.start, 84);
    assert_eq!(window.cells.len(), 33);
    assert_eq!(window.pointer_offset, 16);
    assert_eq!(window.start + window.pointer_offset, 100);
}

#[test]
fn right_edge_window_is_clipped_to_the_last_cell() {
    let machine = machine_with_pointer_at(TAPE_SIZE - 1);
    let window = tape_window(&machine, DEFAULT_WINDOW_RADIUS);
    assert_eq!(window.start, TAPE_SIZE - 1 - 16);
    assert_eq!(window.cells.len(), 17);
    assert_eq!(window.pointer_offset, 16);
    assert_eq!(window.start + window.cells.len(), TAPE_SIZE);
}

#[test]
fn left_edge_window_is_clipped_to_the_first_cell() {
    let machine = machine_with_pointer_at(3);
    let window = tape_window(&machine, DEFAULT_WINDOW_RADIUS);
    assert_eq!(window.start, 0);
    assert_eq!(window.cells.len(), 20);
    assert_eq!(window.pointer_offset, 3);
}

#[test]
fn zero_radius_yields_only_the_current_cell() {
    let machine = machine_with_pointer_at(500);
    let window = tape_window(&machine, 0);
    assert_eq!(window.start, 500);
    assert_eq!(window.cells.len(), 1);
    assert_eq!(window.pointer_offset, 0);
}

#[test]
fn radius_covering_the_whole_tape_yields_all_cells() {
    let machine = machine_with_pointer_at(15_000);
    for radius in [TAPE_SIZE, TAPE_SIZE + 1, 1 << 40, usize::MAX] {
        let window = tape_window(&machine, radius);
        assert_eq!(window.start, 0, "radius {radius}");
        assert_eq!(window.cells.len(), TAPE_SIZE, "radius {radius}");
        assert_eq!(window.pointer_offset, 15_000, "radius {radius}");
    }
}

#[test]
fn extreme_radius_at_the_edges_does_not_panic() {
    let at_left = machine_from("+", &[], MachineConfig::unlimited());
    let window = tape_window(&at_left, usize::MAX);
    assert_eq!(window.start, 0);
    assert_eq!(window.cells.len(), TAPE_SIZE);
    assert_eq!(window.pointer_offset, 0);

    let at_right = machine_with_pointer_at(TAPE_SIZE - 1);
    let window = tape_window(&at_right, usize::MAX);
    assert_eq!(window.start, 0);
    assert_eq!(window.cells.len(), TAPE_SIZE);
    assert_eq!(window.pointer_offset, TAPE_SIZE - 1);
}

#[test]
fn window_cells_match_the_same_indices_of_the_whole_tape() {
    // 位置ごとに異なる値を書き込み、窓のセル値がテープ全体の同じ添字と一致することを確認する。
    for dp in [3usize, 10, 29_990, TAPE_SIZE - 1] {
        // 先頭に値を書いた後、`dp` まで移動する。窓に非ゼロの値を含ませる。
        let source = format!("+>++>+++>{}", ">".repeat(dp - 3));
        let mut machine = machine_from(&source, &[], MachineConfig::unlimited());
        machine.run().expect("完走する");
        assert_eq!(machine.snapshot().data_pointer, dp);
        let tape = machine.tape();
        let window = tape_window(&machine, DEFAULT_WINDOW_RADIUS);
        for (offset, cell) in window.cells.iter().enumerate() {
            assert_eq!(
                *cell,
                tape[window.start + offset],
                "dp {dp} offset {offset}"
            );
        }
        assert_eq!(window.start + window.pointer_offset, dp, "dp {dp}");
    }
}

#[test]
fn window_cells_reflect_written_values() {
    // `+` を 7 回 → セル 0 は 7。`>` 1 回後に `+` を 3 回 → セル 1 は 3。半径 1 なので右隣のセル 2 は 0。
    let mut machine = machine_from("+++++++>+++", &[], MachineConfig::unlimited());
    machine.run().expect("完走する");
    let window = tape_window(&machine, 1);
    assert_eq!(window.start, 0);
    assert_eq!(window.cells, &[7, 3, 0][..]);
    assert_eq!(window.pointer_offset, 1);
}

#[test]
fn machine_window_equals_slice_window_for_the_same_input() {
    let machine = machine_with_pointer_at(12_345);
    for radius in [0usize, 1, 16, 100, TAPE_SIZE, usize::MAX] {
        let via_machine = tape_window(&machine, radius);
        let via_slice = tape_window_of(machine.tape(), machine.snapshot().data_pointer, radius);
        assert_eq!(via_machine, via_slice, "radius {radius}");
    }
}

#[test]
fn slice_window_clips_both_ends_on_a_short_tape() {
    let tape: Vec<u8> = (0..10).collect();

    let left = tape_window_of(&tape, 0, 2);
    assert_eq!(left.start, 0);
    assert_eq!(left.cells, &[0, 1, 2][..]);
    assert_eq!(left.pointer_offset, 0);

    let right = tape_window_of(&tape, 9, 2);
    assert_eq!(right.start, 7);
    assert_eq!(right.cells, &[7, 8, 9][..]);
    assert_eq!(right.pointer_offset, 2);

    let whole = tape_window_of(&tape, 5, 100);
    assert_eq!(whole.start, 0);
    assert_eq!(whole.cells, &tape[..]);
    assert_eq!(whole.pointer_offset, 5);
}

#[test]
fn slice_window_does_not_pad_or_shift_near_the_end() {
    let tape: Vec<u8> = (0..10).collect();
    // 半径が大きくても、窓は有効範囲に収まる(パディングもずらしもしない)。
    let window = tape_window_of(&tape, 1, 4);
    assert_eq!(window.start, 0);
    assert_eq!(window.cells.len(), 6);
    assert_eq!(window.cells, &tape[0..6]);
}

#[test]
fn slice_window_with_pointer_outside_tape_is_empty_and_does_not_panic() {
    let tape: Vec<u8> = (0..10).collect();
    for dp in [10usize, 11, 1_000, usize::MAX] {
        let window = tape_window_of(&tape, dp, 16);
        assert_eq!(window.start, tape.len(), "dp {dp}");
        assert!(window.cells.is_empty(), "dp {dp}");
        assert_eq!(window.pointer_offset, 0, "dp {dp}");
    }
}

#[test]
fn slice_window_on_an_empty_tape_is_empty_and_does_not_panic() {
    let tape: Vec<u8> = Vec::new();
    for radius in [0usize, 16, usize::MAX] {
        let window = tape_window_of(&tape, 0, radius);
        assert_eq!(window.start, 0);
        assert!(window.cells.is_empty());
        assert_eq!(window.pointer_offset, 0);
    }
}

#[test]
fn slice_window_borrows_without_changing_the_tape() {
    let tape: Vec<u8> = (0..10).collect();
    let before = tape.clone();
    let _ = tape_window_of(&tape, 4, 3);
    assert_eq!(tape, before);
}

#[test]
fn session_window_at_creation_matches_the_machine_window() {
    let session = session_from("+", &[], MachineConfig::unlimited());
    let window = session.tape_window(DEFAULT_WINDOW_RADIUS);
    assert_eq!(window.start, 0);
    assert_eq!(window.cells.len(), 17);
    assert_eq!(window.pointer_offset, 0);
    assert_eq!(
        window,
        tape_window(session.machine(), DEFAULT_WINDOW_RADIUS)
    );
}

#[test]
fn session_window_delegates_to_the_slice_function_for_every_radius() {
    let session = session_from("+", &[], MachineConfig::unlimited());
    let machine = session.machine();
    for radius in [0usize, 1, DEFAULT_WINDOW_RADIUS, TAPE_SIZE, usize::MAX] {
        let via_session = session.tape_window(radius);
        let via_slice = tape_window_of(machine.tape(), machine.snapshot().data_pointer, radius);
        assert_eq!(via_session, via_slice, "radius {radius}");
    }
}

#[test]
fn state_view_fields_agree_with_the_session_accessors() {
    let session = session_from("+>+", b"", MachineConfig::unlimited());
    let view = session.state_view(DEFAULT_WINDOW_RADIUS);
    assert_eq!(view.snapshot, session.snapshot());
    assert_eq!(view.tape_window, session.tape_window(DEFAULT_WINDOW_RADIUS));
    assert_eq!(view.output, session.machine().output());
    assert_eq!(view.last_stop, session.last_stop());
    assert_eq!(view.last_stop, None);
}

#[test]
fn state_view_window_start_and_offset_sum_to_the_pointer() {
    let session = session_from("+", &[], MachineConfig::unlimited());
    let view = session.state_view(4);
    assert_eq!(
        view.tape_window.start + view.tape_window.pointer_offset,
        view.snapshot.data_pointer
    );
    assert_eq!(view.output, &[][..]);
}
