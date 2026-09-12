//! Bare-token classification, character classes and string escapes.
//! Every predicate is a literal port of the same-named one in v7 `0_parser.pl`.

pub fn term_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '{' | '}' | ';' | '"')
}

pub fn ascii_alpha(c: char) -> bool {
    c.is_ascii_alphabetic()
}

pub fn decimal_digit(c: char) -> bool {
    c.is_ascii_digit()
}

pub fn identifier_rest_char(c: char) -> bool {
    ascii_alpha(c) || decimal_digit(c) || c == '_' || c == '-' || c == '.'
}

pub fn valid_identifier(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some(first) if ascii_alpha(first) || first == '_' => chars.all(identifier_rest_char),
        _ => false,
    }
}

pub fn valid_atom(token: &str) -> bool {
    if matches!(token, ":" | "*" | "+" | "->" | "<-" | "<+") {
        return true;
    }
    if let Some(name) = token.strip_suffix(':') {
        if valid_identifier(name) {
            return true;
        }
    }
    valid_identifier(token)
}

pub fn integer_token(token: &str) -> bool {
    let digits = token.strip_prefix('-').unwrap_or(token);
    !digits.is_empty() && digits.chars().all(decimal_digit)
}

/// v7 `decoded_escape/2`: an unknown escape keeps both characters.
pub fn decoded_escape(escape: char, out: &mut String) {
    match escape {
        'n' => out.push('\n'),
        't' => out.push('\t'),
        'r' => out.push('\r'),
        '\\' => out.push('\\'),
        '"' => out.push('"'),
        other => {
            out.push('\\');
            out.push(other);
        }
    }
}
