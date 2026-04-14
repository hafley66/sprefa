/// json() body parser — v2.
///
/// Parses the string inside `json(...)` into a Vec<SelectStep> tree
/// that the existing walk engine can execute.
///
/// Grammar:
///   pattern    = annotation | object | array | capture | wildcard | value_glob
///   annotation = "$$" IDENT "(" "$" SCREAMING ")"
///   IDENT      = [a-zA-Z_][a-zA-Z0-9_]*         # op-declared sigil; validated at lower time
///   SCREAMING  = [A-Z][A-Z0-9_]*
///   object     = "{" (entry ("," entry)*)? "}"
///   entry      = key ":" pattern
///   key        = "**" | "$" SCREAMING | "$_" | "re:" REGEX | glob_str
///   array      = "[" "..." pattern "]"
///   capture    = "$" SCREAMING
///   wildcard   = "$_"
///   value_glob = (not , } ] )+
use std::sync::Arc;

use super::_2_compile::{KeyMatcher, ObjectEntry, SelectStep};

/// A scan annotation discovered during json pattern parsing. `sigil` names
/// the op-declared `ScanPointer` (e.g. "repo", "repo_norm", "rev_norm", or
/// a sigil from a newly registered op). Unknown sigils are diagnosed at
/// lower time, not here.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanAnnotation {
    pub var:   String,
    pub sigil: Arc<str>,
}

pub fn parse_body(src: &str) -> anyhow::Result<(Vec<SelectStep>, Vec<ScanAnnotation>)> {
    let mut pos = 0;
    let mut annotations = Vec::new();
    let steps = parse_pattern(src.trim(), &mut pos, &mut annotations)?;
    let remaining = src[pos..].trim();
    if !remaining.is_empty() {
        anyhow::bail!("unexpected trailing content in json body: {:?}", remaining);
    }
    Ok((steps, annotations))
}

fn parse_pattern(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
) -> anyhow::Result<Vec<SelectStep>> {
    skip_ws(input, pos);
    if *pos >= input.len() {
        anyhow::bail!("unexpected end of json pattern");
    }

    // Check for `$$<sigil>(<$VAR>)` annotation. Sigil is any identifier;
    // validation against registered `ScanPointer`s happens at lower time.
    if input[*pos..].starts_with("$$") {
        let sigil_start = *pos + 2;
        let mut p = sigil_start;
        let b = input.as_bytes();
        if p < b.len() && (b[p].is_ascii_alphabetic() || b[p] == b'_') {
            p += 1;
            while p < b.len() && (b[p].is_ascii_alphanumeric() || b[p] == b'_') { p += 1; }
            if p > sigil_start && b.get(p) == Some(&b'(') {
                let sigil = Arc::<str>::from(&input[sigil_start..p]);
                *pos = p + 1;
                skip_ws(input, pos);

                let inner_start = *pos;
                let inner_end = {
                    let mut q = inner_start;
                    while q < input.len() && input.as_bytes()[q] != b')' { q += 1; }
                    q
                };
                let inner_raw = input[inner_start..inner_end].trim();

                let is_bare_capture = inner_raw.starts_with('$')
                    && inner_raw.len() > 1
                    && inner_raw[1..].chars().next().map_or(false, |c| c.is_ascii_uppercase())
                    && inner_raw[1..].chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');

                if !is_bare_capture {
                    anyhow::bail!(
                        "annotation $${} requires a bare capture var ($NAME), got: {{{}}}",
                        sigil, inner_raw
                    );
                }

                let var_name = inner_raw[1..].to_string();
                *pos = inner_end;
                expect_byte(input, pos, b')')?;

                annotations.push(ScanAnnotation {
                    var:   var_name.clone(),
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
        b'{' => parse_object(input, pos, annotations),
        b'[' => parse_array(input, pos, annotations),
        b'$' => parse_capture_or_wildcard(input, pos),
        b'"' => parse_quoted_value(input, pos),
        _ => parse_value_glob(input, pos),
    }
}

fn parse_object(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
) -> anyhow::Result<Vec<SelectStep>> {
    expect_byte(input, pos, b'{')?;
    skip_ws(input, pos);

    let mut entries: Vec<ObjectEntry> = vec![];

    if peek_byte(input, *pos) == Some(b'}') {
        *pos += 1;
        return Ok(vec![SelectStep::Object { entries }]);
    }

    loop {
        skip_ws(input, pos);
        let (key, value_steps) = parse_entry(input, pos, annotations)?;

        if matches!(&key, KeyMatcher::Exact(s) if s == "**") {
            skip_ws(input, pos);
            if peek_byte(input, *pos) == Some(b',') {
                *pos += 1;
            }
            skip_ws(input, pos);
            expect_byte(input, pos, b'}')?;

            if !entries.is_empty() {
                let mut result = vec![SelectStep::Object { entries }];
                result.push(SelectStep::Any);
                result.extend(value_steps);
                return Ok(result);
            }
            let mut steps = vec![SelectStep::Any];
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
            Some(c) => anyhow::bail!("expected `,` or `}}` in object, found {:?}", c as char),
            None => anyhow::bail!("unclosed `{{` in json pattern"),
        }
    }

    Ok(vec![SelectStep::Object { entries }])
}

fn parse_array(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
) -> anyhow::Result<Vec<SelectStep>> {
    expect_byte(input, pos, b'[')?;
    skip_ws(input, pos);

    if !input[*pos..].starts_with("...") {
        anyhow::bail!("expected `...` after `[` in array pattern");
    }
    *pos += 3;
    skip_ws(input, pos);

    let item_steps = parse_pattern(input, pos, annotations)?;

    skip_ws(input, pos);
    expect_byte(input, pos, b']')?;

    Ok(vec![SelectStep::Array { item: item_steps }])
}

fn parse_capture_or_wildcard(input: &str, pos: &mut usize) -> anyhow::Result<Vec<SelectStep>> {
    *pos += 1; // skip $
    if *pos >= input.len() {
        anyhow::bail!("unexpected end after `$`");
    }

    // Braced form: ${...}. Two cases distinguished by presence of `.`:
    //   ${VAR}         → plain capture synonym of $VAR
    //   ${rule.$VAR}   → cross-ref. Lowered to a Leaf that captures under
    //                    `VAR`. At runtime, `expand_xrefs` (Layer 2) seeds
    //                    `VAR` on the cursor from the target rule's source
    //                    rows. The walker's Leaf step then constrains against
    //                    the seeded value (constrain-when-prebound), filtering
    //                    branches whose leaf text doesn't match. The `rule`
    //                    component is purely declarative here — the seed
    //                    machinery upstream is what wires it to source data.
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
            anyhow::bail!("unclosed `${{` in json pattern");
        }
        let inner = &input[inner_start..*pos - 1];
        let name = if let Some((_rule, rest)) = inner.split_once('.') {
            rest.strip_prefix('$').ok_or_else(|| {
                anyhow::anyhow!("malformed cross-ref `${{{}}}` (expected `${{rule.$VAR}}`)", inner)
            })?
        } else {
            inner
        };
        if name.is_empty()
            || !(name.as_bytes()[0].is_ascii_alphabetic() || name.as_bytes()[0] == b'_')
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            anyhow::bail!("invalid capture name `${{{}}}`", inner);
        }
        return Ok(vec![SelectStep::Leaf {
            capture: Some(name.to_string()),
        }]);
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
        anyhow::bail!("empty capture name after `$`");
    }
    Ok(vec![SelectStep::Leaf {
        capture: Some(name.to_string()),
    }])
}

fn parse_quoted_value(input: &str, pos: &mut usize) -> anyhow::Result<Vec<SelectStep>> {
    *pos += 1; // skip opening "
    let start = *pos;
    while *pos < input.len() && input.as_bytes()[*pos] != b'"' {
        *pos += 1;
    }
    if *pos >= input.len() {
        anyhow::bail!("unclosed `\"` in json pattern");
    }
    let content = input[start..*pos].to_string();
    *pos += 1; // skip closing "

    if content.contains('$') {
        Ok(vec![SelectStep::LeafPattern { pattern: content }])
    } else {
        Ok(vec![SelectStep::Key {
            name:    content,
            capture: None,
        }])
    }
}

fn parse_value_glob(input: &str, pos: &mut usize) -> anyhow::Result<Vec<SelectStep>> {
    let start = *pos;
    while *pos < input.len() {
        match input.as_bytes()[*pos] {
            b',' | b'}' | b']' => break,
            _ => *pos += 1,
        }
    }
    let text = input[start..*pos].trim();
    if text.is_empty() {
        anyhow::bail!("empty value in json pattern");
    }
    Ok(vec![SelectStep::Key {
        name:    text.to_string(),
        capture: None,
    }])
}

fn parse_entry(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
) -> anyhow::Result<(KeyMatcher, Vec<SelectStep>)> {
    skip_ws(input, pos);
    let key = parse_key(input, pos)?;
    skip_ws(input, pos);
    expect_byte(input, pos, b':')?;
    skip_ws(input, pos);
    let value = parse_pattern(input, pos, annotations)?;
    Ok((key, value))
}

fn parse_key(input: &str, pos: &mut usize) -> anyhow::Result<KeyMatcher> {
    skip_ws(input, pos);

    if input.as_bytes()[*pos] == b'"' {
        *pos += 1;
        let start = *pos;
        while *pos < input.len() && input.as_bytes()[*pos] != b'"' {
            *pos += 1;
        }
        if *pos >= input.len() {
            anyhow::bail!("unclosed `\"` in key position");
        }
        let content = &input[start..*pos];
        *pos += 1;
        return Ok(key_matcher_parse(content));
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
            anyhow::bail!("empty capture name after `$` in key position");
        }
        return Ok(KeyMatcher::Capture(name.to_string()));
    }

    let start = *pos;
    while *pos < input.len() && input.as_bytes()[*pos] != b':' {
        *pos += 1;
    }
    let key_str = input[start..*pos].trim();
    if key_str.is_empty() {
        anyhow::bail!("empty key in json pattern");
    }
    Ok(key_matcher_parse(key_str))
}

/// Classify a bare string into a KeyMatcher variant.
/// Mirrors `sprefa_rules::types::KeyMatcher::parse` without importing v1.
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

fn expect_byte(input: &str, pos: &mut usize, expected: u8) -> anyhow::Result<()> {
    match input.as_bytes().get(*pos) {
        Some(&b) if b == expected => {
            *pos += 1;
            Ok(())
        }
        Some(&b) => anyhow::bail!("expected {:?}, found {:?}", expected as char, b as char),
        None => anyhow::bail!("expected {:?}, found end of input", expected as char),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_object_with_captures() {
        let (steps, _annotations) = parse_body("{ name: $NAME }").unwrap();
        assert_eq!(steps.len(), 1);
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert_eq!(entries.len(), 1);
                assert!(matches!(&entries[0].key, KeyMatcher::Exact(s) if s == "name"));
                assert!(
                    matches!(&entries[0].value[0], SelectStep::Leaf { capture: Some(c) } if c == "NAME")
                );
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn nested_object() {
        let (steps, _annotations) = parse_body("{ package: { name: $NAME } }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert_eq!(entries.len(), 1);
                assert!(matches!(&entries[0].key, KeyMatcher::Exact(s) if s == "package"));
                match &entries[0].value[0] {
                    SelectStep::Object { entries: inner } => {
                        assert_eq!(inner.len(), 1);
                        assert!(matches!(&inner[0].key, KeyMatcher::Exact(s) if s == "name"));
                    }
                    _ => panic!("expected nested Object"),
                }
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn multi_entry_object() {
        let (steps, _annotations) = parse_body("{ repository: $REPO, tag: $TAG }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert_eq!(entries.len(), 2);
                assert!(matches!(&entries[0].key, KeyMatcher::Exact(s) if s == "repository"));
                assert!(matches!(&entries[1].key, KeyMatcher::Exact(s) if s == "tag"));
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn array_iteration() {
        let (steps, _annotations) = parse_body("{ members: [...$MEMBER] }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => match &entries[0].value[0] {
                SelectStep::Array { item } => {
                    assert!(
                        matches!(&item[0], SelectStep::Leaf { capture: Some(c) } if c == "MEMBER")
                    );
                }
                _ => panic!("expected Array"),
            },
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn recursive_descent() {
        let (steps, _annotations) =
            parse_body("{ **: { image: { repository: $REPO, tag: $TAG } } }").unwrap();
        assert!(matches!(&steps[0], SelectStep::Any));
        assert!(matches!(&steps[1], SelectStep::Object { .. }));
    }

    #[test]
    fn capture_key() {
        let (steps, _annotations) = parse_body("{ $K: $V }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Capture(s) if s == "K"));
                assert!(
                    matches!(&entries[0].value[0], SelectStep::Leaf { capture: Some(c) } if c == "V")
                );
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn wildcard_value() {
        let (steps, _annotations) = parse_body("{ deps: { $NAME: $_ } }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                match &entries[0].value[0] {
                    SelectStep::Object { entries: inner } => {
                        assert!(matches!(&inner[0].key, KeyMatcher::Capture(s) if s == "NAME"));
                        assert!(inner[0].value.is_empty());
                    }
                    _ => panic!("expected inner Object"),
                }
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn glob_key() {
        let (steps, _annotations) = parse_body("{ dep_*: $V }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Glob(s) if s == "dep_*"));
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn regex_key() {
        let (steps, _annotations) =
            parse_body("{ re:^(dev-)?dependencies: { $NAME: $_ } }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Glob(s) if s.starts_with("re:")));
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn quoted_value_pattern() {
        let (steps, _annotations) = parse_body(r#"{ image: "$REPO:$TAG" }"#).unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Exact(s) if s == "image"));
                assert!(
                    matches!(&entries[0].value[0], SelectStep::LeafPattern { pattern } if pattern == "$REPO:$TAG")
                );
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn quoted_key_pattern() {
        let (steps, _annotations) = parse_body(r#"{ "@$SCOPE/$NAME": $_ }"#).unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Glob(s) if s == "@$SCOPE/$NAME"));
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn quoted_literal_value() {
        let (steps, _annotations) = parse_body(r#"{ status: "active" }"#).unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(
                    matches!(&entries[0].value[0], SelectStep::Key { name, .. } if name == "active")
                );
            }
            _ => panic!("expected Object"),
        }
    }

    // ── annotation tests (v2 $$sigil grammar) ─────────────────────────────────

    #[test]
    fn scan_annotation_repo() {
        let (steps, annotations) =
            parse_body("{ repository: $$repo($REPO), tag: $$rev($TAG) }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert_eq!(entries.len(), 2);
                assert!(
                    matches!(&entries[0].value[0], SelectStep::Leaf { capture: Some(c) } if c == "REPO")
                );
                assert!(
                    matches!(&entries[1].value[0], SelectStep::Leaf { capture: Some(c) } if c == "TAG")
                );
            }
            _ => panic!("expected Object"),
        }
        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].var, "REPO");
        assert_eq!(&*annotations[0].sigil, "repo");
        assert_eq!(annotations[1].var, "TAG");
        assert_eq!(&*annotations[1].sigil, "rev");
    }

    #[test]
    fn scan_annotation_norm_variants() {
        let (_, annotations) =
            parse_body("{ repository: $$repo_norm($REPO), tag: $$rev_norm($TAG) }").unwrap();
        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].var, "REPO");
        assert_eq!(&*annotations[0].sigil, "repo_norm");
        assert_eq!(annotations[1].var, "TAG");
        assert_eq!(&*annotations[1].sigil, "rev_norm");
    }

    #[test]
    fn scan_annotation_accepts_unknown_sigil() {
        // Walker is permissive; lower-time validates against the registry.
        let (_, annotations) = parse_body("{ x: $$totally_bogus($X) }").unwrap();
        assert_eq!(annotations.len(), 1);
        assert_eq!(&*annotations[0].sigil, "totally_bogus");
        assert_eq!(annotations[0].var, "X");
    }

    #[test]
    fn no_annotations_without_wrapper() {
        let (_, annotations) = parse_body("{ repository: $REPO, tag: $TAG }").unwrap();
        assert!(annotations.is_empty());
    }

    #[test]
    fn annotation_rejects_object_inner() {
        let err = parse_body("{ x: $$repo({a:$X}) }").unwrap_err();
        assert!(
            err.to_string().contains("requires a bare capture var"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn cross_ref_token_parses_as_capture_leaf() {
        // ${rule.$VAR} lowers to a Leaf that captures under VAR. Runtime
        // seed comes from `expand_xrefs`; walker constrains-when-prebound.
        let (steps, _) = parse_body("{ tag: ${base_rule.$TAG} }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert_eq!(entries.len(), 1);
                match entries[0].value.as_slice() {
                    [SelectStep::Leaf { capture: Some(c) }] if c == "TAG" => {}
                    other => panic!("expected Leaf capturing TAG, got {other:?}"),
                }
            }
            other => panic!("expected Object, got {other:?}"),
        }
    }

    #[test]
    fn annotation_rejects_wildcard_inner() {
        let err = parse_body("{ x: $$rev($_) }").unwrap_err();
        assert!(
            err.to_string().contains("requires a bare capture var"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn annotation_rejects_bare_word_inner() {
        let err = parse_body("{ x: $$repo(foo) }").unwrap_err();
        assert!(
            err.to_string().contains("requires a bare capture var"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn annotation_multi_in_object() {
        let (steps, annotations) =
            parse_body("{ a: $$repo($R), b: $$rev_norm($V) }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert_eq!(entries.len(), 2);
                assert!(
                    matches!(&entries[0].value[0], SelectStep::Leaf { capture: Some(c) } if c == "R")
                );
                assert!(
                    matches!(&entries[1].value[0], SelectStep::Leaf { capture: Some(c) } if c == "V")
                );
            }
            _ => panic!("expected Object"),
        }
        assert_eq!(annotations.len(), 2);
        assert_eq!(&*annotations[0].sigil, "repo");
        assert_eq!(&*annotations[1].sigil, "rev_norm");
    }

    // `${VAR}` is a synonym of `$VAR` — both lower to the same Leaf capture.
    #[test]
    fn braced_capture_synonym() {
        let (bare, _)   = parse_body("{ name: $NAME }").unwrap();
        let (braced, _) = parse_body("{ name: ${NAME} }").unwrap();
        assert_eq!(format!("{bare:?}"), format!("{braced:?}"));
    }

    // `${rule.$VAR}` lowers to a Leaf capturing under `VAR`. At runtime
    // `expand_xrefs` seeds `VAR` from the target rule, and the walker's
    // constrain-when-prebound logic filters non-matching branches. The
    // `rule` component is informational at parse time (drives the DAG) and
    // doesn't appear in the lowered SelectStep.
    #[test]
    fn braced_crossref_lowers_to_leaf_capture() {
        let (steps, _) = parse_body("{ name: ${other.$TAG} }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(
                    &entries[0].value[0],
                    SelectStep::Leaf { capture: Some(c) } if c == "TAG"
                ));
            }
            _ => panic!("expected Object"),
        }
    }
}
