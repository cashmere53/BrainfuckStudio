//! 素朴換算コスト `Op::naive_cost` と到達範囲 `Op::static_reach` の契約テスト(公開 API のみ使用)。
//!
//! オラクルは core の `Machine::step` / `snapshot` / `tape` である。操作の先頭(`pc` が操作の
//! 開始命令添字に達した時点)の現在セル値 `v` と走査の反復数 `k` を、テストが core の状態から
//! 独立に求め、そこから操作の範囲を出るまでに core が実行した命令数を実測する。その実測値と
//! `naive_cost(v, k)` を比較する。
//!
//! 範囲条件は `static_reach` の結果から求め、core が範囲外エラーを起こすかどうかと照合する。
//! コストの比較は範囲条件が満たされ、エラーが起きない場合に限る。エラーが起きる場合は
//! `PointerOutOfBounds` であることのみ確認する。

use brainfuck_core::{Machine, MachineConfig, Program, RuntimeError, TAPE_SIZE};
use brainfuck_optimized::{Op, OpKind, OptimizedProgram};
use std::collections::BTreeSet;

/// セル値の代表値。0、最小の非 0、中間、符号境界、最大を含む。
const VALUES: [u8; 6] = [0, 1, 2, 127, 128, 255];

/// テストプログラムが無限ループしないことを保証するための安全上限(テスト側の監視)。
const STEP_CAP: u64 = 5_000_000;

/// 検証で観測した内容の集計。
#[derive(Default)]
struct Report {
    /// 操作の先頭に達した回数(一般ループ内の反復を含む)。
    entries: usize,
    /// 観測した操作種別の名前。
    kinds: BTreeSet<&'static str>,
    /// 範囲外エラーで終わったプログラムの数。
    errors: usize,
}

fn kind_name(kind: &OpKind) -> &'static str {
    match kind {
        OpKind::Add(_) => "Add",
        OpKind::Move(_) => "Move",
        OpKind::Output => "Output",
        OpKind::Input => "Input",
        OpKind::LoopStart { .. } => "LoopStart",
        OpKind::LoopEnd { .. } => "LoopEnd",
        OpKind::Clear { .. } => "Clear",
        OpKind::Transfer { .. } => "Transfer",
        OpKind::Scan { .. } => "Scan",
        _ => "unknown",
    }
}

/// `dp` からの到達範囲が `[0, TAPE_SIZE)` に収まるかを、`static_reach` の結果から判定する。
fn reach_within_tape(op: &Op, dp: usize) -> bool {
    let (min, max) = op
        .static_reach()
        .expect("移動を伴う操作は到達範囲を持つ必要がある");
    let dp = dp as isize;
    dp + min >= 0 && dp + max < TAPE_SIZE as isize
}

/// 走査ループを、エントリ時点のテープから独立に歩く。
///
/// 返り値は `(範囲外エラーが起きるか, 反復数 k)`。各反復は本体で `stride` だけ進み、
/// その位置で範囲外になれば(移動が範囲外へ出る)エラー、0 のセルに至れば反復数 k で終了する。
fn scan_walk(dp: usize, stride: isize, tape: &[u8]) -> (bool, u64) {
    let mut iterations: u64 = 0;
    let mut position = dp as isize;
    loop {
        iterations += 1;
        position += stride;
        if position < 0 || position >= TAPE_SIZE as isize {
            return (true, 0);
        }
        if tape[position as usize] == 0 {
            return (false, iterations);
        }
    }
}

/// エントリ時点の状態から、範囲外エラーの期待と走査の反復数 k を求める。
fn expectation(op: &Op, v: u8, dp: usize, tape: &[u8]) -> (bool, u64) {
    match op.kind() {
        // 移動は常に範囲を評価する。
        OpKind::Move(_) => (!reach_within_tape(op, dp), 0),
        // 乗算・加算ループは開始セルが非 0 のときだけ本体を実行する。
        OpKind::Transfer { .. } if v != 0 => (!reach_within_tape(op, dp), 0),
        OpKind::Scan { stride } if v != 0 => scan_walk(dp, *stride, tape),
        _ => (false, 0),
    }
}

/// 1 本のプログラムを core の 1 命令実行に対して検証する。
///
/// 操作の先頭に達するたびに、コストと範囲条件を照合する。範囲外エラーが起きた時点で終了する。
fn check(source: &str, report: &mut Report) {
    let program = Program::parse(source).expect("テスト用ソースは解析に成功する必要がある");
    let optimized = OptimizedProgram::compile(&program);
    let ops = optimized.ops();

    // 操作の開始命令添字から操作添字への対応。操作の範囲は連続し重ならない。
    let mut op_at = vec![None; program.len() + 1];
    for (index, op) in ops.iter().enumerate() {
        op_at[op.source_range().start] = Some(index);
    }

    let mut machine = Machine::new(program.clone(), Vec::new(), MachineConfig::unlimited());
    loop {
        let snapshot = machine.snapshot();
        if snapshot.halted {
            return;
        }
        assert!(
            snapshot.steps_executed < STEP_CAP,
            "{source}: 安全上限を超えた"
        );
        let index = op_at[snapshot.pc]
            .unwrap_or_else(|| panic!("{source}: pc={} は操作の先頭ではない", snapshot.pc));
        let op = &ops[index];
        let range = op.source_range();
        match op.kind() {
            OpKind::Move(_) | OpKind::Transfer { .. } => assert!(
                op.static_reach().is_some(),
                "{source}: 移動を伴う操作の到達範囲が無い"
            ),
            _ => assert!(
                op.static_reach().is_none(),
                "{source}: 静的な到達範囲を持たない操作が範囲を返した"
            ),
        }

        let v = snapshot.current_cell;
        let dp = snapshot.data_pointer;
        let (expect_error, iterations) = expectation(op, v, dp, machine.tape());
        report.entries += 1;
        report.kinds.insert(kind_name(op.kind()));

        let before = snapshot.steps_executed;
        let mut error = None;
        loop {
            let current = machine.snapshot();
            if current.halted || !range.contains(&current.pc) {
                break;
            }
            if let Err(e) = machine.step() {
                error = Some(e);
                break;
            }
        }

        match error {
            Some(e) => {
                assert!(
                    expect_error,
                    "{source}: 想定外の範囲外エラー: op={range:?} kind={:?} v={v} dp={dp} error={e}",
                    op.kind()
                );
                assert!(
                    matches!(e, RuntimeError::PointerOutOfBounds { .. }),
                    "{source}: 範囲外以外のエラー: {e}"
                );
                report.errors += 1;
                return;
            }
            None => {
                assert!(
                    !expect_error,
                    "{source}: 範囲外エラーが起きるはずが起きなかった: op={range:?} kind={:?} v={v} dp={dp}",
                    op.kind()
                );
                let actual = machine.snapshot().steps_executed - before;
                assert_eq!(
                    op.naive_cost(v, iterations),
                    actual,
                    "{source}: コストが core の実測と不一致: op={range:?} kind={:?} v={v} dp={dp} k={iterations}",
                    op.kind()
                );
            }
        }
    }
}

/// 代表的な形のループ・入出力・走査を含むプログラムの本体(先頭でセル 0 に v を置いてから続ける)。
const COST_BODIES: &[&str] = &[
    "[-]",
    "[+]",
    "[->+<]",
    "[->>+++<<]",
    "[->+>+<<]",
    "[>.<-]",
    "[>[-]<-]",
    "[>]",
    "[>>]",
    ">+>+>+<<<[>]",
    ">+>+>+>+<<<<[>>]",
    ",",
    ",.",
    ".+-><",
];

/// 開始セルに `v` を置いたうえで、本体を続けたプログラム群。
fn cost_sources(v: u8) -> Vec<String> {
    let set = "+".repeat(usize::from(v));
    COST_BODIES
        .iter()
        .map(|body| format!("{set}{body}"))
        .collect()
}

/// 端(dp = 29,999 や dp = 0)での範囲外・境界上の成功を検証するプログラム群。
fn edge_sources(v: u8) -> Vec<String> {
    let set = "+".repeat(usize::from(v));
    let right_end = ">".repeat(29_999);
    vec![
        // 右端のセルに v を置いてから右へ走査: v != 0 なら範囲外。
        format!("{right_end}{set}[>]"),
        // 右端で右へ 1 進む乗算: 到達範囲 (0, 1) が範囲外。
        format!("{right_end}{set}[>+<-]"),
        // 右端の 1 つ手前から乗算: 到達範囲 (0, 1) は 29,999 までで収まる。
        format!("{}{set}[>+<-]", ">".repeat(29_998)),
        // 左端で左へ進む乗算: 到達範囲 (-1, 0) が範囲外。
        format!("{set}[<+>-]"),
        // 左端で左へ走査: v != 0 なら範囲外。
        format!("{set}[<]"),
        // 左端で左へ移動: 常に範囲外。
        format!("{set}<"),
        // 右端で右へ移動: 常に範囲外。
        format!("{right_end}>"),
        // 右端の 1 つ手前から右へ移動: 境界上で成功。
        format!("{}>", ">".repeat(29_998)),
    ]
}

#[test]
fn naive_cost_matches_core_steps_for_every_operation_kind() {
    let mut report = Report::default();
    for v in VALUES {
        for source in cost_sources(v) {
            check(&source, &mut report);
        }
    }

    for name in [
        "Add",
        "Move",
        "Output",
        "Input",
        "LoopStart",
        "LoopEnd",
        "Clear",
        "Transfer",
        "Scan",
    ] {
        assert!(
            report.kinds.contains(name),
            "操作種別 {name} が検証されていない: {:?}",
            report.kinds
        );
    }
    assert!(report.entries > 0);
}

#[test]
fn static_reach_range_matches_core_out_of_range_errors() {
    let mut report = Report::default();
    for v in VALUES {
        for source in edge_sources(v) {
            check(&source, &mut report);
        }
    }

    // 範囲外エラーで終わる例と、境界上で成功して最後まで進む例の双方を含むこと。
    assert!(report.errors > 0, "範囲外エラーが検証されていない");
    assert!(report.entries > 0);
}
