//! LSP-style position ↔ byte offset translation.
//!
//! Copied verbatim from v2/src/position.rs (no behavioral change needed
//! for v3). LSP uses `{line, character}` where `character` counts
//! UTF-16 code units; the pipeline speaks in byte offsets.

/// Convert `(line, utf16_col)` to a byte offset into `text`.
pub fn position_to_offset(text: &str, line: u32, utf16_col: u32) -> usize {
    let mut cur_line = 0u32;
    let mut cur_col = 0u32;
    for (i, ch) in text.char_indices() {
        if cur_line == line {
            if cur_col >= utf16_col { return i; }
            let step = ch.len_utf16() as u32;
            if ch != '\n' && cur_col + step > utf16_col { return i; }
        }
        if ch == '\n' {
            if cur_line == line { return i; }
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += ch.len_utf16() as u32;
        }
    }
    text.len()
}

/// Convert a byte offset to `(line, utf16_col)`.
pub fn offset_to_position(text: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if i >= offset { return (line, col); }
        if ch == '\n' { line += 1; col = 0; }
        else { col += ch.len_utf16() as u32; }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_astral_and_clamps() {
        // ASCII roundtrip.
        let t = "abc\ndef";
        assert_eq!(position_to_offset(t, 0, 0), 0);
        assert_eq!(position_to_offset(t, 1, 0), 4);
        assert_eq!(offset_to_position(t, 4), (1, 0));

        // Astral: 🦀 is 4 bytes utf8, 2 utf16 units.
        let t = "x🦀y";
        assert_eq!(position_to_offset(t, 0, 1), 1);
        assert_eq!(position_to_offset(t, 0, 3), 5);
        assert_eq!(offset_to_position(t, 5), (0, 3));
        // Surrogate-interior rounds down to the char start.
        assert_eq!(position_to_offset(t, 0, 2), 1);

        // Past EOF / past line end.
        assert_eq!(position_to_offset("abc", 99, 0), 3);
        assert_eq!(position_to_offset("ab\ncd", 0, 99), 2);
        assert_eq!(offset_to_position("abc", 999), (0, 3));
    }
}
