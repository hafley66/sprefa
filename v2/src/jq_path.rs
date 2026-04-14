use std::sync::Arc;

pub enum PathSeg {
    Key(Arc<str>),
    Index(u32),
}

fn is_ident_safe(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn escape_key(buf: &mut String, key: &str) {
    buf.push_str("[\"");
    for c in key.chars() {
        match c {
            '\\' => buf.push_str("\\\\"),
            '"' => buf.push_str("\\\""),
            '\n' => buf.push_str("\\n"),
            '\t' => buf.push_str("\\t"),
            '\r' => buf.push_str("\\r"),
            c if (c as u32) < 0x20 => buf.push_str(&format!("\\u{:04x}", c as u32)),
            c => buf.push(c),
        }
    }
    buf.push_str("\"]");
}

/// Append a key segment to an existing path string.
/// If `buf` is empty this seeds a new path (`.foo` or `.["x"]`).
pub fn push_key(buf: &mut String, key: &str) {
    if is_ident_safe(key) {
        buf.push('.');
        buf.push_str(key);
    } else {
        // Bracket form: jq uses `.["x"]` at path start, `["x"]` after existing content.
        if buf.is_empty() {
            buf.push('.');
        }
        escape_key(buf, key);
    }
}

/// Append an index segment to an existing path string.
/// If `buf` is empty this seeds with `.` so the result reads `.[N]`.
pub fn push_index(buf: &mut String, idx: u32) {
    if buf.is_empty() {
        buf.push('.');
    }
    buf.push('[');
    buf.push_str(&idx.to_string());
    buf.push(']');
}

pub fn render(segs: &[PathSeg]) -> String {
    if segs.is_empty() {
        return ".".to_string();
    }
    let mut buf = String::new();
    for seg in segs {
        match seg {
            PathSeg::Key(k) => push_key(&mut buf, k),
            PathSeg::Index(i) => push_index(&mut buf, *i),
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> PathSeg {
        PathSeg::Key(Arc::from(s))
    }
    fn idx(n: u32) -> PathSeg {
        PathSeg::Index(n)
    }

    #[test]
    fn empty_path() {
        assert_eq!(render(&[]), ".");
    }

    #[test]
    fn single_ident_key() {
        assert_eq!(render(&[key("foo")]), ".foo");
    }

    #[test]
    fn chained_ident_keys() {
        assert_eq!(render(&[key("a"), key("b"), key("c")]), ".a.b.c");
    }

    #[test]
    fn index_at_root() {
        assert_eq!(render(&[idx(0)]), ".[0]");
    }

    #[test]
    fn index_in_middle() {
        assert_eq!(
            render(&[key("users"), idx(0), key("name")]),
            ".users[0].name"
        );
    }

    #[test]
    fn index_at_end() {
        assert_eq!(render(&[key("a"), idx(3)]), ".a[3]");
    }

    #[test]
    fn consecutive_indices() {
        assert_eq!(render(&[key("a"), idx(3), idx(1)]), ".a[3][1]");
    }

    #[test]
    fn key_needing_quote_space() {
        assert_eq!(render(&[key("weird key")]), ".[\"weird key\"]");
    }

    #[test]
    fn key_needing_quote_at() {
        assert_eq!(render(&[key("@scope/pkg")]), ".[\"@scope/pkg\"]");
    }

    #[test]
    fn key_needing_quote_dot() {
        assert_eq!(render(&[key("a.b")]), ".[\"a.b\"]");
    }

    #[test]
    fn key_needing_quote_leading_digit() {
        assert_eq!(render(&[key("1abc")]), ".[\"1abc\"]");
    }

    #[test]
    fn key_with_backslash_and_quote() {
        assert_eq!(render(&[key("a\\\"b")]), ".[\"a\\\\\\\"b\"]");
    }

    #[test]
    fn key_with_newline() {
        assert_eq!(render(&[key("li\nne")]), ".[\"li\\nne\"]");
    }

    #[test]
    fn underscore_key_is_ident_safe() {
        assert_eq!(render(&[key("_private")]), "._private");
    }

    #[test]
    fn push_key_matches_render_ident() {
        let mut buf = String::new();
        push_key(&mut buf, "foo");
        push_key(&mut buf, "bar");
        assert_eq!(buf, render(&[key("foo"), key("bar")]));
    }

    #[test]
    fn push_key_matches_render_quoted() {
        let mut buf = String::new();
        push_key(&mut buf, "@scope/pkg");
        assert_eq!(buf, render(&[key("@scope/pkg")]));
    }

    #[test]
    fn push_index_matches_render() {
        let mut buf = String::new();
        push_index(&mut buf, 5);
        push_index(&mut buf, 2);
        assert_eq!(buf, render(&[idx(5), idx(2)]));
    }

    #[test]
    fn key_with_tab() {
        assert_eq!(render(&[key("a\tb")]), ".[\"a\\tb\"]");
    }

    #[test]
    fn key_with_carriage_return() {
        assert_eq!(render(&[key("a\rb")]), ".[\"a\\rb\"]");
    }

    #[test]
    fn key_with_control_char() {
        assert_eq!(render(&[key("a\x07b")]), ".[\"a\\u0007b\"]");
    }

    #[test]
    fn non_ascii_key_takes_bracket_form() {
        assert_eq!(render(&[key("日本")]), ".[\"日本\"]");
    }

    #[test]
    fn empty_string_key() {
        assert_eq!(render(&[key("")]), ".[\"\"]");
    }

    #[test]
    fn push_mixed_matches_render() {
        let mut buf = String::new();
        push_key(&mut buf, "users");
        push_index(&mut buf, 0);
        push_key(&mut buf, "name");
        assert_eq!(buf, render(&[key("users"), idx(0), key("name")]));
    }
}
