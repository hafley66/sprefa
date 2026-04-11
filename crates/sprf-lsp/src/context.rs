//! Token scanner and heuristic context detection for partial sprefa syntax.
//!
//! Provides lightweight tokenization for LSP autocomplete at cursor positions
//! with potentially incomplete/in-progress code.

use std::fmt;

/// Token types for sprefa syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Known tag identifier (fs, json, repo, rev, etc.)
    Tag(String),
    /// Opening parenthesis `(`
    LParen,
    /// Closing parenthesis `)`
    RParen,
    /// Opening brace `{`
    LBrace,
    /// Closing brace `}`
    RBrace,
    /// String literal (handles unclosed strings for partial syntax)
    String(String),
    /// Identifier (variable names, captures, etc.)
    Ident(String),
    /// Raw content inside parens (for partial syntax detection)
    Raw(String),
    /// Dollar sign `$` (for captures)
    Dollar,
    /// Greater-than sign `>` (chain operator)
    Gt,
    /// Semicolon `;`
    Semi,
    /// Comma `,`
    Comma,
    /// End of file/input
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Tag(s) => write!(f, "Tag({})", s),
            Token::LParen => write!(f, "LParen"),
            Token::RParen => write!(f, "RParen"),
            Token::LBrace => write!(f, "LBrace"),
            Token::RBrace => write!(f, "RBrace"),
            Token::String(s) => write!(f, "String({})", s),
            Token::Ident(s) => write!(f, "Ident({})", s),
            Token::Raw(s) => write!(f, "Raw({})", s),
            Token::Dollar => write!(f, "Dollar"),
            Token::Gt => write!(f, "Gt"),
            Token::Semi => write!(f, "Semi"),
            Token::Comma => write!(f, "Comma"),
            Token::Eof => write!(f, "Eof"),
        }
    }
}

/// Token with position information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSpan {
    pub token: Token,
    pub start: usize,
    pub end: usize,
}

/// Detected context for autocomplete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Context {
    /// Inside a tag's argument parentheses: `tag(partial`
    InsideTag { tag: String, partial: String },
    /// Inside a repo/rev scoped block, possibly typing revision.
    /// Pattern: `repo(x) { ... rev(y` detects repo="x", partial="y"
    InsideRepoRev { repo: String, partial: String },
    /// Typing a capture variable after `$`
    InsideCapture,
    /// No specific context detected.
    Unknown,
}

/// Check if a word is a known tag.
fn is_known_tag(word: &str) -> bool {
    matches!(
        word,
        "json"
            | "line"
            | "ast"
            | "repo"
            | "rev"
            | "branch"
            | "tag"
            | "repo.norm"
            | "rev.norm"
            | "branch.norm"
            | "tag.norm"
            | "folder"
            | "file"
            | "fs"
            | "marker"
            | "md"
            | "rule"
            | "check"
    )
}

/// Tokenize sprefa source text.
///
/// Handles unclosed strings gracefully for partial syntax support.
/// Skips whitespace and line comments (`# ...`).
/// For content inside parens after tags, captures raw content to preserve
/// glob patterns like `**/car`.
pub fn tokenize(input: &str) -> Vec<TokenSpan> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        // Skip whitespace
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }

        // Skip line comments
        if bytes[pos] == b'#' {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }

        let start = pos;

        // String literal (double quotes)
        if bytes[pos] == b'"' {
            pos += 1;
            let mut content = String::new();
            while pos < bytes.len() {
                if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
                    // Escape sequence
                    content.push(bytes[pos + 1] as char);
                    pos += 2;
                } else if bytes[pos] == b'"' {
                    pos += 1;
                    break;
                } else {
                    content.push(bytes[pos] as char);
                    pos += 1;
                }
            }
            // Note: if we hit EOF without closing quote, we still return the partial string
            tokens.push(TokenSpan {
                token: Token::String(content),
                start,
                end: pos,
            });
            continue;
        }

        // Single-character tokens
        let token = match bytes[pos] {
            b'(' => {
                pos += 1;
                Token::LParen
            }
            b')' => {
                pos += 1;
                Token::RParen
            }
            b'{' => {
                pos += 1;
                Token::LBrace
            }
            b'}' => {
                pos += 1;
                Token::RBrace
            }
            b'$' => {
                // Check if we're inside parens - if so, capture raw content
                let in_parens = tokens.last()
                    .map(|t| matches!(t.token, Token::LParen))
                    .unwrap_or(false);

                if in_parens {
                    // Capture raw content starting with $ until ) or EOF
                    let mut content = String::new();
                    while pos < bytes.len() && bytes[pos] != b')' {
                        content.push(bytes[pos] as char);
                        pos += 1;
                    }
                    if !content.is_empty() {
                        tokens.push(TokenSpan {
                            token: Token::Raw(content.trim().to_string()),
                            start: start,
                            end: pos,
                        });
                    }
                    continue;
                } else {
                    pos += 1;
                    Token::Dollar
                }
            }
            b'>' => {
                pos += 1;
                Token::Gt
            }
            b';' => {
                pos += 1;
                Token::Semi
            }
            b',' => {
                pos += 1;
                Token::Comma
            }
            _ => {
                // Check if we're inside parens (last token was LParen and no RParen yet)
                let in_parens = tokens.last()
                    .map(|t| matches!(t.token, Token::LParen))
                    .unwrap_or(false);

                if in_parens || bytes[pos] == b'$' {
                    // Capture raw content until ) or EOF
                    // This handles both:
                    // - Normal raw content after LParen
                    // - $... content (captures the $ and following identifier as raw)
                    let mut content = String::new();
                    while pos < bytes.len() && bytes[pos] != b')' {
                        content.push(bytes[pos] as char);
                        pos += 1;
                    }
                    if !content.is_empty() {
                        tokens.push(TokenSpan {
                            token: Token::Raw(content.trim().to_string()),
                            start: start,
                            end: pos,
                        });
                    }
                    continue;
                }

                // Identifier or tag
                if bytes[pos].is_ascii_alphabetic() || bytes[pos] == b'_' {
                    let mut word = String::new();
                    while pos < bytes.len()
                        && (bytes[pos].is_ascii_alphanumeric()
                            || bytes[pos] == b'_'
                            || bytes[pos] == b'.')
                    {
                        word.push(bytes[pos] as char);
                        pos += 1;
                    }

                    // Check for known tags
                    let token = if is_known_tag(&word) {
                        Token::Tag(word)
                    } else {
                        Token::Ident(word)
                    };
                    tokens.push(TokenSpan {
                        token,
                        start,
                        end: pos,
                    });
                    continue;
                } else {
                    // Unknown character, skip it
                    pos += 1;
                    continue;
                }
            }
        };

        tokens.push(TokenSpan {
            token,
            start,
            end: pos,
        });
    }

    tokens.push(TokenSpan {
        token: Token::Eof,
        start: pos,
        end: pos,
    });

    tokens
}

/// Scan tokens backwards from cursor position.
///
/// Returns tokens in reverse order (most recent first), useful for
/// context detection looking backwards from the cursor.
pub fn scan_backwards(tokens: &[TokenSpan], cursor_pos: usize) -> Vec<&TokenSpan> {
    tokens
        .iter()
        .filter(|t| t.start < cursor_pos)
        .rev()
        .collect()
}

/// Detect context from token stream for autocomplete.
///
/// Uses heuristics to handle incomplete syntax like `fs(**/car` (no closing paren).
/// 
/// The cursor_pos indicates where the cursor is, which helps detect when we're
/// right after content inside parens (e.g., `fs(**/|)` should still be InsideTag).
pub fn detect_context(tokens: &[TokenSpan], cursor_pos: usize) -> Context {
    if tokens.is_empty() {
        return Context::Unknown;
    }

    // First pass: check for InsideRepoRev (highest priority for scoped contexts)
    if let Some(ctx) = detect_repo_rev_context(tokens, cursor_pos) {
        return ctx;
    }

    // Check if cursor is immediately after a Raw token inside parens
    // This handles `fs(**/|)` where cursor is after the raw content
    for (i, token) in tokens.iter().enumerate() {
        if let Token::Raw(content) = &token.token {
            // Cursor is right after this raw token
            if token.end == cursor_pos {
                // Look back for LParen then Tag
                if i >= 2 {
                    if let Token::LParen = tokens[i-1].token {
                        if let Token::Tag(tag_name) = &tokens[i-2].token {
                            return Context::InsideTag {
                                tag: tag_name.clone(),
                                partial: content.clone(),
                            };
                        }
                    }
                }
            }
        }
    }

    // Second pass: check for other contexts
    let mut iter = tokens.iter().rev();
    let mut found_lbrace = false;

    while let Some(token) = iter.next() {
        match &token.token {
            // Scope boundaries - if we hit these, we're inside a block body
            Token::LBrace | Token::RBrace => {
                found_lbrace = true;
                continue;
            }

            // Check for $ - we're typing a capture
            Token::Dollar => {
                return Context::InsideCapture;
            }

            // Check for raw content after LParen following a tag
            // Only match if we haven't crossed a scope boundary
            Token::Raw(content) if !found_lbrace => {
                // Check if this is a capture after $
                if content == "$" {
                    return Context::InsideCapture;
                }
                // Look back for LParen then Tag
                if let Some(lparen) = iter.next() {
                    if matches!(lparen.token, Token::LParen) {
                        if let Some(prev) = iter.next() {
                            if let Token::Tag(tag_name) = &prev.token {
                                return Context::InsideTag {
                                    tag: tag_name.clone(),
                                    partial: content.clone(),
                                };
                            }
                        }
                    }
                }
            }

            // Check for tag followed by LParen with no content (empty parens)
            Token::LParen if !found_lbrace => {
                // Look back for a tag
                if let Some(prev) = iter.next() {
                    if let Token::Tag(tag_name) = &prev.token {
                        return Context::InsideTag {
                            tag: tag_name.clone(),
                            partial: String::new(),
                        };
                    }
                }
            }

            _ => {}
        }
    }

    Context::Unknown
}

/// Detect InsideRepoRev context by looking for repo/rev patterns.
fn detect_repo_rev_context(tokens: &[TokenSpan], _cursor_pos: usize) -> Option<Context> {
    // Look for pattern: repo(X) { ... rev(Y
    // We need to find a rev/branch/tag tag that has a repo ancestor

    let mut rev_tag_idx = None;
    let mut rev_start_pos = 0;

    // Find the rightmost rev/branch/tag tag
    for (i, t) in tokens.iter().enumerate().rev() {
        if let Token::Tag(tag_name) = &t.token {
            if tag_name == "rev"
                || tag_name == "branch"
                || tag_name == "tag"
                || tag_name == "rev.norm"
                || tag_name == "branch.norm"
                || tag_name == "tag.norm"
            {
                rev_tag_idx = Some(i);
                rev_start_pos = t.end;
                break;
            }
        }
    }

    let rev_idx = rev_tag_idx?;

    // Look for enclosing repo before this rev
    if let Some(repo) = find_enclosing_repo(tokens, rev_idx) {
        // Get partial content after rev tag
        let partial = collect_partial_after(tokens, rev_start_pos);
        // Remove leading ( from raw content token, keeping $ for captures
        let partial = partial.strip_prefix('(').unwrap_or(&partial).to_string();

        return Some(Context::InsideRepoRev { repo, partial });
    }

    None
}

/// Collect partial text after a position (up to next delimiter or EOF).
fn collect_partial_after(tokens: &[TokenSpan], start_pos: usize) -> String {
    tokens
        .iter()
        .filter(|t| t.start >= start_pos)
        .take_while(|t| {
            !matches!(
                t.token,
                Token::RParen | Token::RBrace | Token::Semi | Token::Gt | Token::Eof
            )
        })
        .map(|t| match &t.token {
            Token::Ident(s) | Token::String(s) | Token::Tag(s) | Token::Raw(s) => s.as_str(),
            Token::LParen => "(",
            Token::LBrace => "{",
            Token::Comma => ",",
            _ => "",
        })
        .collect::<String>()
}

/// Find the enclosing repo name for a position.
///
/// Scans backwards looking for `repo(name) { ...` pattern.
fn find_enclosing_repo(tokens: &[TokenSpan], before_idx: usize) -> Option<String> {
    let mut depth = 0i32;
    let mut idx = before_idx;

    while idx > 0 {
        idx -= 1;
        let token = &tokens[idx];

        match &token.token {
            Token::RBrace => depth += 1,
            Token::LBrace => {
                depth -= 1;
                if depth < 0 {
                    // Found an opening brace, look for repo(...) before it
                    depth = 0;

                    // Look for RParen before the brace
                    if idx > 0 {
                        idx -= 1;
                        let rparen = &tokens[idx];
                        if matches!(rparen.token, Token::RParen) {
                            // Look back for the repo body content
                            let mut repo_content = String::new();
                            let mut found_lparen = false;

                            while idx > 0 {
                                idx -= 1;
                                let t = &tokens[idx];
                                match &t.token {
                                    Token::LParen => {
                                        found_lparen = true;
                                        break;
                                    }
                                    Token::Ident(s) | Token::String(s) | Token::Raw(s) => {
                                        repo_content = s.clone() + &repo_content;
                                    }
                                    Token::Dollar => {
                                        repo_content = "$".to_string() + &repo_content;
                                    }
                                    Token::Tag(s) if s == "repo" || s == "repo.norm" => {
                                        return Some(repo_content);
                                    }
                                    _ => {}
                                }
                            }

                            if found_lparen && idx > 0 {
                                // Check if previous token is repo tag
                                let prev = &tokens[idx - 1];
                                if let Token::Tag(s) = &prev.token {
                                    if s == "repo" || s == "repo.norm" {
                                        return Some(repo_content);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Convenience: tokenize and detect context at cursor position.
pub fn analyze_at_position(input: &str, cursor_pos: usize) -> (Vec<TokenSpan>, Context) {
    let tokens = tokenize(input);
    let context = detect_context(&tokens, tokens.last().map(|t| t.end).unwrap_or(0));
    (tokens, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() {
        let input = r#"rule(pkg) { fs(**/Cargo.toml) }"#;
        let tokens = tokenize(input);

        assert!(tokens.iter().any(|t| matches!(&t.token, Token::Tag(s) if s == "rule")));
        assert!(tokens.iter().any(|t| matches!(&t.token, Token::Tag(s) if s == "fs")));
        assert!(tokens.iter().any(|t| matches!(&t.token, Token::LParen)));
        assert!(tokens.iter().any(|t| matches!(&t.token, Token::RParen)));
        assert!(tokens.iter().any(|t| matches!(&t.token, Token::LBrace)));
        assert!(tokens.iter().any(|t| matches!(&t.token, Token::RBrace)));
    }

    #[test]
    fn test_tokenize_string() {
        let input = r#"json({ name: "test" })"#;
        let tokens = tokenize(input);

        let string_token = tokens.iter().find(|t| matches!(&t.token, Token::String(_)));
        assert!(string_token.is_some());
        assert_eq!(
            string_token.unwrap().token,
            Token::String("test".to_string())
        );
    }

    #[test]
    fn test_tokenize_unclosed_string() {
        let input = r#"json({ name: "test"#;
        let tokens = tokenize(input);

        let string_token = tokens.iter().find(|t| matches!(&t.token, Token::String(_)));
        assert!(string_token.is_some());
        assert_eq!(
            string_token.unwrap().token,
            Token::String("test".to_string())
        );
    }

    #[test]
    fn test_tokenize_captures() {
        let input = r#"$NAME ${VAR}"#;
        let tokens = tokenize(input);

        assert!(tokens.iter().any(|t| matches!(&t.token, Token::Dollar)));
        assert!(tokens.iter().any(|t| matches!(&t.token, Token::Ident(s) if s == "NAME")));
        assert!(tokens.iter().any(|t| matches!(&t.token, Token::Ident(s) if s == "VAR")));
    }

    #[test]
    fn test_tokenize_comments() {
        let input = r#"
# This is a comment
rule(pkg) { fs(**/Cargo.toml) }
"#;
        let tokens = tokenize(input);

        assert!(!tokens.iter().any(|t| {
            if let Token::Ident(s) | Token::Tag(s) | Token::String(s) = &t.token {
                s.contains("comment")
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_scan_backwards() {
        let input = r#"rule(pkg) { fs(**/Cargo.toml) }"#;
        let tokens = tokenize(input);

        let backwards = scan_backwards(&tokens, 15);
        assert!(!backwards.is_empty());
        assert!(backwards.iter().any(|t| matches!(&t.token, Token::Tag(s) if s == "rule")));
    }

    #[test]
    fn test_detect_inside_tag() {
        let input = r#"fs(**/car"#;
        let tokens = tokenize(input);
        let context = detect_context(&tokens, tokens.last().map(|t| t.end).unwrap_or(0));

        assert_eq!(
            context,
            Context::InsideTag {
                tag: "fs".to_string(),
                partial: "**/car".to_string(),
            }
        );
    }

    #[test]
    fn test_detect_inside_capture() {
        let input = r#"rule(pkg) { fs($"#;
        let tokens = tokenize(input);
        let context = detect_context(&tokens, tokens.last().map(|t| t.end).unwrap_or(0));

        assert_eq!(context, Context::InsideCapture);
    }

    #[test]
    fn test_detect_inside_repo_rev() {
        let input = r#"repo(myrepo) { rev(main"#;
        let tokens = tokenize(input);
        let context = detect_context(&tokens, tokens.last().map(|t| t.end).unwrap_or(0));

        assert_eq!(
            context,
            Context::InsideRepoRev {
                repo: "myrepo".to_string(),
                partial: "main".to_string(),
            }
        );
    }

    #[test]
    fn test_detect_inside_repo_rev_with_capture() {
        let input = r#"repo($REPO) { rev($BRANCH"#;
        let tokens = tokenize(input);
        let context = detect_context(&tokens, tokens.last().map(|t| t.end).unwrap_or(0));

        assert_eq!(
            context,
            Context::InsideRepoRev {
                repo: "$REPO".to_string(),
                partial: "$BRANCH".to_string(),
            }
        );
    }

    #[test]
    fn test_analyze_at_position() {
        let input = r#"rule(pkg) { fs(**/Cargo.toml) }"#;
        let (tokens, _context) = analyze_at_position(input, 20);
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_partial_with_special_chars() {
        let input = r#"fs(**/src/*.rs"#;
        let tokens = tokenize(input);
        let context = detect_context(&tokens, tokens.last().map(|t| t.end).unwrap_or(0));

        assert!(
            matches!(context, Context::InsideTag { tag, partial } if tag == "fs" && partial.contains("**/src/*.rs"))
        );
    }
}
