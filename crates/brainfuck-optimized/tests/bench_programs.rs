//! ベンチマーク用プログラムの健全性確認(正規テスト・時間計測なし)。
//!
//! `benches/programs/*.bf` の 5 本について、次を確認する。
//!
//! - 入力なしで停止し、素朴実装の実行数が数百万程度であること(下限で確認する)
//! - 素朴実装と最適化実装が観測的に一致すること
//! - パターン主体の 4 本は IR 操作の実行回数が素朴換算の実行数より小さいこと
//! - 一般ループ主体の 1 本は IR 操作の実行回数が素朴換算の実行数以下で、差が小さいこと
//!
//! 実行時間は計測しない。時間の大小は成否判定に使わない。ベンチマークは `benches/` 側で実行する。

mod common;

use brainfuck_core::{MachineConfig, StopReason};
use common::{assert_observations_equivalent, observe_core, observe_optimized, Observation};

/// 連続命令主体のプログラム。
const RUNS_SOURCE: &str = include_str!("../benches/programs/runs.bf");

/// クリア主体のプログラム。
const CLEAR_SOURCE: &str = include_str!("../benches/programs/clear.bf");

/// 移動/加算/乗算主体のプログラム。
const MULTIPLY_SOURCE: &str = include_str!("../benches/programs/multiply.bf");

/// 走査主体のプログラム。
const SCAN_SOURCE: &str = include_str!("../benches/programs/scan.bf");

/// 最適化されにくい一般ループ主体のプログラム。
const GENERAL_SOURCE: &str = include_str!("../benches/programs/general.bf");

/// 暴走時に有限時間で失敗させるための安全上限。正常なプログラムはこれより遥かに少ない命令数で停止する。
const STEP_CAP: u64 = 50_000_000;

/// 「数百万命令程度」の下限。ベンチマークとして意味のある規模であることを確認する。
const MIN_NAIVE_STEPS: u64 = 1_000_000;

/// プログラムを入力なしで素朴実装と最適化実装の両方で実行し、停止と一致を確認する。
///
/// 戻り値は素朴実装の観測結果と、最適化機械の IR 操作の実行回数。
fn run_and_compare(name: &str, source: &str) -> (Observation, u64) {
    let config = MachineConfig::with_max_steps(STEP_CAP);

    let core = observe_core(source, b"", config);
    assert_eq!(
        core.result,
        Ok(StopReason::Halted),
        "{name}: 素朴実装は入力なしで停止する必要がある"
    );
    assert!(
        core.steps_executed >= MIN_NAIVE_STEPS,
        "{name}: 素朴換算の実行数 {} が下限 {MIN_NAIVE_STEPS} に達していない",
        core.steps_executed
    );

    let (optimized, ir_ops) = observe_optimized(source, b"", config);
    assert_observations_equivalent(name, &core, &optimized, ir_ops);

    (core, ir_ops)
}

/// 連続命令主体: 素朴実装と一致して停止し、統合により IR 操作数が実行数より小さい。
#[test]
fn consecutive_program_halts_matches_core_and_merges_instructions() {
    let (core, ir_ops) = run_and_compare("runs", RUNS_SOURCE);
    assert!(
        ir_ops < core.steps_executed,
        "runs: IR 操作数 {ir_ops} は素朴換算の実行数 {} より小さいこと",
        core.steps_executed
    );
}

/// クリア主体: 素朴実装と一致して停止し、`[-]` の一括化により IR 操作数が実行数より小さい。
#[test]
fn clear_program_halts_matches_core_and_collapses_clear_loops() {
    let (core, ir_ops) = run_and_compare("clear", CLEAR_SOURCE);
    assert!(
        ir_ops < core.steps_executed,
        "clear: IR 操作数 {ir_ops} は素朴換算の実行数 {} より小さいこと",
        core.steps_executed
    );
}

/// 移動/加算/乗算主体: 素朴実装と一致して停止し、乗算ループの一括化により IR 操作数が実行数より小さい。
#[test]
fn multiply_program_halts_matches_core_and_collapses_transfer_loops() {
    let (core, ir_ops) = run_and_compare("multiply", MULTIPLY_SOURCE);
    assert!(
        ir_ops < core.steps_executed,
        "multiply: IR 操作数 {ir_ops} は素朴換算の実行数 {} より小さいこと",
        core.steps_executed
    );
}

/// 走査主体: 素朴実装と一致して停止し、走査ループの一括化により IR 操作数が実行数より小さい。
#[test]
fn scan_program_halts_matches_core_and_collapses_scan_loops() {
    let (core, ir_ops) = run_and_compare("scan", SCAN_SOURCE);
    assert!(
        ir_ops < core.steps_executed,
        "scan: IR 操作数 {ir_ops} は素朴換算の実行数 {} より小さいこと",
        core.steps_executed
    );
}

/// 一般ループ主体: 素朴実装と一致して停止する。パターンに該当しないため、IR 操作数と実行数の差は小さい。
#[test]
fn general_loop_program_halts_matches_core_with_small_difference() {
    let (core, ir_ops) = run_and_compare("general", GENERAL_SOURCE);
    assert!(
        ir_ops <= core.steps_executed,
        "general: IR 操作数 {ir_ops} は素朴換算の実行数 {} 以下であること",
        core.steps_executed
    );
    assert!(
        ir_ops * 2 >= core.steps_executed,
        "general: IR 操作数 {ir_ops} と素朴換算の実行数 {} の差が大きすぎる(一般ループ主体は差が小さいこと)",
        core.steps_executed
    );
}
