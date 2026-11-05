//! ファイルバイト列の検査・デコード・改行コード補助(`source_file`)の契約。
//!
//! 対象は要件 2.8(保存は UTF-8・改行の書き戻し)、2.12(不正 UTF-8 の置換と通知)、
//! 2.13(1 MiB 超の拒否)、10.9(エラー文面に内容・パスを含めない)、11.7(改行コードは命令列に影響しない)、
//! 13.7(表示名はディレクトリを含まない)。I/O は行わない(ファイルの読み書きは Tauri 層の責務)。

use std::path::Path;

use brainfuck_gui_api::dto::ApiErrorKind;
use brainfuck_gui_api::limits::MAX_SOURCE_BYTES;
use brainfuck_gui_api::source_file::{decode_source, display_name, encode_source, LineEnding};

const MIB: usize = 1_048_576;

#[test]
fn limit_constant_is_one_mebibyte() {
    assert_eq!(MAX_SOURCE_BYTES, MIB);
}

// ---- デコード: UTF-8 と置換 ----

#[test]
fn valid_utf8_is_decoded_without_replacement() {
    let decoded = decode_source("+[あ]😀\n".as_bytes()).expect("valid utf-8 is accepted");
    assert_eq!(decoded.text, "+[あ]😀\n");
    assert!(!decoded.lossy, "valid input must not report replacement");
    assert_eq!(decoded.line_ending, LineEnding::Lf);
}

#[test]
fn invalid_utf8_bytes_are_replaced_and_reported() {
    // 0xFF は UTF-8 として不正。前後の ASCII はそのまま残る。
    let decoded = decode_source(&[b'a', 0xff, b'b']).expect("invalid utf-8 is still loaded");
    assert_eq!(decoded.text, "a\u{FFFD}b");
    assert!(decoded.lossy, "replacement must be reported");
}

#[test]
fn truncated_multibyte_sequence_is_replaced_and_reported() {
    // 「あ」(E3 81 82)の途中で切れている。
    let decoded = decode_source(&[b'+', 0xe3, 0x81]).expect("accepted");
    assert!(decoded.text.starts_with('+'));
    assert!(decoded.text.contains('\u{FFFD}'));
    assert!(decoded.lossy);
}

#[test]
fn empty_file_is_an_empty_lf_document() {
    let decoded = decode_source(&[]).expect("empty is valid");
    assert_eq!(decoded.text, "");
    assert!(!decoded.lossy);
    assert_eq!(decoded.line_ending, LineEnding::Lf);
}

// ---- デコード: 1 MiB の境界 ----

#[test]
fn exactly_one_mebibyte_is_accepted() {
    let bytes = vec![b'+'; MIB];
    let decoded = decode_source(&bytes).expect("1 MiB is within the limit");
    assert_eq!(decoded.text.len(), MIB);
}

#[test]
fn one_byte_over_one_mebibyte_is_rejected_with_file_too_large() {
    let bytes = vec![b'+'; MIB + 1];
    let error = decode_source(&bytes).expect_err("over 1 MiB must be rejected");
    assert_eq!(error.kind, ApiErrorKind::FileTooLarge);
    assert_eq!(error.field, None);
    assert_eq!(error.message, "file is too large");
}

// ---- デコード: 改行コードの検出と LF 正規化 ----

#[test]
fn crlf_is_detected_and_normalized_to_lf() {
    let decoded = decode_source(b"+\r\n-\r\n").expect("accepted");
    assert_eq!(decoded.line_ending, LineEnding::CrLf);
    assert_eq!(decoded.text, "+\n-\n");
}

#[test]
fn lone_cr_is_detected_and_normalized_to_lf() {
    let decoded = decode_source(b"+\r-\r").expect("accepted");
    assert_eq!(decoded.line_ending, LineEnding::Cr);
    assert_eq!(decoded.text, "+\n-\n");
}

#[test]
fn lf_is_detected_and_kept() {
    let decoded = decode_source(b"+\n-\n").expect("accepted");
    assert_eq!(decoded.line_ending, LineEnding::Lf);
    assert_eq!(decoded.text, "+\n-\n");
}

#[test]
fn first_line_ending_wins_when_mixed() {
    // 最初に現れた改行(CRLF)を採用し、後続の CR / LF はすべて LF へ正規化する。
    let decoded = decode_source(b"a\r\nb\rc\nd").expect("accepted");
    assert_eq!(decoded.line_ending, LineEnding::CrLf);
    assert_eq!(decoded.text, "a\nb\nc\nd");

    let decoded = decode_source(b"a\nb\r\nc").expect("accepted");
    assert_eq!(decoded.line_ending, LineEnding::Lf);
    assert_eq!(decoded.text, "a\nb\nc");
}

#[test]
fn file_without_line_breaks_defaults_to_lf() {
    let decoded = decode_source(b"+[-]").expect("accepted");
    assert_eq!(decoded.line_ending, LineEnding::Lf);
}

#[test]
fn crlf_pair_is_not_split_into_two_breaks() {
    // "\r\n" は 1 つの改行。"\r\r\n" は CR + CRLF の 2 つ。
    let decoded = decode_source(b"a\r\nb").expect("accepted");
    assert_eq!(decoded.text, "a\nb");
    let decoded = decode_source(b"a\r\r\nb").expect("accepted");
    assert_eq!(decoded.text, "a\n\nb");
}

// ---- 保存: LF を元の改行コードへ戻す ----

#[test]
fn encode_writes_the_recorded_line_endings() {
    assert_eq!(
        encode_source("+\n-\n", LineEnding::Lf).expect("ok"),
        b"+\n-\n".to_vec()
    );
    assert_eq!(
        encode_source("+\n-\n", LineEnding::CrLf).expect("ok"),
        b"+\r\n-\r\n".to_vec()
    );
    assert_eq!(
        encode_source("+\n-\n", LineEnding::Cr).expect("ok"),
        b"+\r-\r".to_vec()
    );
}

#[test]
fn encode_writes_utf8_for_multibyte_text() {
    let bytes = encode_source("あ\n😀", LineEnding::CrLf).expect("ok");
    assert_eq!(bytes, "あ\r\n😀".as_bytes().to_vec());
}

#[test]
fn round_trip_preserves_bytes_for_each_line_ending() {
    let cases: [(&[u8], LineEnding); 4] = [
        (b"+[>,.<-]\n", LineEnding::Lf),
        ("あ\r\n+\r\n😀\r\n".as_bytes(), LineEnding::CrLf),
        (b"+\r-\r\rx\r", LineEnding::Cr),
        (b"", LineEnding::Lf),
    ];
    for (original, expected_ending) in cases {
        let decoded = decode_source(original).expect("decode");
        assert_eq!(decoded.line_ending, expected_ending);
        let saved = encode_source(&decoded.text, decoded.line_ending).expect("encode");
        assert_eq!(saved, original.to_vec(), "round trip must restore bytes");
    }
}

#[test]
fn encode_accepts_exactly_one_mebibyte_of_text() {
    let text = "+".repeat(MIB);
    let bytes = encode_source(&text, LineEnding::Lf).expect("1 MiB is within the limit");
    assert_eq!(bytes.len(), MIB);
}

#[test]
fn encode_rejects_text_over_one_mebibyte_with_source_too_large() {
    let text = "+".repeat(MIB + 1);
    let error = encode_source(&text, LineEnding::Lf).expect_err("over 1 MiB must be rejected");
    assert_eq!(error.kind, ApiErrorKind::SourceTooLarge);
    assert_eq!(error.field, None);
}

#[test]
fn encode_rejects_output_that_would_not_be_reopenable() {
    // 本文は 1 MiB ちょうどでも、CRLF へ変換すると 1 MiB を超えるため保存しない
    // (保存したファイルが次回の読み込みで拒否されないようにする)。
    let mut text = "+".repeat(MIB - 1);
    text.push('\n');
    assert_eq!(text.len(), MIB);
    let error = encode_source(&text, LineEnding::CrLf).expect_err("expanded output over 1 MiB");
    assert_eq!(error.kind, ApiErrorKind::SourceTooLarge);
}

// ---- エラー文面に内容・パスを含めない(10.9) ----

#[test]
fn errors_do_not_echo_file_content() {
    let secret = "SECRET-CONTENT-".repeat(100_000);
    let error = decode_source(secret.as_bytes()).expect_err("too large");
    assert!(!error.message.contains("SECRET"));
    assert!(error.field.is_none());
}

// ---- 表示名(13.7) ----

#[test]
fn display_name_drops_the_directory_part() {
    assert_eq!(display_name(Path::new("dir/sub/hello.bf")), "hello.bf");
    assert_eq!(display_name(Path::new("hello.bf")), "hello.bf");
    assert_eq!(
        display_name(Path::new("dir/sub/日本語 プログラム.bf")),
        "日本語 プログラム.bf"
    );
}

#[test]
fn display_name_never_contains_a_separator() {
    let name = display_name(Path::new("a/b\\c/d.txt"));
    assert!(!name.contains('/'));
    assert!(!name.contains('\\'));
}

#[test]
fn display_name_of_a_path_without_file_name_is_empty() {
    assert_eq!(display_name(Path::new("/")), "");
}
