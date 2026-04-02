/// .sprf text parser.
///
/// Parses the outer structure: statements, selector chains, slots.
/// Does NOT parse the inside of json() bodies -- that's _2_pattern.rs.
use crate::_0_ast::{Atom, LinkDecl, Program, QueryDecl, ScanRole, Statement, SelectorChain, Slot, Tag, Term};

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

/// Dispatch: `link(...)`, `query ...`, or a normal rule.
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
    for (keyword, is_check) in [("check", true), ("query", false)] {
        if trimmed.starts_with(keyword) {
            let after = &trimmed[keyword.len()..];
            let after_trimmed = after.trim_start();
            // Disambiguate: keyword requires `name(` next, not a tag like `query(...)`
            // which would be a rule slot. Check for `word(` pattern without being a known tag.
            if let Some(paren_pos) = after_trimmed.find('(') {
                let before_paren = after_trimmed[..paren_pos].trim();
                if !before_paren.is_empty()
                    && before_paren.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    let (mut decl, rest) = parse_query_decl(after_trimmed)?;
                    decl.is_check = is_check;
                    return Ok((Statement::Query(decl), rest));
                }
            }
        }
    }
    let (chain, rest) = parse_rule(trimmed)?;
    Ok((Statement::Rule(chain), rest))
}

/// Parse `query name($A, $B) :- rel($A, $X), rel2($X, $B);`
fn parse_query_decl(input: &str) -> anyhow::Result<(QueryDecl, &str)> {
    // Parse head atom: name($A, $B)
    let (head, rest) = parse_atom(input)?;
    let rest = rest.trim_start();

    // Expect `:-`
    if !rest.starts_with(":-") {
        anyhow::bail!("expected `:-` after query head, found {:?}", &rest[..rest.len().min(20)]);
    }
    let mut rest = rest[2..].trim_start();

    // Parse body atoms separated by `,`
    let mut body = vec![];
    loop {
        let (atom, r) = parse_atom(rest)?;
        body.push(atom);
        let r = r.trim_start();
        if r.starts_with(',') {
            rest = r[1..].trim_start();
        } else if r.starts_with(';') {
            rest = &r[1..];
            break;
        } else {
            anyhow::bail!("expected `,` or `;` in query body, found {:?}", &r[..r.len().min(20)]);
        }
    }

    if body.is_empty() {
        anyhow::bail!("query body cannot be empty");
    }

    Ok((QueryDecl { head, body, is_check: false }, rest))
}

/// Parse one atom: `name($ARG1, $ARG2)` or `not name($ARG1, "literal")`.
fn parse_atom(input: &str) -> anyhow::Result<(Atom, &str)> {
    let input = input.trim_start();

    // Consume optional `not` prefix
    let (negated, input) = if input.starts_with("not ")
        || input.starts_with("not\t")
        || input.starts_with("not\n")
    {
        (true, input[3..].trim_start())
    } else {
        (false, input)
    };

    let name_end = input.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(input.len());
    if name_end == 0 {
        anyhow::bail!("expected relation name, found {:?}", &input[..input.len().min(20)]);
    }
    let relation = input[..name_end].to_string();
    let rest = input[name_end..].trim_start();

    if !rest.starts_with('(') {
        anyhow::bail!("expected `(` after relation name `{}`", relation);
    }
    let (body, rest) = parse_paren_body(&rest[1..])?;

    let mut args = vec![];
    for part in body.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        args.push(parse_term(part)?);
    }

    Ok((Atom { relation, args, negated }, rest))
}

/// Parse a single term: `$VAR`, `$_`, `"literal"`, or bare identifier.
fn parse_term(input: &str) -> anyhow::Result<Term> {
    let input = input.trim();
    if input == "$_" {
        return Ok(Term::Wild);
    }
    if input.starts_with('$') {
        let name = &input[1..];
        if name.is_empty() || !name.chars().next().unwrap().is_ascii_uppercase() {
            anyhow::bail!("query variable must be SCREAMING_CASE: {:?}", input);
        }
        return Ok(Term::Var(name.to_string()));
    }
    if input.starts_with('"') && input.ends_with('"') && input.len() >= 2 {
        return Ok(Term::Lit(input[1..input.len() - 1].to_string()));
    }
    // Bare identifier as literal
    Ok(Term::Lit(input.to_string()))
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
            let (capture, kind, scan) = parse_match_body(&body)?;
            return Ok((Slot::Match { capture, kind, scan }, rest));
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

/// Parse the body of `match($CAPTURE, kind)` or `match($CAPTURE, kind, IS_REPO)`.
fn parse_match_body(body: &str) -> anyhow::Result<(String, String, Option<ScanRole>)> {
    let body = body.trim();
    let comma = body.find(',')
        .ok_or_else(|| anyhow::anyhow!("match() requires `$CAPTURE, kind`"))?;
    let cap_part = body[..comma].trim();
    let rest = body[comma + 1..].trim();

    if !cap_part.starts_with('$') {
        anyhow::bail!("match() capture must start with `$`, found {:?}", cap_part);
    }
    let capture = cap_part[1..].to_string();
    if capture.is_empty() {
        anyhow::bail!("empty capture name in match()");
    }

    // Split remaining into kind and optional scan role
    let (kind_part, scan) = if let Some(comma2) = rest.find(',') {
        let kind = rest[..comma2].trim();
        let scan_str = rest[comma2 + 1..].trim();
        let role = match scan_str {
            "IS_REPO" => ScanRole::Repo,
            "IS_REV" => ScanRole::Rev,
            other => anyhow::bail!(
                "match() 3rd argument must be IS_REPO or IS_REV, found {:?}", other
            ),
        };
        (kind, Some(role))
    } else {
        (rest, None)
    };

    if kind_part.is_empty() {
        anyhow::bail!("empty kind in match()");
    }

    Ok((capture, kind_part.to_string(), scan))
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
            Slot::Match { capture, kind, .. } => {
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
        assert!(matches!(&chain.slots[2], Slot::Match { capture, kind, .. } if capture == "NAME" && kind == "dep_name"));
        assert!(matches!(&chain.slots[3], Slot::Match { capture, kind, .. } if capture == "VER" && kind == "dep_version"));
    }

    #[test]
    fn parse_match_with_scan_role() {
        let input = "fs(**/values.yaml) > json({ image: { repository: $REPO, tag: $TAG } }) > match($REPO, image_repo, IS_REPO) > match($TAG, image_tag, IS_REV);";
        let program = parse_program(input).unwrap();
        let Statement::Rule(chain) = &program[0] else { panic!("expected Rule") };
        assert_eq!(chain.slots.len(), 4);

        match &chain.slots[2] {
            Slot::Match { capture, kind, scan } => {
                assert_eq!(capture, "REPO");
                assert_eq!(kind, "image_repo");
                assert_eq!(*scan, Some(ScanRole::Repo));
            }
            _ => panic!("expected Match"),
        }
        match &chain.slots[3] {
            Slot::Match { capture, kind, scan } => {
                assert_eq!(capture, "TAG");
                assert_eq!(kind, "image_tag");
                assert_eq!(*scan, Some(ScanRole::Rev));
            }
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn parse_match_without_scan_role() {
        let input = "fs(**/Cargo.toml) > json({ package: { name: $N } }) > match($N, pkg);";
        let program = parse_program(input).unwrap();
        let Statement::Rule(chain) = &program[0] else { panic!("expected Rule") };
        match &chain.slots[2] {
            Slot::Match { scan, .. } => assert_eq!(*scan, None),
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn parse_match_invalid_scan_role() {
        let input = "fs(**/x.yaml) > json({ x: $X }) > match($X, kind, IS_BANANA);";
        let result = parse_program(input);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("IS_REPO or IS_REV"), "error: {}", err);
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

    #[test]
    fn parse_query_basic() {
        let input = "query all_deps($A, $C) :- dep_to_package($A, $B), all_deps($B, $C);";
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert_eq!(q.head.relation, "all_deps");
        assert_eq!(q.head.args.len(), 2);
        assert_eq!(q.head.args[0], Term::Var("A".into()));
        assert_eq!(q.head.args[1], Term::Var("C".into()));
        assert_eq!(q.body.len(), 2);
        assert_eq!(q.body[0].relation, "dep_to_package");
        assert_eq!(q.body[1].relation, "all_deps");
    }

    #[test]
    fn parse_query_with_literal() {
        let input = r#"query who_uses($WHO) :- dep_to_package($WHO, "lodash");"#;
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert_eq!(q.head.args.len(), 1);
        assert_eq!(q.body[0].args[1], Term::Lit("lodash".into()));
    }

    #[test]
    fn parse_query_with_wildcard() {
        let input = "query has_dep($A) :- dep_to_package($A, $_);";
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert_eq!(q.body[0].args[1], Term::Wild);
    }

    #[test]
    fn parse_query_nonrecursive() {
        let input = "query same_eco($A, $B) :- dep_to_package($A, $X), dep_to_package($B, $X);";
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert_eq!(q.body.len(), 2);
        assert_eq!(q.body[0].args[1], Term::Var("X".into()));
        assert_eq!(q.body[1].args[1], Term::Var("X".into()));
    }

    #[test]
    fn parse_query_mixed_with_rules_and_links() {
        let input = r#"
            fs(**/Cargo.toml) > json({ package: { name: $NAME } }) > match($NAME, package_name);
            link(dep_name > package_name, norm_eq) > $dep_to_package;
            query all_deps($A, $C) :- dep_to_package($A, $B), all_deps($B, $C);
        "#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 3);
        assert!(matches!(&program[0], Statement::Rule(_)));
        assert!(matches!(&program[1], Statement::Link(_)));
        assert!(matches!(&program[2], Statement::Query(_)));
    }

    #[test]
    fn parse_query_missing_horn_errors() {
        let err = parse_program("query foo($A) dep($A);").unwrap_err();
        assert!(err.to_string().contains(":-"), "{}", err);
    }

    #[test]
    fn parse_check_basic() {
        let input = r#"check orphan_dep($X) :- has_kind($X, "dep"), not dep_link($X, $_);"#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert!(q.is_check);
        assert_eq!(q.head.relation, "orphan_dep");
        assert_eq!(q.body.len(), 2);
        assert!(!q.body[0].negated);
        assert!(q.body[1].negated);
        assert_eq!(q.body[1].relation, "dep_link");
    }

    #[test]
    fn parse_query_is_not_check() {
        let input = "query foo($A) :- bar($A, $_);";
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert!(!q.is_check);
    }

    #[test]
    fn parse_negated_atom_in_query() {
        let input = "query no_link($A) :- has_kind($A, \"dep\"), not linked($A, $_);";
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert!(!q.body[0].negated);
        assert!(q.body[1].negated);
        assert_eq!(q.body[1].relation, "linked");
    }

    #[test]
    fn parse_check_mixed() {
        let input = r#"
            query all_deps($A, $C) :- dep_to_package($A, $B), all_deps($B, $C);
            check orphan($X) :- has_kind($X, "dep"), not dep_link($X, $_);
        "#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 2);
        let Statement::Query(q1) = &program[0] else { panic!("expected Query") };
        let Statement::Query(q2) = &program[1] else { panic!("expected Query") };
        assert!(!q1.is_check);
        assert!(q2.is_check);
    }
}
