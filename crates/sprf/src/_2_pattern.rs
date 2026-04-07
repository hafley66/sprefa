/// json() body parser.
///
/// Parses the string inside `json(...)` into a Vec<SelectStep> tree
/// that the existing walk engine can execute.
///
/// Grammar:
///   pattern    = annotation | object | array | capture | wildcard | value_glob
///   annotation = ("repo" | "rev") "(" pattern ")"
///   object     = "{" (entry ("," entry)*)? "}"
///   entry      = key ":" pattern
///   key        = "**" | "$" SCREAMING | "$_" | "re:" REGEX | glob_str
///   array      = "[" "..." pattern "]"
///   capture    = "$" SCREAMING
///   wildcard   = "$_"
///   value_glob = (not , } ] )+
use sprefa_rules::types::{KeyMatcher, ObjectEntry, SelectStep};

/// A scan annotation discovered during json pattern parsing.
/// Records that a captured variable drives demand scanning.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanAnnotation {
    /// The capture variable name (e.g. "REPO").
    pub var: String,
    /// "repo" or "rev".
    pub kind: String,
}

pub fn parse_json_body(input: &str) -> anyhow::Result<(Vec<SelectStep>, Vec<ScanAnnotation>)> {
    let mut pos = 0;
    let mut annotations = Vec::new();
    let steps = parse_pattern(input.trim(), &mut pos, &mut annotations)?;
    let remaining = input[pos..].trim();
    if !remaining.is_empty() {
        anyhow::bail!("unexpected trailing content in json body: {:?}", remaining);
    }
    Ok((steps, annotations))
}

/// Parse a pattern, returning a Vec<SelectStep> (the sub-chain for this position).
fn parse_pattern(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
) -> anyhow::Result<Vec<SelectStep>> {
    skip_ws(input, pos);
    if *pos >= input.len() {
        anyhow::bail!("unexpected end of json pattern");
    }

    // Check for repo(...) or rev(...) annotation wrapper in value position.
    for kind in &["repo", "rev"] {
        if input[*pos..].starts_with(kind) && input.as_bytes().get(*pos + kind.len()) == Some(&b'(')
        {
            *pos += kind.len() + 1; // skip "repo(" or "rev("
            let inner = parse_pattern(input, pos, annotations)?;
            skip_ws(input, pos);
            expect_byte(input, pos, b')')?;
            // Record annotation for any captures found in the inner pattern.
            for step in &inner {
                if let SelectStep::Leaf {
                    capture: Some(var), ..
                } = step
                {
                    annotations.push(ScanAnnotation {
                        var: var.clone(),
                        kind: kind.to_string(),
                    });
                }
            }
            return Ok(inner);
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

/// Parse `{ entry, entry, ... }`.
/// Returns the SelectStep tree: may be a single Object step, or
/// if a `**` key is present, an Any step followed by the sub-pattern.
fn parse_object(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
) -> anyhow::Result<Vec<SelectStep>> {
    expect_byte(input, pos, b'{')?;
    skip_ws(input, pos);

    let mut entries: Vec<ObjectEntry> = vec![];

    // Empty object
    if peek_byte(input, *pos) == Some(b'}') {
        *pos += 1;
        return Ok(vec![SelectStep::Object { entries }]);
    }

    loop {
        skip_ws(input, pos);
        let (key, value_steps) = parse_entry(input, pos, annotations)?;

        // ** key means recursive descent: lower to Any + sub-pattern
        if matches!(&key, KeyMatcher::Exact(s) if s == "**") {
            // Wrap remaining object entries (if any after this) into a
            // single Object step, then prepend Any.
            // But ** is a single entry -- its value is the descent target.
            // Return [Any, Object { entries so far }, ...value_steps]
            // Actually: ** as key means "descend any depth, then match value".
            // If there are other entries alongside **, they are at the same
            // level. So we split: non-** entries become an Object step,
            // ** entry becomes Any + value_steps appended.
            //
            // For now: ** must be the only entry in its object.
            // This is how the PLAN specifies it.
            skip_ws(input, pos);
            // consume optional comma
            if peek_byte(input, *pos) == Some(b',') {
                *pos += 1;
            }
            skip_ws(input, pos);
            expect_byte(input, pos, b'}')?;

            let mut steps = vec![SelectStep::Any];
            // If we already accumulated entries, wrap them first
            if !entries.is_empty() {
                let mut result = vec![SelectStep::Object { entries }];
                result.push(SelectStep::Any);
                result.extend(value_steps);
                return Ok(result);
            }
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

/// Parse `[ ... pattern ]` (array iteration).
fn parse_array(
    input: &str,
    pos: &mut usize,
    annotations: &mut Vec<ScanAnnotation>,
) -> anyhow::Result<Vec<SelectStep>> {
    expect_byte(input, pos, b'[')?;
    skip_ws(input, pos);

    // Expect `...`
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

/// Parse `$NAME` (capture) or `$_` (wildcard).
fn parse_capture_or_wildcard(input: &str, pos: &mut usize) -> anyhow::Result<Vec<SelectStep>> {
    *pos += 1; // skip $
    if *pos >= input.len() {
        anyhow::bail!("unexpected end after `$`");
    }

    if input.as_bytes()[*pos] == b'_'
        && (*pos + 1 >= input.len() || !input.as_bytes()[*pos + 1].is_ascii_alphanumeric())
    {
        *pos += 1;
        // Empty steps = "match succeeded here" = any value, any shape.
        // Unlike Leaf which only matches scalars.
        return Ok(vec![]);
    }

    // Screaming capture: $NAME
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

/// Parse a quoted value pattern: `"$REPO:$TAG"` or `"@$SCOPE/$NAME"`.
///
/// If the content contains `$VAR`, compiles to `LeafPattern` (segment capture).
/// If no captures, compiles to a literal key descent.
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
        // No captures -- literal value match. Treat as key descent.
        Ok(vec![SelectStep::Key {
            name: content,
            capture: None,
        }])
    }
}

/// Parse a value glob: bare string up to `,`, `}`, `]`, or end.
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
    // A value glob is a leaf match by exact string or glob pattern.
    // For now, treat it as a Leaf with no capture (match but don't bind).
    // The walk engine's Leaf step captures the value if it matches.
    // TODO: glob matching on leaf values would need a new step type.
    // For now bare strings in value position are literal key descents.
    Ok(vec![SelectStep::Key {
        name: text.to_string(),
        capture: None,
    }])
}

/// Parse one `key: pattern` entry.
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

/// Parse a key: `**`, `$CAP`, `$_`, `re:REGEX`, `"quoted"`, or a bare glob string.
fn parse_key(input: &str, pos: &mut usize) -> anyhow::Result<KeyMatcher> {
    skip_ws(input, pos);

    // Quoted key: `"@$SCOPE/$NAME"` or `"literal"`
    if input.as_bytes()[*pos] == b'"' {
        *pos += 1; // skip opening "
        let start = *pos;
        while *pos < input.len() && input.as_bytes()[*pos] != b'"' {
            *pos += 1;
        }
        if *pos >= input.len() {
            anyhow::bail!("unclosed `\"` in key position");
        }
        let content = &input[start..*pos];
        *pos += 1; // skip closing "
        return Ok(KeyMatcher::parse(content));
    }

    // ** recursive descent
    if input[*pos..].starts_with("**")
        && (*pos + 2 >= input.len() || !input.as_bytes()[*pos + 2].is_ascii_alphanumeric())
    {
        *pos += 2;
        return Ok(KeyMatcher::Exact("**".to_string()));
    }

    // re: prefix
    if input[*pos..].starts_with("re:") {
        *pos += 3;
        let start = *pos;
        // Read until `:` (the key-value separator). Since regex can contain `:`,
        // we need to be careful. The convention: re: key ends at the LAST `:`
        // before whitespace-then-pattern. Simpler: read until we see `: ` or `:`
        // followed by whitespace or `{` or `$`.
        // Actually: the regex key ends at the next unescaped `:` that is followed
        // by whitespace or a pattern start character.
        // Simplest correct approach: scan for `:` where the char after it looks
        // like a value start (whitespace, `{`, `[`, `$`, or alphanumeric).
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

    // $_ wildcard or $CAP capture
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

    // Bare key: read until `:` (the key-value separator)
    let start = *pos;
    while *pos < input.len() && input.as_bytes()[*pos] != b':' {
        *pos += 1;
    }
    let key_str = input[start..*pos].trim();
    if key_str.is_empty() {
        anyhow::bail!("empty key in json pattern");
    }
    Ok(KeyMatcher::parse(key_str))
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
        let (steps, _annotations) = parse_json_body("{ name: $NAME }").unwrap();
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
        let (steps, _annotations) = parse_json_body("{ package: { name: $NAME } }").unwrap();
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
        let (steps, _annotations) = parse_json_body("{ repository: $REPO, tag: $TAG }").unwrap();
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
        let (steps, _annotations) = parse_json_body("{ members: [...$MEMBER] }").unwrap();
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
        let (steps, _annotations) = parse_json_body("{ **: { image: { repository: $REPO, tag: $TAG } } }").unwrap();
        assert!(matches!(&steps[0], SelectStep::Any));
        assert!(matches!(&steps[1], SelectStep::Object { .. }));
    }

    #[test]
    fn capture_key() {
        let (steps, _annotations) = parse_json_body("{ $K: $V }").unwrap();
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
        let (steps, _annotations) = parse_json_body("{ deps: { $NAME: $_ } }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                match &entries[0].value[0] {
                    SelectStep::Object { entries: inner } => {
                        assert!(matches!(&inner[0].key, KeyMatcher::Capture(s) if s == "NAME"));
                        // $_ produces empty steps = match any value shape
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
        let (steps, _annotations) = parse_json_body("{ dep_*: $V }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Glob(s) if s == "dep_*"));
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn regex_key() {
        let (steps, _annotations) = parse_json_body("{ re:^(dev-)?dependencies: { $NAME: $_ } }").unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(matches!(&entries[0].key, KeyMatcher::Glob(s) if s.starts_with("re:")));
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn quoted_value_pattern() {
        let (steps, _annotations) = parse_json_body(r#"{ image: "$REPO:$TAG" }"#).unwrap();
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
        let (steps, _annotations) = parse_json_body(r#"{ "@$SCOPE/$NAME": $_ }"#).unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(
                    matches!(&entries[0].key, KeyMatcher::Glob(s) if s == "@$SCOPE/$NAME")
                );
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn quoted_literal_value() {
        // No $VAR -- treated as literal key descent
        let (steps, _annotations) = parse_json_body(r#"{ status: "active" }"#).unwrap();
        match &steps[0] {
            SelectStep::Object { entries } => {
                assert!(
                    matches!(&entries[0].value[0], SelectStep::Key { name, .. } if name == "active")
                );
            }
            _ => panic!("expected Object"),
        }
    }

    #[test]
    fn scan_annotation_repo() {
        let (steps, annotations) =
            parse_json_body("{ repository: repo($REPO), tag: rev($TAG) }").unwrap();
        // Steps should be a normal Object with two Leaf captures.
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
        // Annotations should record both.
        assert_eq!(annotations.len(), 2);
        assert_eq!(
            annotations[0],
            ScanAnnotation {
                var: "REPO".into(),
                kind: "repo".into()
            }
        );
        assert_eq!(
            annotations[1],
            ScanAnnotation {
                var: "TAG".into(),
                kind: "rev".into()
            }
        );
    }

    #[test]
    fn scan_annotation_nested() {
        let (_, annotations) =
            parse_json_body("{ image: { repository: repo($REPO), tag: rev($TAG) } }").unwrap();
        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].var, "REPO");
        assert_eq!(annotations[1].var, "TAG");
    }

    #[test]
    fn no_annotations_without_wrapper() {
        let (_, annotations) =
            parse_json_body("{ repository: $REPO, tag: $TAG }").unwrap();
        assert!(annotations.is_empty());
    }
}
