//! LSP-style position ↔ byte offset translation.
//!
//! LSP protocol uses `{line, character}` where `character` counts UTF-16 code
//! units (surrogate pair = 2 units). Our pipeline speaks in byte offsets.
//! These functions bridge the two without external deps.

/// Convert `(line, utf16_col)` to a byte offset into `text`.
///
/// - Column past the line end returns the position at the line's newline.
/// - Position past EOF returns `text.len()`.
/// - If the column lands inside a surrogate pair, rounds down to the char start.
pub fn position_to_offset(text: &str, line: u32, utf16_col: u32) -> usize {
    let mut cur_line = 0u32;
    let mut cur_col  = 0u32;
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
    let mut col  = 0u32;
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
    fn ascii_roundtrip() {
        let t = "abc\ndef";
        assert_eq!(position_to_offset(t, 0, 0), 0);
        assert_eq!(position_to_offset(t, 0, 3), 3);
        assert_eq!(position_to_offset(t, 1, 0), 4);
        assert_eq!(position_to_offset(t, 1, 2), 6);
        assert_eq!(offset_to_position(t, 0), (0, 0));
        assert_eq!(offset_to_position(t, 4), (1, 0));
        assert_eq!(offset_to_position(t, 6), (1, 2));
    }

    #[test]
    fn em_dash_is_one_utf16_unit() {
        // `──` = two U+2500 box-drawing chars, 3 bytes each, 1 utf16 unit each.
        let t = "# ── File\n";
        // col 4 (after "# ──") should land at byte 8 (# + space + 2x 3-byte)
        assert_eq!(position_to_offset(t, 0, 4), 8);
        assert_eq!(offset_to_position(t, 8), (0, 4));
    }

    #[test]
    fn astral_is_two_utf16_units() {
        // 🦀 = U+1F980, 4 bytes utf8, 2 utf16 code units (surrogate pair).
        let t = "x🦀y";
        assert_eq!(position_to_offset(t, 0, 0), 0);  // before x
        assert_eq!(position_to_offset(t, 0, 1), 1);  // before crab
        assert_eq!(position_to_offset(t, 0, 3), 5);  // before y (1 + 4)
        assert_eq!(offset_to_position(t, 1), (0, 1));
        assert_eq!(offset_to_position(t, 5), (0, 3));
    }

    #[test]
    fn column_inside_surrogate_rounds_down() {
        // utf16 col 2 lands in the middle of the crab; return the crab's start.
        let t = "x🦀y";
        assert_eq!(position_to_offset(t, 0, 2), 1);
    }

    #[test]
    fn past_eof_clamps() {
        let t = "abc";
        assert_eq!(position_to_offset(t, 99, 0), 3);
        assert_eq!(offset_to_position(t, 999), (0, 3));
    }

    #[test]
    fn col_past_line_end_stops_at_newline() {
        let t = "ab\ncd";
        assert_eq!(position_to_offset(t, 0, 99), 2);
    }
}
