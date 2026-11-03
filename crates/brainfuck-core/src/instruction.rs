//! Brainfuck の 8 命令を表す型と、ソース文字との相互変換。

/// Brainfuck の命令。ソース上の 8 文字それぞれに 1 対 1 で対応する。
///
/// ソース位置は保持しない。位置は `Program` が並列配列で管理する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instruction {
    /// `>`: データポインタを右へ移動する。
    MoveRight,
    /// `<`: データポインタを左へ移動する。
    MoveLeft,
    /// `+`: 現在セルを 1 増やす(255 の次は 0)。
    Increment,
    /// `-`: 現在セルを 1 減らす(0 の前は 255)。
    Decrement,
    /// `.`: 現在セルの値を 1 バイトとして出力する。
    Output,
    /// `,`: 1 バイトを入力して現在セルへ格納する。
    Input,
    /// `[`: 現在セルが 0 なら対応する `]` の次へ進む。
    LoopStart,
    /// `]`: 現在セルが 0 でなければ対応する `[` の次へ戻る。
    LoopEnd,
}

impl Instruction {
    /// ASCII バイトを命令へ変換する。8 文字以外は `None`(コメント)。
    pub fn from_byte(byte: u8) -> Option<Instruction> {
        match byte {
            b'>' => Some(Instruction::MoveRight),
            b'<' => Some(Instruction::MoveLeft),
            b'+' => Some(Instruction::Increment),
            b'-' => Some(Instruction::Decrement),
            b'.' => Some(Instruction::Output),
            b',' => Some(Instruction::Input),
            b'[' => Some(Instruction::LoopStart),
            b']' => Some(Instruction::LoopEnd),
            _ => None,
        }
    }

    /// 対応するソース文字(`>` `<` `+` `-` `.` `,` `[` `]`)を返す。
    pub fn as_char(self) -> char {
        match self {
            Instruction::MoveRight => '>',
            Instruction::MoveLeft => '<',
            Instruction::Increment => '+',
            Instruction::Decrement => '-',
            Instruction::Output => '.',
            Instruction::Input => ',',
            Instruction::LoopStart => '[',
            Instruction::LoopEnd => ']',
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Instruction;
    use std::collections::HashSet;

    /// 命令と対応するソース文字の組(8 件)。
    const PAIRS: [(u8, Instruction); 8] = [
        (b'>', Instruction::MoveRight),
        (b'<', Instruction::MoveLeft),
        (b'+', Instruction::Increment),
        (b'-', Instruction::Decrement),
        (b'.', Instruction::Output),
        (b',', Instruction::Input),
        (b'[', Instruction::LoopStart),
        (b']', Instruction::LoopEnd),
    ];

    #[test]
    fn each_of_the_eight_characters_maps_to_its_instruction() {
        for (byte, expected) in PAIRS {
            assert_eq!(
                Instruction::from_byte(byte),
                Some(expected),
                "byte {byte:#04x}"
            );
        }
    }

    #[test]
    fn as_char_returns_the_original_source_character() {
        for (byte, instruction) in PAIRS {
            assert_eq!(
                instruction.as_char(),
                byte as char,
                "instruction {instruction:?}"
            );
        }
    }

    #[test]
    fn round_trip_over_eight_characters_restores_the_original() {
        for (byte, _) in PAIRS {
            let instruction = Instruction::from_byte(byte).expect("8 文字は命令になる");
            assert_eq!(instruction.as_char() as u32, byte as u32);
            assert_eq!(
                Instruction::from_byte(instruction.as_char() as u8),
                Some(instruction)
            );
        }
    }

    #[test]
    fn every_byte_value_is_either_one_of_the_eight_or_none() {
        let valid: Vec<u8> = PAIRS.iter().map(|(byte, _)| *byte).collect();
        for byte in 0u8..=255 {
            let result = Instruction::from_byte(byte);
            if valid.contains(&byte) {
                let instruction = result.expect("8 文字は命令になる");
                assert_eq!(instruction.as_char() as u32, byte as u32);
            } else {
                assert_eq!(result, None, "byte {byte:#04x} must not be an instruction");
            }
        }
    }

    #[test]
    fn non_instruction_ascii_bytes_are_none() {
        // 0x00 / 制御文字 / 記号 / 数字 / 英字の境界値 / DEL
        let samples: [u8; 36] = [
            0x00, 0x09, 0x0A, 0x0D, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29,
            0x2A, 0x2F, 0x30, 0x39, 0x3A, 0x3B, 0x3D, 0x3F, 0x40, 0x41, 0x5A, 0x5C, 0x5E, 0x5F,
            0x60, 0x61, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F,
        ];
        for byte in samples {
            assert_eq!(Instruction::from_byte(byte), None, "byte {byte:#04x}");
        }
    }

    #[test]
    fn letters_and_whitespace_are_none() {
        for byte in (b'a'..=b'z').chain(b'A'..=b'Z').chain(*b" \t\n\r") {
            assert_eq!(Instruction::from_byte(byte), None, "byte {byte:#04x}");
        }
    }

    #[test]
    fn all_non_ascii_bytes_from_0x80_to_0xff_are_none() {
        for byte in 0x80u8..=0xFF {
            assert_eq!(Instruction::from_byte(byte), None, "byte {byte:#04x}");
        }
    }

    #[test]
    fn utf8_multibyte_lead_and_continuation_bytes_are_none() {
        // 「あ」(E3 81 82)と「\u{FF0B}」(全角プラス EF BC 8B)の各バイト
        for byte in [0xE3u8, 0x81, 0x82, 0xEF, 0xBC, 0x8B] {
            assert_eq!(Instruction::from_byte(byte), None, "byte {byte:#04x}");
        }
    }

    #[test]
    fn variants_are_distinct_and_hashable() {
        let set: HashSet<Instruction> = PAIRS.iter().map(|(_, instruction)| *instruction).collect();
        assert_eq!(set.len(), 8);
    }

    #[test]
    fn instruction_is_copy_and_comparable() {
        let a = Instruction::Increment;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(Instruction::Increment, Instruction::Decrement);
    }
}
