//! Base64 のエンコード/デコード。デコード前に長さから上限を検査し、巨大な確保をしない。

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

/// Base64 コーデックのエラー。サービス層で `ApiError` へ写す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// 復号後のバイト数が `max_bytes` を超える(復号前に長さから判定する)。
    TooLong { max_bytes: usize },
    /// 標準 Base64(パディングあり)として不正な文字列。
    InvalidBase64,
}

/// バイト列を標準 Base64(パディングあり)へエンコードする。
pub fn encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// 標準 Base64 の文字列を、復号後 `max_bytes` 以下のバイト列へデコードする。
pub fn decode(text: &str, max_bytes: usize) -> Result<Vec<u8>, CodecError> {
    // 復号後 max_bytes 以下に収まる最長の標準 Base64 文字列長(3 バイトで 4 文字、端数は 4 文字に切り上げ)。
    let max_text_len = max_bytes.div_ceil(3).saturating_mul(4);
    if text.len() > max_text_len {
        return Err(CodecError::TooLong { max_bytes });
    }
    let bytes = STANDARD
        .decode(text)
        .map_err(|_| CodecError::InvalidBase64)?;
    if bytes.len() > max_bytes {
        return Err(CodecError::TooLong { max_bytes });
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: usize = 1_048_576;

    #[test]
    fn encodes_known_vectors() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"Man"), "TWFu");
    }

    #[test]
    fn round_trips_empty_short_and_binary_bytes() {
        let binary: Vec<u8> = (0..=255u8).collect();
        for bytes in [
            Vec::new(),
            b"a".to_vec(),
            b"ab".to_vec(),
            b"abc".to_vec(),
            binary,
        ] {
            let text = encode(&bytes);
            assert_eq!(decode(&text, MIB).expect("valid"), bytes);
        }
    }

    #[test]
    fn round_trips_non_utf8_bytes_unchanged() {
        let bytes = vec![0xff, 0xfe, 0x00, 0x80, 0xc3];
        assert_eq!(decode(&encode(&bytes), MIB).expect("valid"), bytes);
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(decode("@@@@", MIB), Err(CodecError::InvalidBase64));
        assert_eq!(decode("TW Fu", MIB), Err(CodecError::InvalidBase64));
        assert_eq!(decode("TWFu\n", MIB), Err(CodecError::InvalidBase64));
    }

    #[test]
    fn rejects_bad_length_and_missing_padding() {
        assert_eq!(decode("TWF", MIB), Err(CodecError::InvalidBase64));
        assert_eq!(decode("Zg=", MIB), Err(CodecError::InvalidBase64));
    }

    #[test]
    fn rejects_non_canonical_trailing_bits() {
        // "Zh==" は "Zg==" と同じ 1 バイトに見えるが末尾ビットが非ゼロなので不正。
        assert_eq!(decode("Zh==", MIB), Err(CodecError::InvalidBase64));
    }

    #[test]
    fn length_check_runs_before_decoding() {
        // 不正文字を含む巨大な入力でも、先に長さで TooLong になる(確保・復号をしない)。
        let huge_invalid = "@".repeat(10_000);
        assert_eq!(
            decode(&huge_invalid, 3),
            Err(CodecError::TooLong { max_bytes: 3 })
        );
    }

    #[test]
    fn accepts_exactly_max_bytes_and_rejects_one_more() {
        let at_limit = vec![0x41u8; 3];
        assert_eq!(decode(&encode(&at_limit), 3).expect("at limit"), at_limit);

        let over_limit = vec![0x41u8; 4];
        assert_eq!(
            decode(&encode(&over_limit), 3),
            Err(CodecError::TooLong { max_bytes: 3 })
        );
    }

    #[test]
    fn one_mib_input_is_the_boundary() {
        let at_limit = vec![0x20u8; MIB];
        assert_eq!(
            decode(&encode(&at_limit), MIB).expect("1 MiB ok").len(),
            MIB
        );

        let over_limit = vec![0x20u8; MIB + 1];
        assert_eq!(
            decode(&encode(&over_limit), MIB),
            Err(CodecError::TooLong { max_bytes: MIB })
        );
    }

    #[test]
    fn zero_limit_accepts_only_empty_text() {
        assert_eq!(decode("", 0).expect("empty"), Vec::<u8>::new());
        assert_eq!(decode("AA==", 0), Err(CodecError::TooLong { max_bytes: 0 }));
    }
}
