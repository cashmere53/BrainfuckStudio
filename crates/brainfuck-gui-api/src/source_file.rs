//! ファイルのバイト列の検査・デコード・改行コード補助(I/O は行わない)。
//!
//! - 読み込み: 1 MiB 超を拒否し、不正な UTF-8 を置換文字へ置き換えて置換の有無を返す(要件 2.12, 2.13)。
//!   最初に現れた改行コードを記録し、本文は LF に正規化する。
//! - 保存: 本文の LF を記録した改行コードへ戻す。1 MiB 超を拒否する(要件 2.8)。
//! - 表示名: パスからディレクトリを除いたファイル名だけを取り出す(要件 13.7)。
//!
//! エラー文面にはパス・内容を含めない(要件 10.9)。`ApiError::new` の固定文面だけを使う。

use std::borrow::Cow;
use std::path::Path;

use crate::dto::{ApiError, ApiErrorKind};
use crate::limits::MAX_SOURCE_BYTES;

/// ファイルの改行コード。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// LF(`\n`)。改行が無いファイルの既定値でもある。
    Lf,
    /// CRLF(`\r\n`)。
    CrLf,
    /// CR(`\r`)。
    Cr,
}

/// 読み込んだファイルのデコード結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedSource {
    /// 置換つき UTF-8 として読んだ本文(改行は LF に正規化済み)。
    pub text: String,
    /// 不正な UTF-8 を置換文字へ置き換えたかどうか。
    pub lossy: bool,
    /// 最初に現れた改行コード(改行が無ければ LF)。
    pub line_ending: LineEnding,
}

/// ファイルのバイト列を読み込み用の文字列へデコードする。
///
/// 1 MiB(`MAX_SOURCE_BYTES`)を超えるバイト列は `fileTooLarge` で拒否する。
pub fn decode_source(bytes: &[u8]) -> Result<DecodedSource, ApiError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(ApiError::new(ApiErrorKind::FileTooLarge, None));
    }
    let decoded = String::from_utf8_lossy(bytes);
    // `from_utf8_lossy` は不正な列があるときに限り所有値(置換済み)を返す。
    let lossy = matches!(decoded, Cow::Owned(_));
    let line_ending = detect_line_ending(&decoded);
    let text = normalize_to_lf(&decoded);
    Ok(DecodedSource {
        text,
        lossy,
        line_ending,
    })
}

/// 本文(LF 正規化済み)を、記録した改行コードへ戻して UTF-8 のバイト列にする。
///
/// `text` の改行は LF であることを前提とし、LF のみを `ending` へ置き換える。
/// 本文が 1 MiB を超える場合、または改行の変換後の出力が 1 MiB を超える場合は
/// `sourceTooLarge` で拒否する(保存したファイルが次回の読み込みで拒否されないようにするため)。
pub fn encode_source(text: &str, ending: LineEnding) -> Result<Vec<u8>, ApiError> {
    if text.len() > MAX_SOURCE_BYTES {
        return Err(ApiError::new(ApiErrorKind::SourceTooLarge, None));
    }
    let separator = match ending {
        LineEnding::Lf => "\n",
        LineEnding::CrLf => "\r\n",
        LineEnding::Cr => "\r",
    };
    let output = text.replace('\n', separator);
    if output.len() > MAX_SOURCE_BYTES {
        return Err(ApiError::new(ApiErrorKind::SourceTooLarge, None));
    }
    Ok(output.into_bytes())
}

/// パスからファイル名(ディレクトリを含まない部分)を取り出す。ファイル名が無いパスでは空文字を返す。
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 最初に現れた改行コードを返す。改行が無ければ LF。
fn detect_line_ending(text: &str) -> LineEnding {
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'\n' => return LineEnding::Lf,
            b'\r' => {
                return if bytes.get(index + 1) == Some(&b'\n') {
                    LineEnding::CrLf
                } else {
                    LineEnding::Cr
                };
            }
            _ => {}
        }
    }
    LineEnding::Lf
}

/// CRLF と単独の CR をそれぞれ 1 個の LF へ置き換える。
fn normalize_to_lf(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            output.push('\n');
        } else {
            output.push(ch);
        }
    }
    output
}
