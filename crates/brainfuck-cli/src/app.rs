//! プログラムの実行と結果の整理。
//!
//! 解析済みプログラム・入力バイト列・解決済みの命令数上限から core の機械を生成し、
//! `run` を 1 回だけ呼ぶ。実行意味論と上限判定は core に委ね、ここでは再実装しない。
//! 出力・スナップショット・判定は [`RunReport`] に束ね、後続の出力段へ渡す。

// 後続タスク(4.2 以降)で実行経路に接続するまで未使用のため許可する。
#![allow(dead_code)]

use brainfuck_core::{Machine, MachineConfig, MachineSnapshot, Program, RuntimeError, StopReason};

use crate::error::CliError;

/// 実行の結果を束ねた値。成否にかかわらず、取得できた出力とスナップショットを保持する。
#[derive(Debug)]
struct RunReport {
    /// プログラムが出力したバイト列(実行エラー時は部分出力)。
    output: Vec<u8>,
    /// 停止時(または中断時)の機械の状態。
    snapshot: MachineSnapshot,
    /// 正常停止なら `Ok(())`、それ以外は分類済みのエラー。
    result: Result<(), CliError>,
}

/// 機械を生成して 1 回だけ実行し、出力・スナップショット・判定を返す。
///
/// `max_steps` が `Some(n)` なら上限 `n`、`None` なら制限なしで実行する。
fn run_program(program: Program, input: Vec<u8>, max_steps: Option<u64>) -> RunReport {
    let config = match max_steps {
        Some(limit) => MachineConfig::with_max_steps(limit),
        None => MachineConfig::unlimited(),
    };
    let mut machine = Machine::new(program, input, config);
    let outcome = machine.run();
    RunReport {
        output: machine.output().to_vec(),
        snapshot: machine.snapshot(),
        result: classify_stop(outcome),
    }
}

/// `run` の戻り値を CLI の判定へ写す。
///
/// 正常停止のみ成功とする。core の予約済み停止理由(現状は返されない)は未完了停止、
/// core の実行エラーは実行エラーとして分類する。
fn classify_stop(outcome: Result<StopReason, RuntimeError>) -> Result<(), CliError> {
    match outcome {
        Ok(StopReason::Halted) => Ok(()),
        Ok(_) => Err(CliError::Incomplete),
        Err(error) => Err(CliError::Runtime(error)),
    }
}

/// 状態表示の文字列を作る。値はスナップショットをそのまま用いる。
fn format_state(snapshot: &MachineSnapshot) -> String {
    format!(
        "state: pc={} dp={} steps={}\n",
        snapshot.pc, snapshot.data_pointer, snapshot.steps_executed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CliError;
    use crate::source::parse_source;
    use brainfuck_core::{PointerDirection, RuntimeError};

    fn program(source: &[u8]) -> Program {
        parse_source(source).expect("test program must parse")
    }

    #[test]
    fn run_within_limit_halts_successfully() {
        let report = run_program(program(b"+++"), Vec::new(), Some(3));
        assert!(report.result.is_ok());
        assert!(report.output.is_empty());
        assert_eq!(report.snapshot.steps_executed, 3);
        assert!(report.snapshot.halted);
    }

    #[test]
    fn run_limit_two_exceeds_limit_with_runtime_error() {
        let report = run_program(program(b"+++"), Vec::new(), Some(2));
        match report.result {
            Err(CliError::Runtime(RuntimeError::StepLimitExceeded { limit, pc })) => {
                assert_eq!(limit, 2);
                assert_eq!(pc, 2);
            }
            other => panic!("expected step limit runtime error, got {other:?}"),
        }
        assert_eq!(report.snapshot.steps_executed, 2);
        assert!(!report.snapshot.halted);
    }

    #[test]
    fn run_limit_zero_exceeds_limit_before_any_step() {
        let report = run_program(program(b"+++"), Vec::new(), Some(0));
        assert!(matches!(
            report.result,
            Err(CliError::Runtime(RuntimeError::StepLimitExceeded {
                limit: 0,
                pc: 0
            }))
        ));
        assert_eq!(report.snapshot.steps_executed, 0);
    }

    #[test]
    fn empty_program_with_limit_zero_succeeds() {
        let report = run_program(program(b""), Vec::new(), Some(0));
        assert!(report.result.is_ok());
        assert!(report.output.is_empty());
        assert_eq!(report.snapshot.steps_executed, 0);
    }

    #[test]
    fn unlimited_run_completes_past_default_style_limits() {
        let report = run_program(program(b"+++"), Vec::new(), None);
        assert!(report.result.is_ok());
        assert_eq!(report.snapshot.steps_executed, 3);
    }

    #[test]
    fn pointer_out_of_bounds_keeps_partial_output_and_snapshot() {
        // `+` でセルを 1 にし `.` で出力した後、`<` で範囲外へ移動する。
        let report = run_program(program(b"+.<"), Vec::new(), None);
        assert_eq!(report.output, vec![1u8]);
        assert_eq!(report.snapshot.pc, 2);
        assert_eq!(report.snapshot.data_pointer, 0);
        assert_eq!(report.snapshot.steps_executed, 2);
        assert!(matches!(
            report.result,
            Err(CliError::Runtime(RuntimeError::PointerOutOfBounds {
                pc: 2,
                dp: 0,
                direction: PointerDirection::Left,
            }))
        ));
    }

    #[test]
    fn input_instruction_reads_supplied_input() {
        let report = run_program(program(b",."), vec![0x41], None);
        assert!(report.result.is_ok());
        assert_eq!(report.output, vec![0x41]);
    }

    #[test]
    fn classify_halted_is_success() {
        assert!(classify_stop(Ok(StopReason::Halted)).is_ok());
    }

    #[test]
    fn classify_reserved_stop_reasons_are_incomplete() {
        for reason in [
            StopReason::StepLimitReached,
            StopReason::Breakpoint,
            StopReason::Cancelled,
        ] {
            let error = classify_stop(Ok(reason)).expect_err("reserved reason must fail");
            assert!(matches!(error, CliError::Incomplete));
            assert_eq!(error.exit_code(), 3);
        }
    }

    #[test]
    fn classify_core_error_is_runtime_error() {
        let error = classify_stop(Err(RuntimeError::StepLimitExceeded { limit: 1, pc: 1 }))
            .expect_err("runtime error must fail");
        assert!(matches!(error, CliError::Runtime(_)));
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn format_state_uses_snapshot_values() {
        let report = run_program(program(b"+++"), Vec::new(), None);
        assert_eq!(format_state(&report.snapshot), "state: pc=3 dp=0 steps=3\n");
    }

    #[test]
    fn format_state_reports_data_pointer_and_partial_progress() {
        let report = run_program(program(b">+"), Vec::new(), Some(2));
        assert_eq!(format_state(&report.snapshot), "state: pc=2 dp=1 steps=2\n");
    }
}
