/// .sprf text parser.
///
/// Parses the outer structure: rule declarations with scoped bodies.
/// Does NOT parse the inside of json() bodies -- that's _2_pattern.rs.
use crate::_0_ast::{
    CrossRef, CrossRefBinding, Program, RuleBody, RuleDecl, Slot, Statement, Tag,
};

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

/// Dispatch: `rule ...` only.
fn parse_statement(input: &str) -> anyhow::Result<(Statement, &str)> {
    let trimmed = skip_ws_and_comments(input);

    // rule(name) { body };
    if let Some(after) = strip_rule_keyword(trimmed) {
        let (decl, rest) = parse_rule_decl(after)?;
        return Ok((Statement::Rule(decl), rest));
    }

    anyhow::bail!(
        "expected `rule`, found {:?}",
        &trimmed[..trimmed.len().min(30)]
    );
}

/// Check if input starts with `rule(`.
fn strip_rule_keyword(input: &str) -> Option<&str> {
    if !input.starts_with("rule") {
        return None;
    }
    let after = input[4..].trim_start();
    if after.starts_with('(') {
        Some(after)
    } else {
        None
    }
}

/// Parse `rule(name) { body };`
fn parse_rule_decl(input: &str) -> anyhow::Result<(RuleDecl, &str)> {
    // Input starts after "rule", at the `(name)` part
    if !input.starts_with('(') {
        anyhow::bail!("expected `(` after `rule`");
    }
    let (name, rest) = parse_paren_body(&input[1..])?;
    let name = name.trim().to_string();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        anyhow::bail!("invalid rule name: {:?}", name);
    }

    let rest = rest.trim_start();
    if !rest.starts_with('{') {
        anyhow::bail!("expected `{{` after `rule({})`", name);
    }

    // Parse body inside braces
    let (body, rest) = parse_block_contents(&rest[1..])?;

    // Expect `;` after `}`
    let rest = rest.trim_start();
    if !rest.starts_with(';') {
        anyhow::bail!("expected `;` after `rule({}) {{ ... }}`", name);
    }
    let rest = &rest[1..];

    Ok((RuleDecl { name, body }, rest))
}

/// Parse a list of rule body items.
///
/// A body is a sequence of RuleBody items (steps or blocks), terminated by `;`.
///
/// Example flat chain:
/// ```sprf
/// fs(**/Cargo.toml) > json({ ... }) > line($REPO:$TAG);
/// ```
///
/// Example scoped block:
/// ```sprf
/// repo($R) {
///   rev(main) {
///     fs(**/Cargo.toml) > json({ ... })
///   }
/// };
/// ```
fn parse_rule_body_list(input: &str) -> anyhow::Result<(Vec<RuleBody>, &str)> {
    let mut items = vec![];
    let mut remaining = input;

    loop {
        remaining = skip_ws_and_comments(remaining);

        if remaining.is_empty() {
            anyhow::bail!("unexpected end of input, expected `;`");
        }

        // Check for terminator
        if remaining.starts_with(';') {
            return Ok((items, &remaining[1..]));
        }

        // Check for end of block (if we're inside one)
        if remaining.starts_with('}') {
            // Don't consume }, let caller handle it
            return Ok((items, remaining));
        }

        // Try cross-ref first (non-tag identifier with col: $VAR bindings)
        if let Some((cref, rest)) = try_parse_cross_ref(remaining)? {
            items.push(cref);
            remaining = rest;
            continue;
        }

        // Try block (tag or bare slot followed by {)
        if let Some((block, rest)) = try_parse_block(remaining)? {
            items.push(block);
            remaining = rest;
            continue;
        }

        // Otherwise parse as a step (single slot or chain)
        let (step, rest) = parse_step_chain(remaining)?;
        items.push(step);
        remaining = rest;
    }
}

/// Try to parse a cross-rule reference: `rulename(col: $VAR, ...) { children }`
/// or `rulename(col: $VAR, ...)` (no block).
/// Returns None if this doesn't look like a cross-ref.
fn try_parse_cross_ref(input: &str) -> anyhow::Result<Option<(RuleBody, &str)>> {
    // Must start with an identifier that is NOT a known tag
    let word_end = input
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(input.len());
    if word_end == 0 {
        return Ok(None);
    }
    let word = &input[..word_end];

    // If it's a known tag, not a cross-ref
    if Tag::from_str(word).is_some() {
        return Ok(None);
    }

    let after_word = input[word_end..].trim_start();
    if !after_word.starts_with('(') {
        return Ok(None);
    }

    // Parse the paren body and check if it contains `col: $VAR` patterns
    let (paren_body, after_paren) = parse_paren_body(&after_word[1..])?;

    // Try to parse as cross-ref bindings (col: $VAR, col: $VAR)
    let bindings = match parse_cross_ref_bindings(&paren_body) {
        Ok(b) if !b.is_empty() => b,
        _ => return Ok(None),
    };

    let cross_ref = CrossRef {
        rule_name: word.to_string(),
        bindings,
    };

    let after_paren = skip_ws_and_comments(after_paren);

    // Optional block
    if after_paren.starts_with('{') {
        let (children, rest) = parse_block_contents(&after_paren[1..])?;
        Ok(Some((RuleBody::Ref { cross_ref, children }, rest)))
    } else {
        Ok(Some((RuleBody::Ref { cross_ref, children: vec![] }, after_paren)))
    }
}

/// Parse `col: $VAR, col: $VAR` bindings inside a cross-ref.
fn parse_cross_ref_bindings(input: &str) -> anyhow::Result<Vec<CrossRefBinding>> {
    let mut bindings = vec![];
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let colon = part
            .find(':')
            .ok_or_else(|| anyhow::anyhow!("expected `col: $VAR` in cross-ref, got {:?}", part))?;
        let column = part[..colon].trim().to_string();
        let var_str = part[colon + 1..].trim();
        if !var_str.starts_with('$') {
            anyhow::bail!("cross-ref binding value must be `$VAR`, got {:?}", var_str);
        }
        let var = var_str[1..].to_string();
        if var.is_empty()
            || !var
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        {
            anyhow::bail!("cross-ref variable must be SCREAMING_CASE: {:?}", var_str);
        }
        bindings.push(CrossRefBinding { column, var });
    }
    Ok(bindings)
}

/// Try to parse a block: `slot { children }`
/// Returns None if no block is found.
fn try_parse_block(input: &str) -> anyhow::Result<Option<(RuleBody, &str)>> {
    // First, we need to parse a slot
    let (slot, after_slot) = parse_slot(input)?;
    let after_slot = skip_ws_and_comments(after_slot);

    // Check if followed by {
    if !after_slot.starts_with('{') {
        return Ok(None);
    }

    // Parse the block contents
    let (children, rest) = parse_block_contents(&after_slot[1..])?;

    Ok(Some((RuleBody::Block { slot, children }, rest)))
}

/// Parse contents of a { ... } block.
/// Returns list of children and remaining input (after })
fn parse_block_contents(input: &str) -> anyhow::Result<(Vec<RuleBody>, &str)> {
    let mut children = vec![];
    let mut remaining = input;

    loop {
        remaining = skip_ws_and_comments(remaining);

        if remaining.is_empty() {
            anyhow::bail!("unclosed block, expected `}}`");
        }

        // Check for end of block
        if remaining.starts_with('}') {
            let rest = &remaining[1..];
            // Don't consume ; here - let caller handle it
            return Ok((children, rest));
        }

        // Try cross-ref first
        if let Some((cref, rest)) = try_parse_cross_ref(remaining)? {
            children.push(cref);
            remaining = rest;
            continue;
        }

        // Try block
        if let Some((block, rest)) = try_parse_block(remaining)? {
            children.push(block);
            remaining = rest;
            continue;
        }

        // Otherwise parse as a step chain
        let (step, rest) = parse_step_chain(remaining)?;
        children.push(step);
        remaining = rest;
    }
}

/// Parse a step chain: `slot > slot > ... ;`
/// Returns a single RuleBody that may represent multiple steps.
fn parse_step_chain(input: &str) -> anyhow::Result<(RuleBody, &str)> {
    // Parse the first slot
    let (first_slot, mut rest) = parse_slot(input)?;

    rest = skip_ws_and_comments(rest);

    // Check if this is a single slot (no chain)
    if !rest.starts_with('>') {
        // Single slot - just a step
        if !rest.starts_with(';') && !rest.starts_with('}') && !rest.is_empty() {
            // Let caller handle terminator
        }
        return Ok((RuleBody::Step(first_slot), rest));
    }

    // Chain: slot1 > slot2 > ...
    // For now, represent as a Block with the first slot as scope and remaining as children
    // This preserves the sequential semantics
    let mut children = vec![];

    while rest.starts_with('>') {
        rest = &rest[1..];
        rest = skip_ws_and_comments(rest);

        // Parse the next slot
        let (slot, after_slot) = parse_slot(rest)?;
        children.push(RuleBody::Step(slot));
        rest = skip_ws_and_comments(after_slot);

        // Stop if we hit terminator
        if !rest.starts_with('>') {
            break;
        }
    }

    // Return as a Block - first slot is the "scope", rest are children
    Ok((
        RuleBody::Block {
            slot: first_slot,
            children,
        },
        rest,
    ))
}

/// Parse an extractor chain (json(...), ast(...), etc.)
fn parse_extractor_chain(input: &str) -> anyhow::Result<(Slot, &str)> {
    // Parse the extractor slot (json, ast, re, etc.)
    let (slot, rest) = parse_slot(input)?;
    Ok((slot, rest))
}

/// Parse one slot: `tag[arg](body)`, `tag(body)`, or bare glob.
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
        // Bare glob: everything up to unbalanced `>`, `;`, `{`, or end
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

/// Parse a bare glob: content up to the next unquoted `>`, `;`, `{`, `}` at the top
/// level (not inside parens/brackets/quotes).
fn parse_bare_glob(input: &str) -> anyhow::Result<(String, &str)> {
    let mut pos = 0;
    let bytes = input.as_bytes();

    while pos < bytes.len() {
        match bytes[pos] {
            b'>' | b';' | b'{' | b'}' => {
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
            b'(' => {
                // Skip paren-enclosed content
                pos += 1;
                let mut paren_depth = 1u32;
                while pos < bytes.len() && paren_depth > 0 {
                    match bytes[pos] {
                        b'(' => paren_depth += 1,
                        b')' => paren_depth -= 1,
                        b'\\' if paren_depth > 0 => pos += 1,
                        _ => {}
                    }
                    pos += 1;
                }
                continue;
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
    fn parse_simple_rule() {
        let input = "rule(pkg) { fs(**/Cargo.toml) };";
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Rule(decl) = &program[0] else {
            panic!("expected Rule")
        };
        assert_eq!(decl.name, "pkg");
    }

    #[test]
    fn parse_rule_with_scoped_block() {
        let input = r#"rule(deploy) {
            repo($REPO) {
                fs(**/values.yaml) > json({ svc: $SVC })
            }
        };"#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Rule(decl) = &program[0] else {
            panic!("expected Rule")
        };
        assert_eq!(decl.name, "deploy");

        // Check that body[0] is a block
        assert_eq!(decl.body.len(), 1);
        let RuleBody::Block { slot, children } = &decl.body[0] else {
            panic!("expected Block body")
        };
        assert!(matches!(slot.tag(), Some(Tag::Repo)));
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn parse_nested_blocks() {
        let input = r#"rule(img) {
            repo($REPO) {
                rev(main) {
                    folder(packages/$PKG) {
                        fs(values.yaml) > json({ image: { repo: $REPO, tag: $TAG } })
                    }
                }
            }
        };"#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Rule(decl) = &program[0] else {
            panic!("expected Rule")
        };

        // Outer block is repo
        assert_eq!(decl.body.len(), 1);
        let RuleBody::Block {
            slot: repo_slot,
            children: repo_children,
        } = &decl.body[0]
        else {
            panic!("expected outer repo block")
        };
        assert!(matches!(repo_slot.tag(), Some(Tag::Repo)));
        assert_eq!(repo_children.len(), 1);

        // Middle block is rev
        let RuleBody::Block {
            slot: rev_slot,
            children: rev_children,
        } = &repo_children[0]
        else {
            panic!("expected rev block")
        };
        assert!(matches!(rev_slot.tag(), Some(Tag::Rev)));
        assert_eq!(rev_children.len(), 1);

        // Inner block is folder
        let RuleBody::Block {
            slot: folder_slot, ..
        } = &rev_children[0]
        else {
            panic!("expected folder block")
        };
        assert!(matches!(folder_slot.tag(), Some(Tag::Folder)));
    }

    #[test]
    fn parse_flat_chain() {
        let input = "rule(pkg) { fs(**/Cargo.toml) > json({ package: { name: $NAME } }) };";
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
    }

    #[test]
    fn parse_cross_ref() {
        let input = r#"rule(svc_version) {
            deploy_config(repo: $REPO, pin: $PIN)
            repo($REPO) > rev($PIN) > fs(**/package.json) > json({ version: $VERSION })
        };"#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Rule(decl) = &program[0] else {
            panic!("expected Rule")
        };
        assert_eq!(decl.name, "svc_version");

        // body[0] should be a Ref
        assert!(decl.body.len() >= 2);
        let RuleBody::Ref { cross_ref, .. } = &decl.body[0] else {
            panic!("expected Ref body, got {:?}", decl.body[0])
        };
        assert_eq!(cross_ref.rule_name, "deploy_config");
        assert_eq!(cross_ref.bindings.len(), 2);
        assert_eq!(cross_ref.bindings[0].column, "repo");
        assert_eq!(cross_ref.bindings[0].var, "REPO");
        assert_eq!(cross_ref.bindings[1].column, "pin");
        assert_eq!(cross_ref.bindings[1].var, "PIN");
    }

    #[test]
    fn parse_cross_ref_with_block() {
        let input = r#"rule(internal_dep) {
            helm_charts(repo: $REPO, rev: $REV) {
                fs(**/package.json) > json({ dependencies: { $DEP: $SPEC } })
            }
        };"#;
        let program = parse_program(input).unwrap();
        let Statement::Rule(decl) = &program[0] else {
            panic!("expected Rule")
        };

        assert_eq!(decl.body.len(), 1);
        let RuleBody::Ref { cross_ref, children } = &decl.body[0] else {
            panic!("expected Ref body")
        };
        assert_eq!(cross_ref.rule_name, "helm_charts");
        assert_eq!(children.len(), 1);
    }
}
