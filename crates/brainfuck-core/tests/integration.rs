//! 結合テスト: 標準プログラム(Hello World とエコー)を `.bf` フィクスチャから読み込み、
//! 解析・完走実行・観測までの公開 API を通して確認する。
//!
//! フィクスチャのコメント部には改行や日本語が含まれるが、命令文字以外は無視されるため
//! 命令列には影響しない。改行コードが CRLF でもソースの意味は変わらないことも確認する。

mod common;

use brainfuck_core::{MachineConfig, StopReason};
use common::machine_from;

/// `Hello World!\n` を出力する標準プログラム。
const HELLO_SOURCE: &str = include_str!("fixtures/hello.bf");

/// 入力をそのまま出力に返すプログラム(`,[.,]`)。
const ECHO_SOURCE: &str = include_str!("fixtures/echo.bf");

/// 完走実行の命令数上限。Hello World は数千命令以内で停止するため十分な余裕があり、
/// 解析の誤りで無限ループしても有限時間で失敗する。
const STEP_LIMIT: u64 = 100_000;

#[test]
fn hello_world_prints_expected_text_and_halts_normally() {
    let mut machine = machine_from(HELLO_SOURCE, b"", MachineConfig::with_max_steps(STEP_LIMIT));

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), b"Hello World!\n");
    assert!(machine.is_halted());
}

#[test]
fn hello_world_behaves_identically_with_crlf_line_endings() {
    // Windows の改行コード変換を模して、すべての LF を CRLF に置き換えたソースで実行する。
    let crlf_source = HELLO_SOURCE.replace('\n', "\r\n");
    assert_ne!(
        crlf_source, HELLO_SOURCE,
        "変換前後のソースは異なる必要がある"
    );
    let mut machine = machine_from(&crlf_source, b"", MachineConfig::with_max_steps(STEP_LIMIT));

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), b"Hello World!\n");
}

#[test]
fn echo_returns_all_nonzero_bytes_including_0xff_unchanged() {
    // 0x00 を除く全バイト値(0x01 から 0xFF まで)を入力に与える。0xFF と 0x80 以上を含む。
    let input: Vec<u8> = (1..=255u8).collect();
    let mut machine = machine_from(
        ECHO_SOURCE,
        &input,
        MachineConfig::with_max_steps(STEP_LIMIT),
    );

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), input.as_slice());
    assert_eq!(machine.input_position(), input.len());
    assert!(machine.is_halted());
}

#[test]
fn echo_stops_at_eof_when_cell_becomes_zero() {
    // 入力を使い切った後の `,` で現在セルに 0 が入り、ループを抜けて停止する。
    let input = [0xFFu8, b'A'];
    let mut machine = machine_from(
        ECHO_SOURCE,
        &input,
        MachineConfig::with_max_steps(STEP_LIMIT),
    );

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), &input);
    assert_eq!(machine.snapshot().current_cell, 0);
    assert!(machine.is_halted());
}

#[test]
fn echo_with_empty_input_halts_without_output() {
    // 入力が空の場合、最初の `,` で 0 が入りループは一度も回らない。
    let mut machine = machine_from(ECHO_SOURCE, b"", MachineConfig::with_max_steps(STEP_LIMIT));

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), b"");
    assert!(machine.is_halted());
}

#[test]
fn echo_treats_input_zero_byte_as_loop_exit() {
    // `,[.,]` ではループ条件のセルが 0 になった時点で抜けるため、入力中の 0x00 は
    // その位置でループを終える。以降のバイトは読まれず、出力されない。
    let input = [b'A', 0x00, b'B'];
    let mut machine = machine_from(
        ECHO_SOURCE,
        &input,
        MachineConfig::with_max_steps(STEP_LIMIT),
    );

    assert_eq!(machine.run(), Ok(StopReason::Halted));
    assert_eq!(machine.output(), b"A");
    assert_eq!(machine.input_position(), 2);
    assert!(machine.is_halted());
}
