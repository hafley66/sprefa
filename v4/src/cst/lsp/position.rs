//! UTF-16 ↔ byte conversion for LSP `Position` <-> source byte offset.
//! Ported from v3/crates/server/src/position.rs.

use lsp_types::Position;

/// Convert an LSP `Position` (UTF-16 code units within a line) into a byte
/// offset in `text`. Returns None if the position is past the end of the
/// document.
pub fn position_to_byte(text: &str, pos: Position) -> Option<usize> {
    let mut byte = 0usize;
    let mut line = 0u32;

    // Walk to the start of the requested line.
    while line < pos.line {
        match text[byte..].find('\n') {
            Some(rel) => {
                byte += rel + 1;
                line += 1;
            }
            None => return None,
        }
    }

    // Within the line, count UTF-16 code units.
    let line_end = text[byte..]
        .find('\n')
        .map(|n| byte + n)
        .unwrap_or(text.len());
    let line_slice = &text[byte..line_end];
    let mut units: u32 = 0;
    for (offset, ch) in line_slice.char_indices() {
        if units == pos.character {
            return Some(byte + offset);
        }
        units += ch.len_utf16() as u32;
    }
    if units == pos.character {
        Some(line_end)
    } else {
        None
    }
}

/// Convert a byte offset within `text` into an LSP `Position`.
pub fn byte_to_position(text: &str, byte: usize) -> Position {
    let byte = byte.min(text.len());
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    for (i, c) in text.char_indices() {
        if i >= byte {
            break;
        }
        if c == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let mut units: u32 = 0;
    for (offset, ch) in text[line_start..].char_indices() {
        if line_start + offset >= byte {
            break;
        }
        units += ch.len_utf16() as u32;
    }
    Position {
        line,
        character: units,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ascii() {
        let s = "hello\nworld\n";
        for byte in 0..s.len() {
            let pos = byte_to_position(s, byte);
            let back = position_to_byte(s, pos).unwrap();
            assert_eq!(
                back, byte,
                "byte {byte} pos {pos:?} round-tripped to {back}"
            );
        }
    }

    #[test]
    fn utf16_supplementary() {
        // 🦀 is 4 bytes UTF-8, 2 UTF-16 code units (surrogate pair).
        let s = "x🦀y";
        let pos_at_y = byte_to_position(s, 5); // 1 + 4 = 5
        assert_eq!(
            pos_at_y,
            Position {
                line: 0,
                character: 3
            }
        );
        let back = position_to_byte(s, pos_at_y).unwrap();
        assert_eq!(back, 5);
    }
}
