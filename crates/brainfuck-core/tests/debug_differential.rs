//! core の `Machine::run()` との差分テスト(タスク 4.2、要件 1.8・4.10・11.4)。
//!
//! オラクルは `Machine::run()` である。同じプログラム・入力・実行設定に対し、
//! ブレークポイントなし・キャンセルなし(`NeverCancel`)の `DebugSession::run_batch` を
//! 様々なバッチ命令数で繰り返し呼び出し、最終的な出力・テープ・`pc`・`dp`・
//! 実行済み命令数・停止理由またはエラー種別が `run()` と完全に一致することを確認する。
//! バッチ命令数は 1 / 3 / 7 / 1,000 / プログラム全体以上(実行数ちょうどと、それより大きい値)を用いる。

mod debug_common;

use brainfuck_core::{
    BatchSize, DebugSession, Machine, MachineConfig, NeverCancel, RuntimeError, StopReason,
};
use debug_common::{assert_same_final_state, machine_from, session_from};

/// 固定のバッチ命令数(素数の 3 / 7 を含む)。
const FIXED_BATCH_SIZES: [u64; 4] = [1, 3, 7, 1_000];

/// バッチ反復の打ち切り上限。差分テストのプログラムはすべて有限回で終わるため、到達は不具合を意味する。
const MAX_BATCH_CALLS: usize = 100_000;

/// Hello World(`Hello World!\n` を出力する標準的な Brainfuck プログラム)。
const HELLO_WORLD: &str = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";

/// ネストしたループ(外側・内側・最内の 3 段)で数値を作って出力する。
const NESTED_LOOPS: &str = "+++++[>+++++[>+++++<-]<-]>>.";

/// 入力を 3 バイト読んで順に出力する。入力には 0x00 と 0xFF を含む。
const ECHO_THREE: &str = ",.,.,.";

/// 入力を、0 を読むまで 1 バイトずつ反響させる(0x00 で終わる入力を用いる)。
const ECHO_LOOP: &str = ",[.,]";

/// 出力の後にポインタを範囲外(左端より左)へ動かし、`PointerOutOfBounds` で終わる。
const POINTER_ERROR: &str = "+++.<";

/// 無限ループ。小さなステップ上限で `StepLimitExceeded` により終わる。
const ENDLESS: &str = "+[]";

/// 出力を含む有限のループ。小さなステップ上限で途中の `StepLimitExceeded` により終わる。
const LOOP_THEN_OUTPUT: &str = "++[>+<-]>.";

/// 4 命令で正常終了する。ステップ上限をちょうど 4 にすると、上限到達後も停止済みを優先して `Halted` になる。
const FOUR_INSTRUCTIONS: &str = "++++";

/// バッチ反復の結果。最終停止理由またはエラーと、全呼び出しで実行した命令数の合計。
type Outcome = (Result<StopReason, RuntimeError>, u64);

/// `StepLimitReached` の間バッチを繰り返し呼び、最終結果(停止理由またはエラー)と実行数の合計を返す。
///
/// キャンセル・ブレークポイントのないセッションでは `Breakpoint` / `Cancelled` は現れない。
/// 現れた場合は呼び出し元の比較で不一致として検出される。
fn run_in_batches(session: &mut DebugSession, size: BatchSize) -> Outcome {
    let mut total_executed: u64 = 0;
    for _ in 0..MAX_BATCH_CALLS {
        let report = session.run_batch(size, &NeverCancel);
        assert!(
            report.executed <= size.get(),
            "1 回のバッチで実行数がバッチ命令数を超えてはならない"
        );
        total_executed += report.executed;
        match report.stop {
            Ok(StopReason::StepLimitReached) => continue,
            stop => return (stop, total_executed),
        }
    }
    panic!("バッチ反復が上限 {MAX_BATCH_CALLS} 回を超えて終わらない");
}

/// オラクル(`run()`)とバッチ反復の一致を、すべての対象バッチ命令数で確認する。
///
/// 戻り値はオラクルの最終結果(検証の前提確認に用いる)。
fn assert_batches_match_run(
    name: &str,
    source: &str,
    input: &[u8],
    config: MachineConfig,
) -> Result<StopReason, RuntimeError> {
    let mut oracle: Machine = machine_from(source, input, config);
    let oracle_stop = oracle.run();
    let oracle_steps = oracle.snapshot().steps_executed;

    // プログラム全体ちょうど(最後のバッチで停止に達する)と、それより大きい値を追加する。
    let mut widths: Vec<u64> = FIXED_BATCH_SIZES.to_vec();
    widths.push(oracle_steps.max(1));
    widths.push(oracle_steps + 1);
    widths.push(oracle_steps + 1_000);

    for width in widths {
        let size = BatchSize::new(width).expect("テストのバッチ命令数は 1 以上");
        let mut session = session_from(source, input, config);

        let (stop, executed) = run_in_batches(&mut session, size);

        assert_eq!(
            stop, oracle_stop,
            "{name}: 停止理由またはエラー(幅 {width})"
        );
        assert_eq!(executed, oracle_steps, "{name}: 実行数の合計(幅 {width})");
        assert_same_final_state(session.machine(), &oracle);
        assert_eq!(
            session.snapshot(),
            oracle.snapshot(),
            "{name}: pc・dp・実行数(幅 {width})"
        );
        assert_eq!(
            session.machine().tape(),
            oracle.tape(),
            "{name}: テープ(幅 {width})"
        );
        assert_eq!(
            session.machine().output(),
            oracle.output(),
            "{name}: 出力(幅 {width})"
        );
    }

    oracle_stop
}

#[test]
fn hello_world_matches_run_for_every_batch_width() {
    let stop = assert_batches_match_run("hello", HELLO_WORLD, b"", MachineConfig::unlimited());
    assert_eq!(stop, Ok(StopReason::Halted));

    // 前提: オラクルの出力が期待どおりであること(差分が空振りでないことの確認)。
    let mut oracle = machine_from(HELLO_WORLD, b"", MachineConfig::unlimited());
    oracle.run().expect("Hello World は正常終了する");
    assert_eq!(oracle.output(), b"Hello World!\n");
}

#[test]
fn nested_loops_match_run_for_every_batch_width() {
    let stop = assert_batches_match_run("nested", NESTED_LOOPS, b"", MachineConfig::unlimited());
    assert_eq!(stop, Ok(StopReason::Halted));

    let mut oracle = machine_from(NESTED_LOOPS, b"", MachineConfig::unlimited());
    oracle.run().expect("ネストしたループは正常終了する");
    assert!(
        !oracle.output().is_empty(),
        "出力が空だと差分の検証力が落ちる"
    );
}

#[test]
fn input_echo_with_zero_and_ff_bytes_matches_run_for_every_batch_width() {
    let input: &[u8] = &[0x00, 0xFF, 0x41];
    let stop = assert_batches_match_run("echo3", ECHO_THREE, input, MachineConfig::unlimited());
    assert_eq!(stop, Ok(StopReason::Halted));

    let mut oracle = machine_from(ECHO_THREE, input, MachineConfig::unlimited());
    oracle.run().expect("入力エコーは正常終了する");
    assert_eq!(oracle.output(), &[0x00, 0xFF, 0x41][..]);
    assert_eq!(oracle.input_position(), 3);
}

#[test]
fn input_loop_echo_matches_run_for_every_batch_width() {
    // 0x00 で読み取りループが終わる。0xFF は出力・保存されるバイトとして含める。
    let input: &[u8] = &[0x41, 0xFF, 0x00];
    let stop = assert_batches_match_run("echo_loop", ECHO_LOOP, input, MachineConfig::unlimited());
    assert_eq!(stop, Ok(StopReason::Halted));

    let mut oracle = machine_from(ECHO_LOOP, input, MachineConfig::unlimited());
    oracle.run().expect("入力ループの反響は正常終了する");
    assert_eq!(oracle.output(), &[0x41, 0xFF][..]);
}

#[test]
fn pointer_out_of_bounds_error_matches_run_for_every_batch_width() {
    let stop = assert_batches_match_run(
        "pointer_error",
        POINTER_ERROR,
        b"",
        MachineConfig::unlimited(),
    );
    let error = stop.expect_err("範囲外の移動は実行エラーになる");
    assert!(
        matches!(error, RuntimeError::PointerOutOfBounds { pc: 4, dp: 0, .. }),
        "エラー種別と位置: {error:?}"
    );
}

#[test]
fn step_limit_error_on_endless_loop_matches_run_for_every_batch_width() {
    let config = MachineConfig::with_max_steps(10);
    let stop = assert_batches_match_run("endless_limit", ENDLESS, b"", config);
    assert!(
        matches!(stop, Err(RuntimeError::StepLimitExceeded { limit: 10, .. })),
        "上限超過のエラーになる: {stop:?}"
    );
}

#[test]
fn step_limit_error_inside_a_loop_with_output_matches_run_for_every_batch_width() {
    // 出力の直前で上限に達する設定。上限に達した時点の出力・テープが一致することを確認する。
    let config = MachineConfig::with_max_steps(9);
    let stop = assert_batches_match_run("loop_limit", LOOP_THEN_OUTPUT, b"", config);
    assert!(
        matches!(stop, Err(RuntimeError::StepLimitExceeded { limit: 9, .. })),
        "上限超過のエラーになる: {stop:?}"
    );
}

#[test]
fn step_limit_equal_to_program_length_halts_normally_for_every_batch_width() {
    // 停止済みは上限判定より先に評価されるため、上限と実行数が一致しても `Halted` で終わる。
    let config = MachineConfig::with_max_steps(4);
    let stop = assert_batches_match_run("exact_limit", FOUR_INSTRUCTIONS, b"", config);
    assert_eq!(stop, Ok(StopReason::Halted));
}
