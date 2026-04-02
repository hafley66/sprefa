/// .sprf text parser.
///
/// Parses the outer structure: statements, selector chains, slots.
/// Does NOT parse the inside of json() bodies -- that's _2_pattern.rs.
use crate::_0_ast::{Atom, Capture, CaptureAnnotation, LinkDecl, Program, QueryDecl, RuleDecl, Statement, SelectorChain, Slot, Tag, Term};

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

/// Dispatch: `rule ...`, `link(...)`, `query ...`, `check ...`.
fn parse_statement(input: &str) -> anyhow::Result<(Statement, &str)> {
    let trimmed = skip_ws_and_comments(input);

    // rule name(captures) > selectors;
    if let Some(after) = strip_keyword(trimmed, "rule") {
        let (decl, rest) = parse_rule_decl(after)?;
        return Ok((Statement::Rule(decl), rest));
    }

    // link(src > tgt, pred) > $kind;
    if trimmed.starts_with("link") {
        let after = &trimmed[4..];
        let after_trimmed = after.trim_start();
        if after_trimmed.starts_with('(') {
            let (decl, rest) = parse_link_decl(after_trimmed)?;
            return Ok((Statement::Link(decl), rest));
        }
    }

    // query/check name(args) > body;
    for (keyword, is_check) in [("check", true), ("query", false)] {
        if let Some(after) = strip_keyword(trimmed, keyword) {
            let (mut decl, rest) = parse_query_decl(after)?;
            decl.is_check = is_check;
            return Ok((Statement::Query(decl), rest));
        }
    }

    anyhow::bail!(
        "expected `rule`, `link`, `query`, or `check`, found {:?}",
        &trimmed[..trimmed.len().min(30)]
    );
}

/// Strip a keyword followed by whitespace and a `name(` pattern.
/// Returns the remaining input starting at the name.
fn strip_keyword<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    if !input.starts_with(keyword) {
        return None;
    }
    let after = &input[keyword.len()..];
    let after_trimmed = after.trim_start();
    // Must be followed by `name(` -- an identifier then `(`
    if let Some(paren_pos) = after_trimmed.find('(') {
        let before_paren = after_trimmed[..paren_pos].trim();
        if !before_paren.is_empty()
            && before_paren.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Some(after_trimmed);
        }
    }
    None
}

/// Parse `rule name($SVC, repo($REPO), rev($TAG)) > selectors;`
fn parse_rule_decl(input: &str) -> anyhow::Result<(RuleDecl, &str)> {
    // Parse name
    let name_end = input.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(input.len());
    if name_end == 0 {
        anyhow::bail!("expected rule name");
    }
    let name = input[..name_end].to_string();
    let rest = input[name_end..].trim_start();

    if !rest.starts_with('(') {
        anyhow::bail!("expected `(` after rule name `{}`", name);
    }
    let (captures_body, rest) = parse_paren_body(&rest[1..])?;
    let captures = parse_captures(&captures_body)?;

    let rest = rest.trim_start();
    if !rest.starts_with('>') {
        anyhow::bail!("expected `>` after rule head `{}(...)`", name);
    }
    let rest = &rest[1..];

    // Parse selector chain as body
    let (chain, rest) = parse_selector_chain(rest)?;

    Ok((RuleDecl { name, captures, chain }, rest))
}

/// Parse comma-separated captures: `$SVC, repo($REPO), rev($TAG)`
fn parse_captures(input: &str) -> anyhow::Result<Vec<Capture>> {
    let mut captures = vec![];
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        captures.push(parse_one_capture(part)?);
    }
    if captures.is_empty() {
        anyhow::bail!("rule head must have at least one capture");
    }
    Ok(captures)
}

/// Parse one capture: `$VAR`, `repo($VAR)`, `rev($VAR)`, `name($VAR)`, `file($VAR)`
fn parse_one_capture(input: &str) -> anyhow::Result<Capture> {
    let input = input.trim();

    // Bare $VAR
    if input.starts_with('$') {
        let var = &input[1..];
        if var.is_empty() || !var.chars().next().unwrap().is_ascii_uppercase() {
            anyhow::bail!("capture variable must be SCREAMING_CASE: {:?}", input);
        }
        return Ok(Capture { var: var.to_string(), annotation: None });
    }

    // Annotated: annotation($VAR)
    let paren = input.find('(')
        .ok_or_else(|| anyhow::anyhow!("expected `$VAR` or `annotation($VAR)`, found {:?}", input))?;
    let ann_name = input[..paren].trim();
    let annotation = match ann_name {
        "repo" => CaptureAnnotation::Repo,
        "rev" => CaptureAnnotation::Rev,
        "name" => CaptureAnnotation::Name,
        "file" => CaptureAnnotation::File,
        _ => anyhow::bail!("unknown capture annotation `{}`, expected repo/rev/name/file", ann_name),
    };

    let inner = &input[paren + 1..];
    let close = inner.find(')')
        .ok_or_else(|| anyhow::anyhow!("unclosed `(` in capture annotation"))?;
    let var_str = inner[..close].trim();

    if !var_str.starts_with('$') {
        anyhow::bail!("capture annotation body must be `$VAR`, found {:?}", var_str);
    }
    let var = &var_str[1..];
    if var.is_empty() || !var.chars().next().unwrap().is_ascii_uppercase() {
        anyhow::bail!("capture variable must be SCREAMING_CASE: {:?}", var_str);
    }

    Ok(Capture { var: var.to_string(), annotation: Some(annotation) })
}

/// Parse `query name($A, $B) > rel($A, $X)  rel2($X, $B);`
///
/// Head and body separated by `>`. Body atoms are whitespace-delimited
/// (newline or space), terminated by `;`.
fn parse_query_decl(input: &str) -> anyhow::Result<(QueryDecl, &str)> {
    // Parse head atom: name($A, $B)
    let (head, rest) = parse_atom(input)?;
    let rest = rest.trim_start();

    // Expect `>`
    if !rest.starts_with('>') {
        anyhow::bail!("expected `>` after query head, found {:?}", &rest[..rest.len().min(20)]);
    }
    let mut rest = skip_ws_and_comments(&rest[1..]);

    // Parse body atoms: whitespace-delimited, terminated by `;`
    let mut body = vec![];
    loop {
        if rest.starts_with(';') {
            rest = &rest[1..];
            break;
        }
        if rest.is_empty() {
            anyhow::bail!("unexpected end of input in query body, expected `;`");
        }
        let (atom, r) = parse_atom(rest)?;
        body.push(atom);
        rest = skip_ws_and_comments(r);
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

/// Parse a selector chain: `slot > slot > ... ;`
/// Returns (SelectorChain, remaining input).
fn parse_selector_chain(input: &str) -> anyhow::Result<(SelectorChain, &str)> {
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
    fn parse_rule_head_and_selectors() {
        let input = "rule pkg($NAME) > fs(**/Cargo.toml) > json({ package: { name: $NAME } });";
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Rule(decl) = &program[0] else { panic!("expected Rule") };
        assert_eq!(decl.name, "pkg");
        assert_eq!(decl.captures.len(), 1);
        assert_eq!(decl.captures[0].var, "NAME");
        assert!(decl.captures[0].annotation.is_none());
        assert_eq!(decl.chain.slots.len(), 2);

        match &decl.chain.slots[0] {
            Slot::Tagged { tag, arg, body } => {
                assert_eq!(*tag, Tag::Fs);
                assert!(arg.is_none());
                assert_eq!(body, "**/Cargo.toml");
            }
            _ => panic!("expected Tagged"),
        }
    }

    #[test]
    fn parse_rule_with_annotations() {
        let input = "rule deploy($SVC, repo($REPO), rev($TAG)) > fs(**/values.yaml) > json({ svc: $SVC, repo: $REPO, tag: $TAG });";
        let program = parse_program(input).unwrap();
        let Statement::Rule(decl) = &program[0] else { panic!("expected Rule") };
        assert_eq!(decl.name, "deploy");
        assert_eq!(decl.captures.len(), 3);
        assert_eq!(decl.captures[0].var, "SVC");
        assert!(decl.captures[0].annotation.is_none());
        assert_eq!(decl.captures[1].var, "REPO");
        assert_eq!(decl.captures[1].annotation, Some(CaptureAnnotation::Repo));
        assert_eq!(decl.captures[2].var, "TAG");
        assert_eq!(decl.captures[2].annotation, Some(CaptureAnnotation::Rev));
    }

    #[test]
    fn parse_rule_bare_three() {
        let input = "rule deps($K) > my-org/* > main|release/* > **/Cargo.toml > json({ deps: { $K: $_ } });";
        let program = parse_program(input).unwrap();
        let Statement::Rule(decl) = &program[0] else { panic!("expected Rule") };
        assert_eq!(decl.chain.slots.len(), 4);

        match &decl.chain.slots[0] {
            Slot::Bare(g) => assert_eq!(g, "my-org/*"),
            _ => panic!("expected Bare"),
        }
        match &decl.chain.slots[1] {
            Slot::Bare(g) => assert_eq!(g, "main|release/*"),
            _ => panic!("expected Bare"),
        }
    }

    #[test]
    fn parse_rule_ast_with_lang() {
        let input = "rule imports($NAME, $PATH) > fs(**/*.config) > ast[typescript](import $NAME from '$PATH');";
        let program = parse_program(input).unwrap();
        let Statement::Rule(decl) = &program[0] else { panic!("expected Rule") };
        match &decl.chain.slots[1] {
            Slot::Tagged { tag, arg, body } => {
                assert_eq!(*tag, Tag::Ast);
                assert_eq!(arg.as_deref(), Some("typescript"));
                assert_eq!(body, "import $NAME from '$PATH'");
            }
            _ => panic!("expected Tagged"),
        }
    }

    #[test]
    fn parse_rule_nested_parens_in_re() {
        let input = r"rule img($REPO, $TAG) > fs(helm/**/*.yaml) > re(image:\s+(?P<REPO>[^:]+):(?P<TAG>.+));";
        let program = parse_program(input).unwrap();
        let Statement::Rule(decl) = &program[0] else { panic!("expected Rule") };
        match &decl.chain.slots[1] {
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
rule a($NAME) > fs(**/package.json) > json({ name: $NAME });

# second rule
rule b($N) > fs(**/Cargo.toml) > json({ package: { name: $N } });
"#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 2);
    }

    #[test]
    fn unclosed_paren_errors() {
        let input = "rule x($Y) > fs(**/foo > json({ x: $Y });";
        let err = parse_program(input).unwrap_err();
        assert!(err.to_string().contains("unclosed"), "{}", err);
    }

    #[test]
    fn missing_keyword_errors() {
        let input = "fs(**/foo);";
        let err = parse_program(input).unwrap_err();
        assert!(err.to_string().contains("rule") || err.to_string().contains("expected"), "{}", err);
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
            rule package_name($NAME) > fs(**/Cargo.toml) > json({ package: { name: $NAME } });
            link(NAME > NAME, norm_eq) > $dep_to_package;
        "#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 2);
        assert!(matches!(&program[0], Statement::Rule(_)));
        assert!(matches!(&program[1], Statement::Link(_)));
    }

    #[test]
    fn parse_query_basic() {
        let input = "query all_deps($A, $C) > dep_to_package($A, $B) all_deps($B, $C);";
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
        let input = r#"query who_uses($WHO) > dep_to_package($WHO, "lodash");"#;
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert_eq!(q.head.args.len(), 1);
        assert_eq!(q.body[0].args[1], Term::Lit("lodash".into()));
    }

    #[test]
    fn parse_query_with_wildcard() {
        let input = "query has_dep($A) > dep_to_package($A, $_);";
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert_eq!(q.body[0].args[1], Term::Wild);
    }

    #[test]
    fn parse_query_nonrecursive() {
        let input = "query same_eco($A, $B) > dep_to_package($A, $X) dep_to_package($B, $X);";
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert_eq!(q.body.len(), 2);
        assert_eq!(q.body[0].args[1], Term::Var("X".into()));
        assert_eq!(q.body[1].args[1], Term::Var("X".into()));
    }

    #[test]
    fn parse_query_mixed_with_rules_and_links() {
        let input = r#"
            rule package_name($NAME) > fs(**/Cargo.toml) > json({ package: { name: $NAME } });
            link(NAME > NAME, norm_eq) > $dep_to_package;
            query all_deps($A, $C) > dep_to_package($A, $B) all_deps($B, $C);
        "#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 3);
        assert!(matches!(&program[0], Statement::Rule(_)));
        assert!(matches!(&program[1], Statement::Link(_)));
        assert!(matches!(&program[2], Statement::Query(_)));
    }

    #[test]
    fn parse_query_missing_arrow_errors() {
        let err = parse_program("query foo($A) dep($A);").unwrap_err();
        assert!(err.to_string().contains(">"), "{}", err);
    }

    #[test]
    fn parse_check_basic() {
        let input = r#"check orphan_dep($X) > has_kind($X, "dep") not dep_link($X, $_);"#;
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
        let input = "query foo($A) > bar($A, $_);";
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert!(!q.is_check);
    }

    #[test]
    fn parse_negated_atom_in_query() {
        let input = "query no_link($A) > has_kind($A, \"dep\") not linked($A, $_);";
        let program = parse_program(input).unwrap();
        let Statement::Query(q) = &program[0] else { panic!("expected Query") };
        assert!(!q.body[0].negated);
        assert!(q.body[1].negated);
        assert_eq!(q.body[1].relation, "linked");
    }

    #[test]
    fn parse_check_mixed() {
        let input = r#"
            query all_deps($A, $C) > dep_to_package($A, $B) all_deps($B, $C);
            check orphan($X) > has_kind($X, "dep") not dep_link($X, $_);
        "#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 2);
        let Statement::Query(q1) = &program[0] else { panic!("expected Query") };
        let Statement::Query(q2) = &program[1] else { panic!("expected Query") };
        assert!(!q1.is_check);
        assert!(q2.is_check);
    }
}
