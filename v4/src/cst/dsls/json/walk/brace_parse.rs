//! Parser for `json(...)` brace body. Ported from v2/src/walk/_4_brace_parse.rs.
//!
//! Grammar (verbatim from v2):
//!   pattern    = annotation | object | array | capture | wildcard | value_glob
//!   annotation = "$$" IDENT "(" "$" SCREAMING ")"
//!   object     = "{" (entry ("," entry)*)? "}"
//!   entry      = key ":" pattern
//!   key        = "**" | "$" SCREAMING | "$_" | "re:" REGEX | glob_str
//!   array      = "[" "..." pattern "]"
//!   capture    = "$" SCREAMING
//!   wildcard   = "$_"
//!   value_glob = (not , } ] )+
//!
//! `$$sigil(...)` annotations parse but the v0 op ignores them. They're
//! kept for v2 fidelity; v3 cross-repo wiring (sprefa-4m7.13.x) decides
//! whether to revive them.

use std::ops::Range;
use std::sync::Arc;

use super::compile::{KeyMatcher, ObjectEntry, SelectStep};

#[derive(Debug, Clone, PartialEq)]
pub struct ScanAnnotation {
    pub var: String,
    pub sigil: Arc<str>,
}

/// One `$NAME` / `${NAME}` / `${NAME?}` token recorded during the brace
/// parse. `range` is bytes into the body string (the slice handed to
/// `parse_body`).
#[derive(Debug, Clone)]
pub struct CapturePosition {
    pub name: Arc<str>,
    pub range: Range<usize>,
}

pub fn parse_body(
    src: &str,
) -> Result<(Vec<SelectStep>, Vec<ScanAnnotation>, Vec<CapturePosition>), String> {
    let mut pos = 0;
    let mut annotations = Vec::new();
    let mut positions = Vec::new();
    let steps = parse_pattern(src, &mut pos, &mut annotations, &mut positions)?;
    let remaining = src[pos..].trim();
    if !remaining.is_empty() {
        return Err(format!(
            "unexpected trailing content in json body: {:?}",
            remaining
        ));
    }
    Ok((steps, annotations, positions))
}

fn parse_pattern(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
    positions: &mut Vec<CapturePosition>,
) -> Result<Vec<SelectStep>, String> {
    skip_ws(input, pos);
    if *pos >= input.len() {
        return Err("unexpected end of json pattern".into());
    }

    if input[*pos..].starts_with("$$") {
        let sigil_start = *pos + 2;
        let mut p = sigil_start;
        let b = input.as_bytes();
        if p < b.len() && (b[p].is_ascii_alphabetic() || b[p] == b'_') {
            p += 1;
            while p < b.len() && (b[p].is_ascii_alphanumeric() || b[p] == b'_') {
                p += 1;
            }
            if p > sigil_start && b.get(p) == Some(&b'(') {
                let sigil = Arc::<str>::from(&input[sigil_start..p]);
                *pos = p + 1;
                skip_ws(input, pos);

                let inner_start = *pos;
                let inner_end = {
                    let mut q = inner_start;
                    while q < input.len() && input.as_bytes()[q] != b')' {
                        q += 1;
                    }
                    q
                };
                let inner_raw = input[inner_start..inner_end].trim();

                let is_bare_capture = inner_raw.starts_with('$')
                    && inner_raw.len() > 1
                    && inner_raw[1..]
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_ascii_uppercase())
                    && inner_raw[1..]
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');

                if !is_bare_capture {
                    return Err(format!(
                        "annotation $${} requires a bare capture var ($NAME), got: {{{}}}",
                        sigil, inner_raw
                    ));
                }

                let var_name = inner_raw[1..].to_string();
                *pos = inner_end;
                expect_byte(input, pos, b')')?;

                annotations.push(ScanAnnotation {
                    var: var_name.clone(),
                    sigil: sigil.clone(),
                });
                return Ok(vec![SelectStep::Leaf {
                    capture: Some(var_name),
                }]);
            }
        }
    }

    let c = input.as_bytes()[*pos];
    match c {
        b'{' => parse_object(input, pos, annotations, positions),
        b'[' => parse_array(input, pos, annotations, positions),
        b'$' => parse_capture_or_wildcard(input, pos, positions),
        b'"' => parse_quoted_value(input, pos),
        _ => parse_value_glob(input, pos),
    }
}

fn parse_object(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
    positions: &mut Vec<CapturePosition>,
) -> Result<Vec<SelectStep>, String> {
    expect_byte(input, pos, b'{')?;
    skip_ws(input, pos);

    let mut entries: Vec<ObjectEntry> = vec![];

    if peek_byte(input, *pos) == Some(b'}') {
        *pos += 1;
        return Ok(vec![SelectStep::Object { entries }]);
    }

    loop {
        skip_ws(input, pos);
        let (key, value_steps) = parse_entry(input, pos, annotations, positions)?;

        // `**:` and `$$${PATH?}:` both expand to recursive descent here,
        // consuming the rest of the object. `$$${PATH?}` additionally
        // binds the traversed path via `AnyCapture`.
        let recursive_step: Option<SelectStep> = match &key {
            KeyMatcher::Exact(s) if s == "**" => Some(SelectStep::Any),
            KeyMatcher::RecursiveCapture(name) => Some(SelectStep::AnyCapture {
                capture: name.clone(),
            }),
            _ => None,
        };
        if let Some(step) = recursive_step {
            skip_ws(input, pos);
            if peek_byte(input, *pos) == Some(b',') {
                *pos += 1;
            }
            skip_ws(input, pos);
            expect_byte(input, pos, b'}')?;

            if !entries.is_empty() {
                let mut result = vec![SelectStep::Object { entries }];
                result.push(step);
                result.extend(value_steps);
                return Ok(result);
            }
            let mut steps = vec![step];
            steps.extend(value_steps);
            return Ok(steps);
        }

        entries.push(ObjectEntry {
            key,
            value: value_steps,
        });

        skip_ws(input, pos);
        match peek_byte(input, *pos) {
            Some(b',') => {
                *pos += 1;
            }
            Some(b'}') => {
                *pos += 1;
                break;
            }
            Some(c) => {
                return Err(format!(
                    "expected `,` or `}}` in object, found {:?}",
                    c as char
                ))
            }
            None => return Err("unclosed `{` in json pattern".into()),
        }
    }

    Ok(vec![SelectStep::Object { entries }])
}

fn parse_array(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
    positions: &mut Vec<CapturePosition>,
) -> Result<Vec<SelectStep>, String> {
    expect_byte(input, pos, b'[')?;
    skip_ws(input, pos);

    if !input[*pos..].starts_with("...") {
        return Err("expected `...` after `[` in array pattern".into());
    }
    *pos += 3;
    skip_ws(input, pos);

    let item_steps = parse_pattern(input, pos, annotations, positions)?;

    skip_ws(input, pos);
    expect_byte(input, pos, b']')?;

    Ok(vec![SelectStep::Array { item: item_steps }])
}

fn parse_capture_or_wildcard(
    input: &str,
    pos: &mut usize,
    positions: &mut Vec<CapturePosition>,
) -> Result<Vec<SelectStep>, String> {
    let token_start = *pos;
    *pos += 1;
    if *pos >= input.len() {
        return Err("unexpected end after `$`".into());
    }

    if input.as_bytes()[*pos] == b'{' {
        let inner_start = *pos + 1;
        let mut depth = 1usize;
        *pos += 1;
        while *pos < input.len() && depth > 0 {
            match input.as_bytes()[*pos] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            *pos += 1;
        }
        if depth != 0 {
            return Err("unclosed `${` in json pattern".into());
        }
        let inner = &input[inner_start..*pos - 1];
        let (name_with_suffix, is_crossref) = if let Some((_rule, rest)) = inner.split_once('.') {
            let n = rest.strip_prefix('$').ok_or_else(|| {
                format!(
                    "malformed cross-ref `${{{}}}` (expected `${{rule.$VAR}}`)",
                    inner
                )
            })?;
            (n, true)
        } else {
            (inner, false)
        };
        // Trailing `?` = Unbound binding marker (host-grammar `${NAME?}`).
        // Recorded structurally elsewhere; for the v0 walker `${X?}` and
        // `${X}` are the same capture name.
        let name = name_with_suffix
            .strip_suffix('?')
            .unwrap_or(name_with_suffix);
        if name.is_empty()
            || !(name.as_bytes()[0].is_ascii_alphabetic() || name.as_bytes()[0] == b'_')
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(format!("invalid capture name `${{{}}}`", inner));
        }
        positions.push(CapturePosition {
            name: Arc::from(name),
            range: token_start..*pos,
        });
        return if is_crossref {
            Ok(vec![SelectStep::Leaf {
                capture: Some(name.to_string()),
            }])
        } else {
            Ok(vec![SelectStep::CaptureAny {
                capture: Some(name.to_string()),
            }])
        };
    }

    if input.as_bytes()[*pos] == b'_'
        && (*pos + 1 >= input.len() || !input.as_bytes()[*pos + 1].is_ascii_alphanumeric())
    {
        *pos += 1;
        return Ok(vec![]);
    }

    let start = *pos;
    while *pos < input.len()
        && (input.as_bytes()[*pos].is_ascii_alphanumeric() || input.as_bytes()[*pos] == b'_')
    {
        *pos += 1;
    }
    let name = &input[start..*pos];
    if name.is_empty() {
        return Err("empty capture name after `$`".into());
    }
    positions.push(CapturePosition {
        name: Arc::from(name),
        range: token_start..*pos,
    });
    Ok(vec![SelectStep::CaptureAny {
        capture: Some(name.to_string()),
    }])
}

fn parse_quoted_value(input: &str, pos: &mut usize) -> Result<Vec<SelectStep>, String> {
    *pos += 1;
    let start = *pos;
    while *pos < input.len() && input.as_bytes()[*pos] != b'"' {
        *pos += 1;
    }
    if *pos >= input.len() {
        return Err("unclosed `\"` in json pattern".into());
    }
    let content = input[start..*pos].to_string();
    *pos += 1;

    if content.contains('$') {
        Ok(vec![SelectStep::LeafPattern { pattern: content }])
    } else {
        Ok(vec![SelectStep::Key {
            name: content,
            capture: None,
        }])
    }
}

fn parse_value_glob(input: &str, pos: &mut usize) -> Result<Vec<SelectStep>, String> {
    let start = *pos;
    while *pos < input.len() {
        match input.as_bytes()[*pos] {
            b',' | b'}' | b']' => break,
            _ => *pos += 1,
        }
    }
    let text = input[start..*pos].trim();
    if text.is_empty() {
        return Err("empty value in json pattern".into());
    }
    Ok(vec![SelectStep::Key {
        name: text.to_string(),
        capture: None,
    }])
}

fn parse_entry(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
    positions: &mut Vec<CapturePosition>,
) -> Result<(KeyMatcher, Vec<SelectStep>), String> {
    skip_ws(input, pos);
    let key = parse_key(input, pos, positions)?;
    skip_ws(input, pos);
    expect_byte(input, pos, b':')?;
    skip_ws(input, pos);
    let value = parse_pattern(input, pos, annotations, positions)?;
    Ok((key, value))
}

fn parse_key(
    input: &str,
    pos: &mut usize,
    positions: &mut Vec<CapturePosition>,
) -> Result<KeyMatcher, String> {
    skip_ws(input, pos);

    if input.as_bytes()[*pos] == b'"' {
        *pos += 1;
        let start = *pos;
        while *pos < input.len() && input.as_bytes()[*pos] != b'"' {
            *pos += 1;
        }
        if *pos >= input.len() {
            return Err("unclosed `\"` in key position".into());
        }
        let content = &input[start..*pos];
        *pos += 1;
        return Ok(key_matcher_parse(content));
    }

    // json `$$${NAME?}` / `$$${NAME}` carveout: the recursive-descent
    // path-capture key (the v1/v2 `**:` "arbitrary json path down" idea).
    // The host pre-pass already recorded a Bind interp at the third `$`
    // (it matches `${`, so its range.lo points at `$$$`+2, NOT +3); the
    // json native parser still sees the raw `$$${NAME?}` bytes here and
    // owns the `?`-strip, exactly as the single `${NAME?}` value path
    // does. Detected in key position only; value-position `$$$` stays
    // ast-grep territory.
    if input[*pos..].starts_with("$$${") {
        let token_start = *pos;
        let inner_start = *pos + 4;
        let mut p = inner_start;
        let b = input.as_bytes();
        while p < input.len() && b[p] != b'}' {
            p += 1;
        }
        if p >= input.len() {
            return Err("unclosed `$$${` in json key position".into());
        }
        let inner = &input[inner_start..p];
        // Strip the host-grammar bind marker; `$$${X?}` and `$$${X}` are
        // the same recursive path-capture (mirrors `${X?}` value path).
        let name = inner.strip_suffix('?').unwrap_or(inner);
        if name.is_empty()
            || !(name.as_bytes()[0].is_ascii_alphabetic() || name.as_bytes()[0] == b'_')
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(format!("invalid recursive-capture name `$$${{{}}}`", inner));
        }
        *pos = p + 1;
        positions.push(CapturePosition {
            name: Arc::from(name),
            range: token_start..*pos,
        });
        return Ok(KeyMatcher::RecursiveCapture(name.to_string()));
    }

    if input[*pos..].starts_with("**")
        && (*pos + 2 >= input.len() || !input.as_bytes()[*pos + 2].is_ascii_alphanumeric())
    {
        *pos += 2;
        return Ok(KeyMatcher::Exact("**".to_string()));
    }

    if input[*pos..].starts_with("re:") {
        *pos += 3;
        let start = *pos;
        while *pos < input.len() {
            if input.as_bytes()[*pos] == b':' {
                let after = *pos + 1;
                if after >= input.len()
                    || matches!(
                        input.as_bytes()[after],
                        b' ' | b'\t' | b'\n' | b'{' | b'[' | b'$'
                    )
                {
                    break;
                }
            }
            *pos += 1;
        }
        let re_pattern = input[start..*pos].trim();
        return Ok(KeyMatcher::Glob(format!("re:{}", re_pattern)));
    }

    if input.as_bytes()[*pos] == b'$' {
        let token_start = *pos;
        *pos += 1;
        if *pos < input.len()
            && input.as_bytes()[*pos] == b'_'
            && (*pos + 1 >= input.len() || !input.as_bytes()[*pos + 1].is_ascii_alphanumeric())
        {
            *pos += 1;
            return Ok(KeyMatcher::Wildcard);
        }
        let start = *pos;
        while *pos < input.len()
            && (input.as_bytes()[*pos].is_ascii_alphanumeric() || input.as_bytes()[*pos] == b'_')
        {
            *pos += 1;
        }
        let name = &input[start..*pos];
        if name.is_empty() {
            return Err("empty capture name after `$` in key position".into());
        }
        positions.push(CapturePosition {
            name: Arc::from(name),
            range: token_start..*pos,
        });
        return Ok(KeyMatcher::Capture(name.to_string()));
    }

    let start = *pos;
    while *pos < input.len() && input.as_bytes()[*pos] != b':' {
        *pos += 1;
    }
    let key_str = input[start..*pos].trim();
    if key_str.is_empty() {
        return Err("empty key in json pattern".into());
    }
    Ok(key_matcher_parse(key_str))
}

fn key_matcher_parse(s: &str) -> KeyMatcher {
    if s == "$_" {
        KeyMatcher::Wildcard
    } else if s.starts_with('$')
        && s.len() > 1
        && s[1..].starts_with(|c: char| c.is_ascii_uppercase())
        && !s.contains('/')
        && !s.contains(':')
    {
        KeyMatcher::Capture(s[1..].to_string())
    } else if s.contains('*') || s.contains('?') || s.contains('[') || s.contains('$') {
        KeyMatcher::Glob(s.to_string())
    } else {
        KeyMatcher::Exact(s.to_string())
    }
}

fn skip_ws(input: &str, pos: &mut usize) {
    while *pos < input.len() && input.as_bytes()[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn peek_byte(input: &str, pos: usize) -> Option<u8> {
    input.as_bytes().get(pos).copied()
}

fn expect_byte(input: &str, pos: &mut usize, expected: u8) -> Result<(), String> {
    match input.as_bytes().get(*pos) {
        Some(&b) if b == expected => {
            *pos += 1;
            Ok(())
        }
        Some(&b) => Err(format!(
            "expected {:?}, found {:?}",
            expected as char, b as char
        )),
        None => Err(format!(
            "expected {:?}, found end of input",
            expected as char
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_object() {
        let (steps, _, _) = parse_body("{ name: $NAME }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Exact(s) if s == "name"));
                assert!(
                    matches!(&entries[0].value[0], SelectStep::CaptureAny { capture: Some(c) } if c == "NAME")
                );
            }
            _ => panic!(),
        }
    }

    #[test]
    fn double_star_wildcard() {
        let (steps, _, _) = parse_body("{ **: { image: $I } }").unwrap();
        assert!(matches!(&steps[0], SelectStep::Any));
        assert!(matches!(&steps[1], SelectStep::Object { .. }));
    }

    #[test]
    fn triple_dollar_key_is_recursive_capture() {
        // `$$${PATH?}` in key position lowers to AnyCapture, not a
        // single-key Capture. Mirrors the `**` → Any expansion.
        let (steps, _, positions) = parse_body("{ $$${PATH?}: { id: $X } }").unwrap();
        assert!(
            matches!(&steps[0], SelectStep::AnyCapture { capture } if capture == "PATH"),
            "steps[0] = {:?}",
            steps[0]
        );
        // `?` is stripped at parse, capture name recorded for LSP.
        assert!(positions.iter().any(|p| p.name.as_ref() == "PATH"));
    }

    #[test]
    fn captured_key() {
        let (steps, _, _) = parse_body("{ $K: $V }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Capture(s) if s == "K"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn array_iter() {
        let (steps, _, _) = parse_body("{ items: [...$X] }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].value[0], SelectStep::Array { .. }));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn unbound_suffix_in_brace_capture() {
        let (steps, _, _) = parse_body("{ name: ${N?}, version: ${V?} }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(
                    matches!(&entries[0].value[0], SelectStep::CaptureAny { capture: Some(c) } if c == "N")
                );
                assert!(
                    matches!(&entries[1].value[0], SelectStep::CaptureAny { capture: Some(c) } if c == "V")
                );
            }
            _ => panic!(),
        }
    }

    #[test]
    fn quoted_leaf_pattern() {
        let (steps, _, _) = parse_body(r#"{ image: "$REPO:$TAG" }"#).unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(
                    matches!(&entries[0].value[0], SelectStep::LeafPattern { pattern } if pattern == "$REPO:$TAG")
                );
            }
            _ => panic!(),
        }
    }
}
