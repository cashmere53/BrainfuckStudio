//! 素朴実装(core)と最適化実装の実行時間と実行数を、同一プログラムで比較するベンチマーク。
//!
//! 実行: `cargo bench -p brainfuck-optimized`。`harness = false` のため標準のテストハーネスは使わず、
//! `cargo test --workspace` では実行されない。
//!
//! 手順:
//! 1. 各プログラムについて両実装を実行し、観測結果(停止/エラー、`pc`、データポインタ、現在セル、
//!    素朴換算の実行数、停止フラグ、テープ、出力、入力位置)が一致し、素朴実装が停止することを確認する。
//!    1 本でも一致しなければ計測せず非ゼロ終了する。
//! 2. 一致したプログラムについて、ウォームアップ 1 回の後に各 5 回計測し、中央値を採用する。
//!
//! 計測対象は実行のみ。最適化実装の IR への変換は計測前に 1 回だけ行い、時間に含めない。
//! 時間の大小は成否判定に使わない。ベンチマークは最適化の正しさの証明ではない。
//!
//! 比較処理は `tests/common` を参照できないため、ここに最小限を持つ。

use std::hint::black_box;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use brainfuck_core::{Machine, MachineConfig, Program, RuntimeError, StopReason};
use brainfuck_optimized::{OptimizedMachine, OptimizedProgram};

/// 計測対象の固定プログラム(名前, ソース)。いずれも入力なし・停止する。
const PROGRAMS: [(&str, &str); 5] = [
    ("runs", include_str!("programs/runs.bf")),
    ("clear", include_str!("programs/clear.bf")),
    ("multiply", include_str!("programs/multiply.bf")),
    ("scan", include_str!("programs/scan.bf")),
    ("general", include_str!("programs/general.bf")),
];

/// 暴走時に有限時間で失敗させるための安全上限。
const STEP_CAP: u64 = 50_000_000;

/// 計測回数(ウォームアップを除く)。中央値を採用する。
const MEASURED_RUNS: usize = 5;

/// 1 回の実行で観測できる項目の組(両実装で共通の形)。
#[derive(Debug)]
struct Observed {
    result: Result<StopReason, RuntimeError>,
    pc: usize,
    data_pointer: usize,
    current_cell: u8,
    steps_executed: u64,
    halted: bool,
    tape: Vec<u8>,
    output: Vec<u8>,
    input_position: usize,
}

/// 検証済みのプログラム。計測の前に両実装の一致を確認したもの。
struct Prepared {
    name: &'static str,
    program: Program,
    optimized: OptimizedProgram,
    naive_steps: u64,
    ir_ops: u64,
}

/// 1 プログラムの計測結果。
struct Row {
    name: &'static str,
    naive_steps: u64,
    ir_ops: u64,
    naive: Duration,
    optimized: Duration,
}

fn config() -> MachineConfig {
    MachineConfig::with_max_steps(STEP_CAP)
}

/// 素朴実装を実行し、観測結果と実行時間を返す。時間は `Machine` の生成と実行のみを含む。
fn run_naive(program: Program) -> (Observed, Duration) {
    let start = Instant::now();
    let mut machine = Machine::new(program, Vec::new(), config());
    let result = machine.run();
    let elapsed = start.elapsed();
    let snapshot = machine.snapshot();
    let observed = Observed {
        result,
        pc: snapshot.pc,
        data_pointer: snapshot.data_pointer,
        current_cell: snapshot.current_cell,
        steps_executed: snapshot.steps_executed,
        halted: snapshot.halted,
        tape: machine.tape().to_vec(),
        output: machine.output().to_vec(),
        input_position: machine.input_position(),
    };
    black_box(&observed);
    (observed, elapsed)
}

/// 最適化実装を実行し、観測結果・IR 操作の実行回数・実行時間を返す。時間は機械の生成と実行のみを含む。
fn run_optimized(program: &OptimizedProgram) -> (Observed, u64, Duration) {
    let start = Instant::now();
    let mut machine = OptimizedMachine::new(program, Vec::new(), config());
    let result = machine.run();
    let elapsed = start.elapsed();
    let snapshot = machine.snapshot();
    let observed = Observed {
        result,
        pc: snapshot.pc,
        data_pointer: snapshot.data_pointer,
        current_cell: snapshot.current_cell,
        steps_executed: snapshot.steps_executed,
        halted: snapshot.halted,
        tape: machine.tape().to_vec(),
        output: machine.output().to_vec(),
        input_position: machine.input_position(),
    };
    black_box(&observed);
    (observed, snapshot.ir_ops_executed, elapsed)
}

/// 不一致の項目名を列挙する。空なら両実装の観測結果は一致する。
fn mismatched_fields(naive: &Observed, optimized: &Observed) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if naive.result != optimized.result {
        fields.push("result");
    }
    if naive.pc != optimized.pc {
        fields.push("pc");
    }
    if naive.data_pointer != optimized.data_pointer {
        fields.push("data_pointer");
    }
    if naive.current_cell != optimized.current_cell {
        fields.push("current_cell");
    }
    if naive.steps_executed != optimized.steps_executed {
        fields.push("steps_executed");
    }
    if naive.halted != optimized.halted {
        fields.push("halted");
    }
    if naive.tape != optimized.tape {
        fields.push("tape");
    }
    if naive.output != optimized.output {
        fields.push("output");
    }
    if naive.input_position != optimized.input_position {
        fields.push("input_position");
    }
    fields
}

/// プログラムを解析・変換し、両実装の観測結果が一致して素朴実装が停止することを確認する。
fn prepare(name: &'static str, source: &str) -> Result<Prepared, String> {
    let program =
        Program::parse(source).map_err(|error| format!("{name}: 解析に失敗: {error:?}"))?;
    let optimized = OptimizedProgram::compile(&program);

    let (naive, _) = run_naive(program.clone());
    let (optimized_observed, ir_ops, _) = run_optimized(&optimized);

    if naive.result != Ok(StopReason::Halted) {
        return Err(format!(
            "{name}: 素朴実装が停止しない、または失敗した: {:?}",
            naive.result
        ));
    }
    let fields = mismatched_fields(&naive, &optimized_observed);
    if !fields.is_empty() {
        return Err(format!(
            "{name}: 両実装の観測結果が一致しない(不一致の項目: {})",
            fields.join(", ")
        ));
    }

    Ok(Prepared {
        name,
        program,
        optimized,
        naive_steps: naive.steps_executed,
        ir_ops,
    })
}

/// 計測値の中央値を返す。
fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

/// ウォームアップ 1 回の後に各 `MEASURED_RUNS` 回計測し、中央値を返す。
fn measure(prepared: &Prepared) -> Row {
    run_naive(prepared.program.clone());
    run_optimized(&prepared.optimized);

    let mut naive_times = Vec::with_capacity(MEASURED_RUNS);
    let mut optimized_times = Vec::with_capacity(MEASURED_RUNS);
    for _ in 0..MEASURED_RUNS {
        naive_times.push(run_naive(prepared.program.clone()).1);
        optimized_times.push(run_optimized(&prepared.optimized).2);
    }

    Row {
        name: prepared.name,
        naive_steps: prepared.naive_steps,
        ir_ops: prepared.ir_ops,
        naive: median(naive_times),
        optimized: median(optimized_times),
    }
}

fn print_report(rows: &[Row]) {
    println!("brainfuck-optimized: 素朴実装 と 最適化実装 の比較ベンチマーク");
    println!("各列の定義:");
    println!("  素朴換算の実行数: 素朴実装(core)の steps_executed。両実装で常に同一。");
    println!(
        "  IR 操作の実行回数: 最適化実装の ir_ops_executed。方式固有の実行単位で、素朴換算の実行数とは別の定義(縮退再生は数えない)。"
    );
    println!(
        "  素朴 ms / 最適化 ms: ウォームアップ 1 回の後の {MEASURED_RUNS} 回計測の中央値。最適化は IR への変換時間を含まない。"
    );
    println!("  倍率: 素朴 ms ÷ 最適化 ms(1 より大きいほど最適化実装が速い)。");
    println!("ベンチマークは最適化の正しさの証明ではない。正しさは正規テスト(cargo test --workspace)で確認する。");
    println!("時間の大小は成否判定に使わない。");
    println!();
    println!(
        "{:<10} | {:>16} | {:>16} | {:>10} | {:>10} | {:>8}",
        "プログラム", "素朴換算の実行数", "IR操作の実行回数", "素朴 ms", "最適化 ms", "倍率"
    );
    for row in rows {
        let naive_ms = row.naive.as_secs_f64() * 1000.0;
        let optimized_ms = row.optimized.as_secs_f64() * 1000.0;
        println!(
            "{:<10} | {:>16} | {:>16} | {:>10.3} | {:>10.3} | {:>8.2}",
            row.name,
            row.naive_steps,
            row.ir_ops,
            naive_ms,
            optimized_ms,
            naive_ms / optimized_ms,
        );
    }
}

fn main() -> ExitCode {
    // 計測の前に全プログラムの一致を確認する。1 本でも不一致なら計測せず失敗とする。
    let mut prepared = Vec::with_capacity(PROGRAMS.len());
    for (name, source) in PROGRAMS {
        match prepare(name, source) {
            Ok(item) => prepared.push(item),
            Err(message) => {
                eprintln!("ベンチマークを中止しました: {message}");
                return ExitCode::FAILURE;
            }
        }
    }

    let rows: Vec<Row> = prepared.iter().map(measure).collect();
    print_report(&rows);
    ExitCode::SUCCESS
}
