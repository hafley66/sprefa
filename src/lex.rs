use anyhow::{bail, Result};

use crate::desc;

#[derive(Clone, Debug, PartialEq)]
pub enum StrPart { Lit(String), Var(String) }

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Str(String),
    InterpStr(Vec<StrPart>), // a "..." containing ${var} holes
    Int(i64),
    Regex(String),
    /// A typed path literal `scheme:body` (`fs:src/x`, `glob:src/**/*.rs`). `span`
    /// is the (start, end) byte offset of the whole literal in the source, for
    /// diagnostics. The body is the raw (still-escaped) text after the colon.
    Scheme { scheme: String, body: String, span: (u32, u32) },
    /// A temporal rule modifier after the `<-` neck: `@next` / `@async`. The
    /// string is the bare word after `@` (validated to a known modifier in the
    /// parser, not here). `@` is otherwise unused punctuation.
    At(String),
    LParen, RParen, Comma, Dot, Colon, Bang, Question, Arrow,
    ThinArrow, // `->` effect-output arrow (sh fn outs; distinct from `<-` neck)
    Lt2, // `<:` brand subtype operator
    Pipe, // `|` enum-brand variant separator (`type sev = "a" | "b"`)
    Eq, Ne, Lt, Le, Gt, Ge, Match, Glob,
    Plus, Minus, Star, Slash, Percent, // int arithmetic (heads + comparisons)
}

/// Scan a typed-literal body after the `scheme:` prefix. `i` points just past the
/// colon and is advanced past the body. Returns the RAW body text (escapes kept;
/// the descriptor resolver unescapes). Two forms (v4 rules):
///   - fenced: `` `...` `` — only `` ` `` and `${` are special.
///   - bare: ends at whitespace / `,` / `)` at depth 0; `()[]{}` tracked balanced;
///     `\` escapes the next char; `${NAME}` is an interp hole.
fn lex_scheme_body(b: &[u8], src: &str, i: &mut usize) -> Result<String> {
    if b.get(*i) == Some(&b'`') {
        // Fenced form.
        *i += 1;
        let start = *i;
        while *i < b.len() && b[*i] != b'`' {
            // `${...}` runs to its `}` (still inside the fence); a lone backslash
            // is literal in fenced form (only backtick and `${` are special).
            *i += 1;
        }
        if *i >= b.len() { bail!("unterminated fenced scheme literal"); }
        let body = src[start..*i].to_string();
        *i += 1; // closing backtick
        return Ok(body);
    }
    // Bare form.
    let start = *i;
    let mut depth = 0i32;
    while *i < b.len() {
        let c = b[*i];
        match c {
            b'\\' if *i + 1 < b.len() => { *i += 2; continue; }
            b'(' | b'[' | b'{' => { depth += 1; }
            b')' | b']' | b'}' => {
                if depth == 0 && c == b')' { break; }
                depth -= 1;
            }
            b' ' | b'\t' | b'\r' | b'\n' | b',' if depth == 0 => break,
            _ => {}
        }
        *i += 1;
    }
    if *i == start { bail!("empty scheme literal body"); }
    Ok(src[start..*i].to_string())
}

/// Find the body-end index of a raw string whose body starts at `start` in
/// `b`. The close sequence is `delim` (`"` or `` ` ``) followed by exactly
/// `hashes` `#` bytes. Returns the index of the `delim` (so the body is
/// `b[start..ret]`); the caller advances past `delim + hashes`. `None` when no
/// such close exists (unterminated). With `hashes == 0` this is just "the next
/// delim"; with N>0 the body may contain `delim` not followed by N `#`s.
fn find_raw_close(b: &[u8], start: usize, delim: u8, hashes: usize) -> Option<usize> {
    let mut i = start;
    while i < b.len() {
        if b[i] == delim {
            // For hashes==0, this `delim` closes. Else require `hashes` `#`s
            // immediately after; otherwise keep scanning (an inner `delim` not
            // followed by enough `#`s is a literal body byte).
            if hashes == 0 {
                return Some(i);
            }
            let tail = &b[i + 1..];
            if tail.len() >= hashes && tail[..hashes].iter().all(|&c| c == b'#') {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

pub fn lex(src: &str) -> Result<Vec<Tok>> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => { i += 1; }
            b'#' => { while i < b.len() && b[i] != b'\n' { i += 1; } }
            b'(' => { out.push(Tok::LParen); i += 1; }
            b')' => { out.push(Tok::RParen); i += 1; }
            b'`' => {
                // Standalone fenced string: raw, multiline, only the closing
                // backtick terminates. Same Tok::Str as "...", so it flows
                // through term() and the sg()/ast_yaml() body arms unchanged.
                // Used for multiline YAML bodies (ast_yaml) and any literal
                // with quotes/newlines the "..." form would need to escape.
                i += 1;
                let start = i;
                while i < b.len() && b[i] != b'`' { i += 1; }
                if i >= b.len() { bail!("unterminated backtick string"); }
                let body = src[start..i].to_string();
                i += 1;
                out.push(Tok::Str(body));
            }
            b',' => { out.push(Tok::Comma); i += 1; }
            b'.' => { out.push(Tok::Dot); i += 1; }
            b':' => { out.push(Tok::Colon); i += 1; }
            b'?' => { out.push(Tok::Question); i += 1; }
            b'|' => { out.push(Tok::Pipe); i += 1; }
            b'=' => {
                if b.get(i + 1) == Some(&b'~') { out.push(Tok::Match); i += 2; }
                else { out.push(Tok::Eq); i += 1; }
            }
            b'~' => {
                if b.get(i + 1) == Some(&b'~') { out.push(Tok::Glob); i += 2; }
                else { bail!("lone '~' (use '~~' for glob, '=~' for regex)"); }
            }
            b'!' => {
                if b.get(i + 1) == Some(&b'=') { out.push(Tok::Ne); i += 2; }
                else { out.push(Tok::Bang); i += 1; }
            }
            b'<' => {
                match b.get(i + 1) {
                    Some(&b'-') => { out.push(Tok::Arrow); i += 2; }
                    Some(&b'=') => { out.push(Tok::Le); i += 2; }
                    Some(&b':') => { out.push(Tok::Lt2); i += 2; }
                    _ => { out.push(Tok::Lt); i += 1; }
                }
            }
            b'>' => {
                if b.get(i + 1) == Some(&b'=') { out.push(Tok::Ge); i += 2; }
                else { out.push(Tok::Gt); i += 1; }
            }
            b'"' => {
                i += 1;
                let mut cur = String::new();
                let mut parts: Vec<StrPart> = Vec::new();
                let mut interp = false;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' && i + 1 < b.len() {
                        if matches!(b[i + 1], b's' | b'd' | b'w' | b'b' | b'n') {
                            let c = b[i + 1] as char;
                            tracing::warn!(c = %c, "warning[plain-string-escape]: `\\\\{c}` in a plain string drops the backslash; use r\"...\" for regex text");
                        }
                        cur.push(b[i + 1] as char); i += 2;
                    } else if b[i] == b'$' && b.get(i + 1) == Some(&b'{') {
                        interp = true;
                        if !cur.is_empty() { parts.push(StrPart::Lit(std::mem::take(&mut cur))); }
                        i += 2;
                        let start = i;
                        while i < b.len() && b[i] != b'}' { i += 1; }
                        if i >= b.len() { bail!("unterminated ${{ in string"); }
                        parts.push(StrPart::Var(src[start..i].to_string()));
                        i += 1; // skip }
                    } else if b[i] == b'$' && b.get(i + 1) == Some(&b'$') && b.get(i + 2) == Some(&b'{') {
                        // `$${...}` escapes a literal `${...}` that would otherwise
                        // open a `${NAME}` interp. Emits `${` and advances past
                        // all three, so `$${name}` survives verbatim (a JS template
                        // literal `\`hello ${jsName}\`` written into a dl `"..."`
                        // needs this — the JS dollar must not be read as dl interp).
                        // Scoped to `$${` so ast-grep variadic metavars (`$$$NAME`)
                        // are untouched; for whole-hog no-interp, use `r"..."` / `` r`...` ``.
                        cur.push_str("${"); i += 3;
                    } else if b[i] == b'$' {
                        // A bare `$` not opening `${...}`: a literal dollar, as in
                        // ast-grep metavars ($X, $$$A) inside an sg/ast pattern.
                        // Without this arm the run-scanner below stops on `$`
                        // without advancing and the outer loop spins forever.
                        cur.push('$'); i += 1;
                    } else {
                        // Push the run of boring bytes as a slice to keep UTF-8
                        // sequences intact. Byte-by-byte `b[i] as char` would
                        // independently Latin-1-promote each byte of a multibyte
                        // sequence and double-encode it (e.g. `—` -> `ââ`).
                        let start = i;
                        while i < b.len() && b[i] != b'"' && b[i] != b'\\' && b[i] != b'$' {
                            i += 1;
                        }
                        cur.push_str(&src[start..i]);
                    }
                }
                if i >= b.len() { bail!("unterminated string"); }
                i += 1;
                if interp {
                    if !cur.is_empty() { parts.push(StrPart::Lit(cur)); }
                    out.push(Tok::InterpStr(parts));
                } else {
                    out.push(Tok::Str(cur));
                }
            }
            b'@' => {
                // A temporal rule modifier `@next` / `@async` after the `<-`
                // neck. Lex the bare word; the parser validates it and rejects
                // `@` in any other position. Independent of the `<-` (Arrow)
                // token, so `<-@next` and `<- @next` lex identically.
                i += 1;
                let start = i;
                while i < b.len() && (b[i] == b'_' || b[i].is_ascii_alphanumeric()) { i += 1; }
                if i == start { bail!("lone '@' (expected @next/@async after a rule neck)"); }
                out.push(Tok::At(src[start..i].to_string()));
            }
            b'+' => { out.push(Tok::Plus); i += 1; }
            b'-' => {
                if b.get(i + 1) == Some(&b'>') { out.push(Tok::ThinArrow); i += 2; }
                else { out.push(Tok::Minus); i += 1; }
            }
            b'*' => { out.push(Tok::Star); i += 1; }
            b'%' => { out.push(Tok::Percent); i += 1; }
            b'/' => {
                // `/` is division after a value (ident, int, `)`), a regex
                // opener everywhere else (after `,`, `(`, `=~`, ...). The same
                // value-position rule JS lexers use for / vs /re/.
                let after_value = matches!(out.last(),
                    Some(Tok::Ident(_)) | Some(Tok::Int(_)) | Some(Tok::RParen));
                if after_value {
                    out.push(Tok::Slash); i += 1;
                } else {
                    let mut s = String::new();
                    i += 1;
                    while i < b.len() && b[i] != b'/' {
                        if b[i] == b'\\' && i + 1 < b.len() {
                            s.push(b[i] as char); s.push(b[i + 1] as char); i += 2;
                        } else {
                            // Slice-run preserves UTF-8 (see string-literal arm above).
                            let start = i;
                            while i < b.len() && b[i] != b'/' && b[i] != b'\\' {
                                i += 1;
                            }
                            s.push_str(&src[start..i]);
                        }
                    }
                    if i >= b.len() { bail!("unterminated regex"); }
                    i += 1;
                    // A bare `//` lexes as an empty regex, which is never useful
                    // and is almost always a C-style comment habit (dl comments
                    // are `#`). Fail here with a clear message instead of letting
                    // a `Regex("")` token surface as a baffling parse error later.
                    if s.is_empty() {
                        bail!("empty regex `//`: dl comments start with `#` (not `//`), string literals use quotes");
                    }
                    out.push(Tok::Regex(s));
                }
            }
            _ if c.is_ascii_digit() => {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() { i += 1; }
                out.push(Tok::Int(src[start..i].parse()?));
            }
            _ if c == b'_' || c.is_ascii_alphabetic() => {
                let start = i;
                while i < b.len() && (b[i] == b'_' || b[i].is_ascii_alphanumeric()) { i += 1; }
                let word = &src[start..i];
                // Raw string prefix: `r"..."`, `` r`...` ``, or with hash
                // delimiters `r#"..."#` / `r##"..."##` / ... like Rust. Disables
                // `${NAME}` interp AND `\` escapes — every byte inside is literal
                // until the matching closing sequence. Hash delimiters let the
                // body contain the delim char (a JS template literal carrying
                // both `"` and `` ` `` needs `r#"..."#` since `r"..."` would
                // close at the first inner `"`). `r` must be a lone word.
                if word == "r" && i < b.len() && (b[i] == b'"' || b[i] == b'`' || b[i] == b'#') {
                    // Count opening `#`s (0 for `r"` / `` r` ``). Hash form is
                    // only valid before a `"` (Rust gates `r#`...`#` to double-
                    // quote bodies; dl mirrors that).
                    let mut hashes = 0usize;
                    let saved = i;
                    while b.get(i) == Some(&b'#') { i += 1; hashes += 1; }
                    let delim = match b.get(i) {
                        Some(&b'"') => b'"',
                        Some(&b'`') if hashes == 0 => b'`',
                        _ => {
                            // Not a raw string after all (`r#ident`, `r##stuff`);
                            // reset and fall through to the scheme/ident paths.
                            i = saved;
                            out.push(Tok::Ident(word.to_string()));
                            continue;
                        }
                    };
                    i += 1;
                    let body_start = i;
                    // Closing sequence is `delim` followed by exactly `hashes`
                    // `#`s. With hashes==0 the close is just the next `delim`.
                    let close = find_raw_close(b, i, delim, hashes)
                        .ok_or_else(|| anyhow::anyhow!(
                            "unterminated raw string (started at byte {body_start}; \
                             expected closing `{delim}` followed by {hashes} `#`)"))?;
                    let body = src[body_start..close].to_string();
                    i = close + 1 + hashes; // past delim + hashes
                    out.push(Tok::Str(body));
                    continue;
                }
                // A typed path literal: an identifier IMMEDIATELY followed by `:`
                // (no space) where the identifier is a registered scheme. The
                // `, :rust` form in ast()/sg() has a space before the colon and no
                // scheme word, so it still lexes as Colon + Ident. An unknown
                // `word:` adjacency is a parse error (unknown scheme).
                if b.get(i) == Some(&b':') && b.get(i + 1).is_some_and(|&n| n != b' ' && n != b'\t' && n != b'\n' && n != b'\r') {
                    if desc::is_scheme(word) {
                        let scheme = word.to_string();
                        i += 1; // consume ':'
                        let body = lex_scheme_body(b, src, &mut i)?;
                        out.push(Tok::Scheme { scheme, body, span: (start as u32, i as u32) });
                    } else {
                        bail!("unknown scheme `{word}:` (known: fs, glob)");
                    }
                } else {
                    out.push(Tok::Ident(word.to_string()));
                }
            }
            _ => bail!("unexpected char {:?}", c as char),
        }
    }
    Ok(out)
}
