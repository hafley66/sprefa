/// .sprf text parser.
///
/// Parses the outer structure: statements, selector chains, slots.
/// Does NOT parse the inside of json() bodies -- that's _2_pattern.rs.
use crate::_0_ast::{Program, Statement, SelectorChain, Slot, Tag};

pub fn parse_program(input: &str) -> anyhow::Result<Program> {
    let mut stmts = vec![];
    let mut remaining = input;

    loop {
        remaining = skip_ws_and_comments(remaining);
        if remaining.is_empty() {
            break;
        }
        let (chain, rest) = parse_rule(remaining)?;
        stmts.push(Statement::Rule(chain));
        remaining = rest;
    }

    Ok(stmts)
}

/// Skip whitespace and `# ...` line comments.
fn skip_ws_and_comments(mut input: &str) -> &str {
    loop {
        input = input.trim_start();
        if input.starts_with('#') {
            match input.find('\n') {
                Some(pos) => input = &input[pos + 1..],
                None => return "",
            }
        } else {
            return input;
        }
    }
}

/// Parse one rule: `slot > slot > ... ;`
/// Returns (SelectorChain, remaining input).
fn parse_rule(input: &str) -> anyhow::Result<(SelectorChain, &str)> {
    let mut slots = vec![];
    let mut remaining = input;

    loop {
        remaining = skip_ws_and_comments(remaining);
        if remaining.is_empty() {
            anyhow::bail!("unexpected end of input, expected `;`");
        }

        let (slot, rest) = parse_slot(remaining)?;
        slots.push(slot);
        remaining = skip_ws_and_comments(rest);

        if remaining.starts_with(';') {
            remaining = &remaining[1..];
            break;
        } else if remaining.starts_with('>') {
            remaining = &remaining[1..];
        } else if remaining.is_empty() {
            anyhow::bail!("unexpected end of input, expected `>` or `;`");
        } else {
            anyhow::bail!(
                "expected `>` or `;`, found {:?}",
                &remaining[..remaining.len().min(20)]
            );
        }
    }

    if slots.is_empty() {
        anyhow::bail!("empty rule");
    }

    Ok((SelectorChain { slots }, remaining))
}

/// Parse one slot: either `tag[arg](body)`, `tag(body)`, or a bare glob.
fn parse_slot(input: &str) -> anyhow::Result<(Slot, &str)> {
    let input = skip_ws_and_comments(input);

    // Try to parse as tagged: word followed by `(` or `[`
    if let Some((tag, rest)) = try_parse_tag(input) {
        let rest = rest.trim_start();

        // Optional [arg]
        let (arg, rest) = if rest.starts_with('[') {
            let (arg_str, rest) = parse_bracketed(&rest[1..])?;
            (Some(arg_str), rest)
        } else {
            (None, rest)
        };

        let rest = rest.trim_start();
        if !rest.starts_with('(') {
            anyhow::bail!("expected `(` after tag `{:?}`", tag);
        }

        let (body, rest) = parse_paren_body(&rest[1..])?;
        Ok((Slot::Tagged { tag, arg, body }, rest))
    } else {
        // Bare glob: everything up to unbalanced `>`, `;`, or end
        let (glob, rest) = parse_bare_glob(input)?;
        Ok((Slot::Bare(glob), rest))
    }
}

/// If the input starts with a known tag name followed by `(` or `[`, return
/// the tag and the remaining input after the tag name.
fn try_parse_tag(input: &str) -> Option<(Tag, &str)> {
    // Tags are alphabetic, followed by ( or [
    let word_end = input
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(input.len());
    if word_end == 0 {
        return None;
    }
    let word = &input[..word_end];
    let rest = &input[word_end..];
    let rest_trimmed = rest.trim_start();

    // Only parse as tag if followed by ( or [
    if !rest_trimmed.starts_with('(') && !rest_trimmed.starts_with('[') {
        return None;
    }

    Tag::from_str(word).map(|t| (t, rest))
}

/// Parse content between `[` and `]` (bracket already consumed).
fn parse_bracketed(input: &str) -> anyhow::Result<(String, &str)> {
    match input.find(']') {
        Some(pos) => Ok((input[..pos].trim().to_string(), &input[pos + 1..])),
        None => anyhow::bail!("unclosed `[`"),
    }
}

/// Parse a paren-counted body (opening `(` already consumed).
/// Counts nested parens. Returns body content and remaining input after closing `)`.
fn parse_paren_body(input: &str) -> anyhow::Result<(String, &str)> {
    let mut depth: u32 = 1;
    let mut pos = 0;
    let bytes = input.as_bytes();

    while pos < bytes.len() {
        match bytes[pos] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let body = input[..pos].trim().to_string();
                    return Ok((body, &input[pos + 1..]));
                }
            }
            b'\\' => {
                // Skip escaped character
                pos += 1;
            }
            _ => {}
        }
        pos += 1;
    }

    anyhow::bail!("unclosed `(` -- paren counting reached end of input");
}

/// Parse a bare glob: content up to the next unquoted `>` or `;` at the top
/// level (not inside parens/brackets/quotes).
fn parse_bare_glob(input: &str) -> anyhow::Result<(String, &str)> {
    let mut pos = 0;
    let bytes = input.as_bytes();

    while pos < bytes.len() {
        match bytes[pos] {
            b'>' | b';' => {
                let glob = input[..pos].trim().to_string();
                if glob.is_empty() {
                    anyhow::bail!("empty slot");
                }
                return Ok((glob, &input[pos..]));
            }
            b'"' => {
                // Skip quoted string
                pos += 1;
                while pos < bytes.len() && bytes[pos] != b'"' {
                    if bytes[pos] == b'\\' {
                        pos += 1;
                    }
                    pos += 1;
                }
                // Skip closing quote
            }
            _ => {}
        }
        pos += 1;
    }

    // End of input with no terminator -- bare glob at end is ok, caller
    // will complain about missing `;`
    let glob = input.trim().to_string();
    if glob.is_empty() {
        anyhow::bail!("empty slot");
    }
    Ok((glob, ""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_ast::Tag;

    #[test]
    fn parse_tagged_fs_json() {
        let input = "fs(**/Cargo.toml) > json({ package: { name: $NAME } });";
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Rule(chain) = &program[0];
        assert_eq!(chain.slots.len(), 2);

        match &chain.slots[0] {
            Slot::Tagged { tag, arg, body } => {
                assert_eq!(*tag, Tag::Fs);
                assert!(arg.is_none());
                assert_eq!(body, "**/Cargo.toml");
            }
            _ => panic!("expected Tagged"),
        }
        match &chain.slots[1] {
            Slot::Tagged { tag, arg, body } => {
                assert_eq!(*tag, Tag::Json);
                assert!(arg.is_none());
                assert_eq!(body, "{ package: { name: $NAME } }");
            }
            _ => panic!("expected Tagged"),
        }
    }

    #[test]
    fn parse_bare_three_then_tagged() {
        let input = "my-org/* > main|release/* > **/Cargo.toml > json({ deps: { $K: $_ } });";
        let program = parse_program(input).unwrap();
        let Statement::Rule(chain) = &program[0];
        assert_eq!(chain.slots.len(), 4);

        match &chain.slots[0] {
            Slot::Bare(g) => assert_eq!(g, "my-org/*"),
            _ => panic!("expected Bare"),
        }
        match &chain.slots[1] {
            Slot::Bare(g) => assert_eq!(g, "main|release/*"),
            _ => panic!("expected Bare"),
        }
        match &chain.slots[2] {
            Slot::Bare(g) => assert_eq!(g, "**/Cargo.toml"),
            _ => panic!("expected Bare"),
        }
    }

    #[test]
    fn parse_ast_with_lang_arg() {
        let input = "fs(**/*.config) > ast[typescript](import $NAME from '$PATH');";
        let program = parse_program(input).unwrap();
        let Statement::Rule(chain) = &program[0];
        assert_eq!(chain.slots.len(), 2);

        match &chain.slots[1] {
            Slot::Tagged { tag, arg, body } => {
                assert_eq!(*tag, Tag::Ast);
                assert_eq!(arg.as_deref(), Some("typescript"));
                assert_eq!(body, "import $NAME from '$PATH'");
            }
            _ => panic!("expected Tagged"),
        }
    }

    #[test]
    fn parse_nested_parens_in_re() {
        let input = r"fs(helm/**/*.yaml) > re(image:\s+(?P<REPO>[^:]+):(?P<TAG>.+));";
        let program = parse_program(input).unwrap();
        let Statement::Rule(chain) = &program[0];

        match &chain.slots[1] {
            Slot::Tagged { tag, body, .. } => {
                assert_eq!(*tag, Tag::Re);
                assert_eq!(body, r"image:\s+(?P<REPO>[^:]+):(?P<TAG>.+)");
            }
            _ => panic!("expected Tagged"),
        }
    }

    #[test]
    fn parse_comments_and_multiple_rules() {
        let input = r#"
# first rule
fs(**/package.json) > json({ name: $NAME });

# second rule
fs(**/Cargo.toml) > json({ package: { name: $N } });
"#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 2);
    }

    #[test]
    fn unclosed_paren_errors() {
        let input = "fs(**/foo > json({ x: $Y });";
        let err = parse_program(input).unwrap_err();
        assert!(err.to_string().contains("unclosed"), "{}", err);
    }

    #[test]
    fn missing_semicolon_errors() {
        let input = "fs(**/foo)";
        let err = parse_program(input).unwrap_err();
        assert!(err.to_string().contains("`;`") || err.to_string().contains("end of input"), "{}", err);
    }
}
