/// .sprf text parser.
///
/// Parses the outer structure: statements, selector chains, slots.
/// Does NOT parse the inside of json() bodies -- that's _2_pattern.rs.
use crate::_0_ast::{LinkDecl, Program, Statement, SelectorChain, Slot, Tag};

pub fn parse_program(input: &str) -> anyhow::Result<Program> {
    let mut stmts = vec![];
    let mut remaining = input;

    loop {
        remaining = skip_ws_and_comments(remaining);
        if remaining.is_empty() {
            break;
        }
        let (stmt, rest) = parse_statement(remaining)?;
        stmts.push(stmt);
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

/// Dispatch: `link(...)` or a normal rule.
fn parse_statement(input: &str) -> anyhow::Result<(Statement, &str)> {
    let trimmed = skip_ws_and_comments(input);
    if trimmed.starts_with("link") {
        let after = &trimmed[4..];
        let after_trimmed = after.trim_start();
        if after_trimmed.starts_with('(') {
            let (decl, rest) = parse_link_decl(after_trimmed)?;
            return Ok((Statement::Link(decl), rest));
        }
    }
    let (chain, rest) = parse_rule(trimmed)?;
    Ok((Statement::Rule(chain), rest))
}

/// Parse `link(src > tgt, pred, ...) > $kind;`
fn parse_link_decl(input: &str) -> anyhow::Result<(LinkDecl, &str)> {
    // input starts with `(`
    let (body, rest) = parse_paren_body(&input[1..])?;

    // Parse body: src_kind > tgt_kind, pred, pred, ...
    let (kinds_part, preds_part) = match body.find(',') {
        Some(pos) => (&body[..pos], Some(body[pos + 1..].trim())),
        None => (body.as_str(), None),
    };

    let (src_kind, tgt_kind) = match kinds_part.find('>') {
        Some(pos) => (kinds_part[..pos].trim(), kinds_part[pos + 1..].trim()),
        None => anyhow::bail!("link body must contain `src > tgt`, found {:?}", kinds_part),
    };

    let predicates: Vec<String> = match preds_part {
        Some(p) if !p.is_empty() => p.split(',').map(|s| s.trim().to_string()).collect(),
        _ => vec![],
    };

    // Optional `> $kind_name`
    let rest = rest.trim_start();
    let (kind_name, rest) = if rest.starts_with('>') {
        let rest = rest[1..].trim_start();
        if !rest.starts_with('$') {
            anyhow::bail!("expected `$KIND_NAME` after `>` in link declaration");
        }
        let rest = &rest[1..];
        let end = rest.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let name = rest[..end].to_string();
        if name.is_empty() {
            anyhow::bail!("empty kind name after `$` in link declaration");
        }
        (Some(name), &rest[end..])
    } else {
        (None, rest)
    };

    let rest = rest.trim_start();
    if !rest.starts_with(';') {
        anyhow::bail!("expected `;` after link declaration");
    }

    Ok((
        LinkDecl {
            src_kind: src_kind.to_string(),
            tgt_kind: tgt_kind.to_string(),
            predicates,
            kind_name,
        },
        &rest[1..],
    ))
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

/// Parse one slot: `match($CAP, kind)`, `tag[arg](body)`, `tag(body)`, or bare glob.
fn parse_slot(input: &str) -> anyhow::Result<(Slot, &str)> {
    let input = skip_ws_and_comments(input);

    // Check for match() slot
    if let Some(rest) = try_parse_match_keyword(input) {
        let rest = rest.trim_start();
        if rest.starts_with('(') {
            let (body, rest) = parse_paren_body(&rest[1..])?;
            let (capture, kind) = parse_match_body(&body)?;
            return Ok((Slot::Match { capture, kind }, rest));
        }
    }

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

/// Check if input starts with `match` followed by `(`.
fn try_parse_match_keyword(input: &str) -> Option<&str> {
    if input.starts_with("match") {
        let rest = &input[5..];
        let rest_trimmed = rest.trim_start();
        if rest_trimmed.starts_with('(') {
            return Some(rest);
        }
    }
    None
}

/// Parse the body of `match($CAPTURE, kind)`.
fn parse_match_body(body: &str) -> anyhow::Result<(String, String)> {
    let body = body.trim();
    let comma = body.find(',')
        .ok_or_else(|| anyhow::anyhow!("match() requires `$CAPTURE, kind`"))?;
    let cap_part = body[..comma].trim();
    let kind_part = body[comma + 1..].trim();

    if !cap_part.starts_with('$') {
        anyhow::bail!("match() capture must start with `$`, found {:?}", cap_part);
    }
    let capture = cap_part[1..].to_string();
    if capture.is_empty() {
        anyhow::bail!("empty capture name in match()");
    }

    if kind_part.is_empty() {
        anyhow::bail!("empty kind in match()");
    }

    Ok((capture, kind_part.to_string()))
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
        let Statement::Rule(chain) = &program[0] else { panic!("expected Rule") };
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
        let Statement::Rule(chain) = &program[0] else { panic!("expected Rule") };
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
        let Statement::Rule(chain) = &program[0] else { panic!("expected Rule") };
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
        let Statement::Rule(chain) = &program[0] else { panic!("expected Rule") };

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

    #[test]
    fn parse_match_slot() {
        let input = "fs(**/Cargo.toml) > json({ package: { name: $NAME } }) > match($NAME, package_name);";
        let program = parse_program(input).unwrap();
        let Statement::Rule(chain) = &program[0] else { panic!("expected Rule") };
        assert_eq!(chain.slots.len(), 3);

        match &chain.slots[2] {
            Slot::Match { capture, kind } => {
                assert_eq!(capture, "NAME");
                assert_eq!(kind, "package_name");
            }
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn parse_multiple_match_slots() {
        let input = "fs(**/package.json) > json({ dependencies: { $NAME: $VER } }) > match($NAME, dep_name) > match($VER, dep_version);";
        let program = parse_program(input).unwrap();
        let Statement::Rule(chain) = &program[0] else { panic!("expected Rule") };
        assert_eq!(chain.slots.len(), 4);
        assert!(matches!(&chain.slots[2], Slot::Match { capture, kind } if capture == "NAME" && kind == "dep_name"));
        assert!(matches!(&chain.slots[3], Slot::Match { capture, kind } if capture == "VER" && kind == "dep_version"));
    }

    #[test]
    fn parse_link_decl() {
        let input = "link(dep_name > package_name, norm_eq) > $dep_to_package;";
        let program = parse_program(input).unwrap();
        let Statement::Link(decl) = &program[0] else { panic!("expected Link") };
        assert_eq!(decl.src_kind, "dep_name");
        assert_eq!(decl.tgt_kind, "package_name");
        assert_eq!(decl.predicates, vec!["norm_eq"]);
        assert_eq!(decl.kind_name.as_deref(), Some("dep_to_package"));
    }

    #[test]
    fn parse_link_multiple_predicates() {
        let input = "link(import_name > export_name, target_file_eq, string_eq) > $import_binding;";
        let program = parse_program(input).unwrap();
        let Statement::Link(decl) = &program[0] else { panic!("expected Link") };
        assert_eq!(decl.predicates, vec!["target_file_eq", "string_eq"]);
    }

    #[test]
    fn parse_link_no_kind_name() {
        let input = "link(dep_name > package_name, norm_eq);";
        let program = parse_program(input).unwrap();
        let Statement::Link(decl) = &program[0] else { panic!("expected Link") };
        assert!(decl.kind_name.is_none());
    }

    #[test]
    fn parse_mixed_rules_and_links() {
        let input = r#"
            fs(**/Cargo.toml) > json({ package: { name: $NAME } }) > match($NAME, package_name);
            link(dep_name > package_name, norm_eq) > $dep_to_package;
        "#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 2);
        assert!(matches!(&program[0], Statement::Rule(_)));
        assert!(matches!(&program[1], Statement::Link(_)));
    }
}
