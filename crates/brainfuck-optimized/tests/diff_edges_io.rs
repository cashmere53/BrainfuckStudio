//! ポインタ端と入出力の境界比較(`tests/common` の差分基盤を用いる)。
//!
//! 素朴実装(core)をオラクルとし、最適化機械と比較する。
//!
//! - データポインタを 29,990〜29,999 へ運ぶ接頭辞の後に、連続右移動・乗算ループ・走査ループ・連続左移動を置き、
//!   範囲外エラー(`pc`・`dp`・方向、およびテープ・出力・実行数)と境界上の成功の双方を比較する。`dp = 0` からの左側も同様。
//! - セル 0 で到達した乗算ループ・走査ループの本体が範囲外に触れうる場合でも、本体が実行されないためエラーにならない。
//! - `,` について、入力が空・尽きる・0x00 と 0xFF を含む場合に、EOF 時の 0 と入力読取位置が素朴実装と一致する。
//!
//! 各プログラムは停止が確実なので、上限なしで比較する。
//! 共通基盤(`tests/common`)は変更しない。

mod common;

use brainfuck_core::{MachineConfig, PointerDirection, RuntimeError, StopReason};
use brainfuck_optimized::{OptimizedMachine, OptimizedProgram};
use common::{assert_equivalent, observe_core};

/// テープの右端のセル番号(有効範囲は 0 以上 29,999 以下)。
const RIGHT_EDGE: usize = 29_999;

/// 右端付近の開始位置。
const NEAR_RIGHT: std::ops::RangeInclusive<usize> = 29_990..=RIGHT_EDGE;

/// 入出力の比較に用いる入力。空、尽きる、0x00 と 0xFF を含む、の各ケースを含む。
const INPUT_CASES: &[&[u8]] = &[
    &[],
    &[0x00],
    &[0xFF],
    &[0x00, 0xFF],
    &[0xFF, 0x00, 0x41],
    &[0x41, 0x00, 0xFF],
];

/// 入出力を含む停止する代表プログラム。`,` の EOF 時の 0 と、読み取り位置の一致を確認する。
const IO_PROGRAMS: &[&str] = &[
    ",.",
    ",.,.,.,.",
    ",,,.",
    ",[.,]",
    ",.,[.,]",
    ",[-].",
    ",+.",
    ",-.",
    ",>,<.",
    ",[>+<-]>.",
    ",+[.,]",
    ",[-]+.",
];

/// 右端から `to` の方向へ `from` から移動する命令列を返す。
fn moves(from: usize, to: usize) -> String {
    if to >= from {
        ">".repeat(to - from)
    } else {
        "<".repeat(from - to)
    }
}

/// 位置 `lo` から `hi` までのセルを 1 にし、データポインタを `hi` に置く命令列を返す。
fn paint(lo: usize, hi: usize) -> String {
    assert!(lo <= hi, "paint requires lo <= hi");
    let mut source = moves(0, lo);
    source.push('+');
    source.push_str(&">+".repeat(hi - lo));
    source
}

/// 同じ上限なしの設定で core を実行した結果(停止またはエラー)を返す。
fn core_result(source: &str) -> Result<StopReason, RuntimeError> {
    observe_core(source, &[], MachineConfig::unlimited()).result
}

/// 範囲外エラーの期待値を組み立てる。
fn pointer_error(
    pc: usize,
    dp: usize,
    direction: PointerDirection,
) -> Result<StopReason, RuntimeError> {
    Err(RuntimeError::PointerOutOfBounds { pc, dp, direction })
}

/// 右端へ連続右移動するプログラムについて、範囲外エラーと境界上の成功の双方を比較する。
///
/// 開始位置 `start` から `extra` 回右へ移動する。`start + extra` が右端を超えるとき、
/// 右端を超えて移動しようとする 30,000 個目の `>` で誤る(`pc` = 29,999)。
#[test]
fn consecutive_right_moves_match_the_core_at_the_right_edge() {
    for start in NEAR_RIGHT {
        for extra in 0..=12 {
            let source = format!("{}{}+", moves(0, start), ">".repeat(extra));
            let label = format!("start {start}, extra {extra}");
            let expected = if start + extra <= RIGHT_EDGE {
                Ok(StopReason::Halted)
            } else {
                pointer_error(RIGHT_EDGE, RIGHT_EDGE, PointerDirection::Right)
            };
            assert_eq!(core_result(&source), expected, "core result: {label}");
            assert_equivalent(&source, &[], MachineConfig::unlimited());
        }
    }
}

/// 連続左移動について、dp = 0 からの左側を含めて範囲外エラーと境界上の成功の双方を比較する。
///
/// 開始位置 `k` から `k + extra` 回左へ移動する。`extra >= 1` のとき、左端を超える移動で誤る。
/// その移動は `k + 1` 個目の `<` で、添字は `2k`、そのときの `dp` は 0 である。
#[test]
fn consecutive_left_moves_match_the_core_at_the_left_edge() {
    for k in 0..=9 {
        for extra in 0..=10 {
            let source = format!("{}{}+", moves(0, k), "<".repeat(k + extra));
            let label = format!("k {k}, extra {extra}");
            let expected = if extra == 0 {
                Ok(StopReason::Halted)
            } else {
                pointer_error(2 * k, 0, PointerDirection::Left)
            };
            assert_eq!(core_result(&source), expected, "core result: {label}");
            assert_equivalent(&source, &[], MachineConfig::unlimited());
        }
    }
}

/// 右端付近の乗算ループについて、本体の右移動が右端を超えるかどうかで成否が決まることを比較する。
///
/// セル `start` の値 3 に対し、本体が `start + m` へ加算する。`start + m` が右端を超えるとき、
/// 本体の 30,000 個目の位置へ移動する `>` で誤る。その添字は 30,004 である。
#[test]
fn multiplication_loop_reaching_past_the_right_edge_matches_the_core() {
    for start in NEAR_RIGHT {
        for m in 1..=10 {
            let source = format!(
                "{}+++[-{}+{}]",
                moves(0, start),
                ">".repeat(m),
                "<".repeat(m)
            );
            let label = format!("start {start}, m {m}");
            let expected = if start + m <= RIGHT_EDGE {
                Ok(StopReason::Halted)
            } else {
                pointer_error(30_004, RIGHT_EDGE, PointerDirection::Right)
            };
            assert_eq!(core_result(&source), expected, "core result: {label}");
            assert_equivalent(&source, &[], MachineConfig::unlimited());
        }
    }
}

/// 現在セルが 0 のため本体が実行されない乗算ループは、本体が範囲外に触れうる場合でも成功する。
#[test]
fn multiplication_loop_with_zero_cell_does_not_touch_out_of_range_body() {
    for start in NEAR_RIGHT {
        for m in 1..=10 {
            let source = format!("{}[-{}+{}]+", moves(0, start), ">".repeat(m), "<".repeat(m));
            assert_eq!(
                core_result(&source),
                Ok(StopReason::Halted),
                "zero cell right edge: start {start}, m {m}"
            );
            assert_equivalent(&source, &[], MachineConfig::unlimited());
        }
    }
    // dp = 0 からの左側: セル 0 が 0 なので、本体の左移動は実行されない。
    let source = "[-<+>]+";
    assert_eq!(core_result(source), Ok(StopReason::Halted));
    assert_equivalent(source, &[], MachineConfig::unlimited());
}

/// 右端付近の走査ループについて、途中の 1 が右端を超えて続く場合は範囲外エラー、
/// 右端の手前の 0 で止まる場合は成功することを比較する。
#[test]
fn scan_loop_at_the_right_edge_matches_the_core() {
    for hi in NEAR_RIGHT {
        for lo in [29_990, 29_995, hi] {
            if lo > hi {
                continue;
            }
            let source = format!("{}{}[>]", paint(lo, hi), moves(hi, lo));
            let label = format!("lo {lo}, hi {hi}");
            let expected = if hi == RIGHT_EDGE {
                // 右端の 1 を越えて移動する `>` で誤る。その添字はソースの末尾から 2 つ目。
                pointer_error(source.len() - 2, RIGHT_EDGE, PointerDirection::Right)
            } else {
                Ok(StopReason::Halted)
            };
            assert_eq!(core_result(&source), expected, "core result: {label}");
            assert_equivalent(&source, &[], MachineConfig::unlimited());
        }
    }
}

/// 左端付近の走査ループについて、途中の 1 が左端を超えて続く場合は範囲外エラー、
/// 左端のセル 0 で止まる場合は成功することを比較する。`dp = 0` からの左側を含む。
#[test]
fn scan_loop_at_the_left_edge_matches_the_core() {
    for hi in 0..=9 {
        // lo = 0: 走査が左端を超えて誤る。
        let source = format!("{}[<]", paint(0, hi));
        assert_eq!(
            core_result(&source),
            pointer_error(source.len() - 2, 0, PointerDirection::Left),
            "left edge error: hi {hi}"
        );
        assert_equivalent(&source, &[], MachineConfig::unlimited());

        // lo = 1: セル 0 が 0 で止まるため成功する。
        if hi >= 1 {
            let source = format!("{}[<]", paint(1, hi));
            assert_eq!(
                core_result(&source),
                Ok(StopReason::Halted),
                "left edge success: hi {hi}"
            );
            assert_equivalent(&source, &[], MachineConfig::unlimited());
        }
    }

    // dp = 0 のセル 0 で走査しても、ループ本体は実行されない。
    assert_equivalent("[<]", &[], MachineConfig::unlimited());
    assert_equivalent("[>]", &[], MachineConfig::unlimited());
}

/// 右端付近で、走査ループ・乗算ループの本体が実行されない形(セル 0)を比較する。
#[test]
fn scan_loop_with_zero_cell_at_the_right_edge_is_a_no_op() {
    for start in NEAR_RIGHT {
        let source = format!("{}[>]+", moves(0, start));
        assert_eq!(
            core_result(&source),
            Ok(StopReason::Halted),
            "start {start}"
        );
        assert_equivalent(&source, &[], MachineConfig::unlimited());
    }
}

/// 右端 0 から 29,999 への往復を含む、境界上の成功を比較する。
#[test]
fn round_trip_across_the_full_tape_succeeds() {
    let sources = [
        format!("{}{}+", ">".repeat(RIGHT_EDGE), "<".repeat(RIGHT_EDGE)),
        format!("{}+[-]{}+", ">".repeat(RIGHT_EDGE), "<".repeat(RIGHT_EDGE)),
        format!("{}+{}", ">".repeat(RIGHT_EDGE), "<".repeat(RIGHT_EDGE)),
    ];
    for source in &sources {
        assert_eq!(core_result(source), Ok(StopReason::Halted));
        assert_equivalent(source, &[], MachineConfig::unlimited());
    }
}

/// 範囲外エラーの後、同じ機械を再実行しても同じエラーで、状態は変化しない。
#[test]
fn pointer_error_leaves_the_optimized_machine_state_unchanged() {
    let source = format!("{}>+", ">".repeat(RIGHT_EDGE));
    let program = brainfuck_core::Program::parse(&source).expect("テスト用ソースは解析に成功する");
    let optimized = OptimizedProgram::compile(&program);
    let mut machine = OptimizedMachine::new(&optimized, Vec::new(), MachineConfig::unlimited());

    let first = machine.run();
    let snapshot = machine.snapshot();
    let tape = machine.tape().to_vec();
    assert_eq!(
        first,
        pointer_error(RIGHT_EDGE, RIGHT_EDGE, PointerDirection::Right)
    );

    let second = machine.run();
    assert_eq!(second, first, "re-run must report the same error");
    assert_eq!(machine.snapshot().steps_executed, snapshot.steps_executed);
    assert_eq!(machine.snapshot().pc, snapshot.pc);
    assert_eq!(machine.snapshot().data_pointer, snapshot.data_pointer);
    assert_eq!(machine.tape(), tape.as_slice());
}

/// 入出力を含むプログラムを、入力の全ケース(空・尽きる・0x00/0xFF を含む)で比較する。
#[test]
fn io_programs_match_the_core_for_every_input_case() {
    for &source in IO_PROGRAMS {
        for &input in INPUT_CASES {
            assert_equivalent(source, input, MachineConfig::unlimited());
        }
    }
}

/// 右端付近で入出力を行っても、範囲と入出力が素朴実装と一致する。
#[test]
fn io_near_the_right_edge_matches_the_core() {
    for start in NEAR_RIGHT {
        let source = format!("{}{}", moves(0, start), ",.,[.,]");
        for &input in INPUT_CASES {
            assert_equivalent(&source, input, MachineConfig::unlimited());
        }
    }
}

/// `,` は入力が空のとき 0 を格納し、エラーにはならない(EOF = 0)。
#[test]
fn read_at_eof_stores_zero_and_does_not_fail() {
    let observation = observe_core(",.", &[], MachineConfig::unlimited());
    assert_eq!(observation.result, Ok(StopReason::Halted));
    assert_eq!(observation.output, vec![0]);
    assert_eq!(observation.input_position, 0);

    // 入力を使い果たした後の `,` も 0 を格納する。
    let observation = observe_core(",,.", &[0xFF], MachineConfig::unlimited());
    assert_eq!(observation.result, Ok(StopReason::Halted));
    assert_eq!(observation.output, vec![0]);
    assert_eq!(observation.input_position, 1);
}

/// 入力読取位置の一致を、読み残しのある入力で確認する。
#[test]
fn unread_input_bytes_match_the_core_position() {
    let input: &[u8] = &[0x00, 0xFF, 0x41];
    assert_equivalent(",.", input, MachineConfig::unlimited());
    assert_equivalent(",,", input, MachineConfig::unlimited());

    let observation = observe_core(",.", input, MachineConfig::unlimited());
    assert_eq!(observation.input_position, 1);
    assert_eq!(observation.output, vec![0x00]);
}

/// `prefix_len` を先頭の添字とする本体 `body` を、絶対位置 `start` から実行したとき、
/// 絶対位置が `escape`(テープ外の最初の位置)に達する移動命令の添字を返す。達しなければ `None`。
fn first_escape_index(prefix_len: usize, start: isize, body: &str, escape: isize) -> Option<usize> {
    let mut pos = start;
    for (index, byte) in body.bytes().enumerate() {
        match byte {
            b'>' => pos += 1,
            b'<' => pos -= 1,
            _ => {}
        }
        if pos == escape {
            return Some(prefix_len + index);
        }
    }
    None
}

/// 右端へ越える移動の添字。越えれば範囲外エラー(dp は右端)、越えなければ停止の期待値を返す。
fn expected_right_body(
    start: usize,
    prefix_len: usize,
    body: &str,
) -> Result<StopReason, RuntimeError> {
    match first_escape_index(prefix_len, start as isize, body, RIGHT_EDGE as isize + 1) {
        Some(pc) => pointer_error(pc, RIGHT_EDGE, PointerDirection::Right),
        None => Ok(StopReason::Halted),
    }
}

/// 左端へ越える移動の添字。越えれば範囲外エラー(dp は 0)、越えなければ停止の期待値を返す。
fn expected_left_body(
    start: usize,
    prefix_len: usize,
    body: &str,
) -> Result<StopReason, RuntimeError> {
    match first_escape_index(prefix_len, start as isize, body, -1) {
        Some(pc) => pointer_error(pc, 0, PointerDirection::Left),
        None => Ok(StopReason::Halted),
    }
}

/// 開始セルが非 0 で、本体の到達範囲が右端を越えるが、項の対象はすべて右端内に収まる乗算ループ。
///
/// 到達範囲だけで判定する実装が誤ると、対象は範囲内なのに範囲外エラーとなる(逆に、範囲の判定を
/// 省くと素朴実装のエラーを取り逃す)。開始位置 29,994 は到達が右端ちょうど(成功)、29,995 は 1 越える(エラー)。
#[test]
fn multiplication_reaching_past_the_right_edge_with_in_range_targets_matches_the_core() {
    // 本体の到達範囲はそれぞれ +5 と +6。項の対象は +1 のみ、または +1 と +2。
    let bodies = ["->+>>>><<<<<", "->+>+>>>><<<<<<"];
    for body in bodies {
        for start in NEAR_RIGHT {
            let prefix_len = start + 4;
            let source = format!("{}+++[{}]", moves(0, start), body);
            let label = format!("start {start}, body {body}");
            let expected = expected_right_body(start, prefix_len, body);
            assert_eq!(core_result(&source), expected, "core result: {label}");
            assert_equivalent(&source, &[], MachineConfig::unlimited());
        }
    }

    // 境界の確認: 到達が右端ちょうど(+5 → 29,999)なら成功、1 越える(29,995 から +5 → 30,000)ならエラー。
    let exact = format!("{}+++[->+>>>><<<<<]", moves(0, 29_994));
    assert_eq!(core_result(&exact), Ok(StopReason::Halted));
    let over = format!("{}+++[->+>>>><<<<<]", moves(0, 29_995));
    assert_eq!(
        core_result(&over),
        pointer_error(29_995 + 4 + 6, RIGHT_EDGE, PointerDirection::Right)
    );
    assert_equivalent(&exact, &[], MachineConfig::unlimited());
    assert_equivalent(&over, &[], MachineConfig::unlimited());
}

/// 開始セルが非 0 の左側の乗算ループ。`dp = 0` でエラー、`dp = 1` 以上で成功する、などの境界を比較する。
///
/// 本体は `<<` を含む変種、2 項の変種、左への到達が項の対象より広い変種(`-<<>><+>` は到達 -2、項の対象 -1)を含む。
#[test]
fn left_multiplication_with_nonzero_start_matches_the_core_at_the_left_edge() {
    let bodies = ["-<+>", "-<<+>>", "-<<>><+>", "-<+<+>>"];
    for body in bodies {
        for start in 0..=9 {
            let prefix_len = start + 4;
            let source = format!("{}+++[{}]", moves(0, start), body);
            let label = format!("start {start}, body {body}");
            let expected = expected_left_body(start, prefix_len, body);
            assert_eq!(core_result(&source), expected, "core result: {label}");
            assert_equivalent(&source, &[], MachineConfig::unlimited());
        }
    }

    // 代表例: `+++[-<+>]` は dp = 0 でエラー(添字 5)、`>+++[-<+>]` は dp = 1 で成功する。
    assert_eq!(
        core_result("+++[-<+>]"),
        pointer_error(5, 0, PointerDirection::Left)
    );
    assert_eq!(core_result(">+++[-<+>]"), Ok(StopReason::Halted));
    assert_equivalent("+++[-<+>]", &[], MachineConfig::unlimited());
    assert_equivalent(">+++[-<+>]", &[], MachineConfig::unlimited());
}

/// 開始セルが非 0 で、到達範囲(-2)が左端を越えるが、項の対象(-1)はテープ内に収まる移動/加算/乗算。
///
/// `>[<<>+>-]` 相当の本体は正味のポインタ移動が 0 で、開始セルを 1 減らし、一つ左のセルへ加算する。
/// `dp = 0` と `dp = 1` では本体の 2 つ目の `<` が左端を越えるため範囲外エラー、
/// `dp = 2` では到達が左端ちょうど(0)に収まり成功する。項の対象だけで判定する実装は誤る。
#[test]
fn transfer_reaching_past_the_left_edge_with_in_range_targets_matches_the_core() {
    let body = "<<>+>-";
    for start in 0..=2 {
        let prefix_len = start + 4;
        let source = format!("{}+++[{}]", moves(0, start), body);
        let label = format!("start {start}, body {body}");
        let expected = expected_left_body(start, prefix_len, body);
        assert_eq!(core_result(&source), expected, "core result: {label}");
        assert_equivalent(&source, &[], MachineConfig::unlimited());
    }

    // 代表例: dp = 1 では `<` が 0 へ、2 つ目の `<` が -1 へ移るため、添字 5 の `<` で誤る。
    assert_eq!(
        core_result(">+++[<<>+>-]"),
        pointer_error(6, 0, PointerDirection::Left)
    );
    // dp = 2 は到達が左端ちょうど(0)で、成功する。
    assert_eq!(core_result(">>+++[<<>+>-]"), Ok(StopReason::Halted));
    assert_equivalent(">+++[<<>+>-]", &[], MachineConfig::unlimited());
    assert_equivalent(">>+++[<<>+>-]", &[], MachineConfig::unlimited());
}

/// 全バイト値 0..=255 を入力して出力するエコーを、素朴実装と比較する。
#[test]
fn echo_of_every_byte_value_matches_the_core() {
    let input: Vec<u8> = (0..=255).collect();
    let source = ",.".repeat(256);
    assert_eq!(
        observe_core(&source, &input, MachineConfig::unlimited()).output,
        input
    );
    assert_equivalent(&source, &input, MachineConfig::unlimited());
}
