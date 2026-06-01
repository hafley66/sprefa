use anyhow::{bail, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum StrPart { Lit(String), Var(String) }

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Str(String),
    InterpStr(Vec<StrPart>), // a "..." containing ${var} holes
    Int(i64),
    Regex(String),
    LParen, RParen, Comma, Dot, Colon, Bang, Question, Arrow,
    Eq, Ne, Lt, Le, Gt, Ge, Match, Glob,
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
            b',' => { out.push(Tok::Comma); i += 1; }
            b'.' => { out.push(Tok::Dot); i += 1; }
            b':' => { out.push(Tok::Colon); i += 1; }
            b'?' => { out.push(Tok::Question); i += 1; }
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
                    } else { cur.push(b[i] as char); i += 1; }
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
            b'/' => {
                let mut s = String::new();
                i += 1;
                while i < b.len() && b[i] != b'/' {
                    if b[i] == b'\\' && i + 1 < b.len() {
                        s.push(b[i] as char); s.push(b[i + 1] as char); i += 2;
                    } else { s.push(b[i] as char); i += 1; }
                }
                if i >= b.len() { bail!("unterminated regex"); }
                i += 1;
                out.push(Tok::Regex(s));
            }
            _ if c.is_ascii_digit() => {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() { i += 1; }
                out.push(Tok::Int(src[start..i].parse()?));
            }
            _ if c == b'_' || c.is_ascii_alphabetic() => {
                let start = i;
                while i < b.len() && (b[i] == b'_' || b[i].is_ascii_alphanumeric()) { i += 1; }
                out.push(Tok::Ident(src[start..i].to_string()));
            }
            _ => bail!("unexpected char {:?}", c as char),
        }
    }
    Ok(out)
}
