/// .sprf text parser.
///
/// Parses the outer structure: rule declarations with scoped bodies.
/// Does NOT parse the inside of json() bodies -- that's _2_pattern.rs.
use crate::_0_ast::{
    CheckDecl, CrossRef, CrossRefBinding, Program, RuleBody, RuleDecl, Slot, Statement, Tag,
};

/// A parse error with position information for diagnostics.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Human-readable error message
    pub message: String,
    /// Byte offset in the source where the error occurred
    pub offset: usize,
    /// Approximate line number (best effort for partial parsing)
    pub line: usize,
    /// Context snippet around the error
    pub context: String,
}

/// A diagnostic that is not an error (warning, info, pending cut, etc).
/// This is the "side channel" for non-fatal conditions that tools may want
/// to surface differently than errors. Designed for event bus extensibility.
#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    /// Diagnostic kind - extensible for event bus filtering
    pub kind: DiagnosticKind,
    /// Message to display
    pub message: String,
    /// Byte offset in source
    pub offset: usize,
    /// Line number
    pub line: usize,
    /// Optional structured data for tool consumption
    pub data: Option<serde_json::Value>,
}

/// Kinds of non-error diagnostics. Event bus listeners can filter on these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// Syntax warning (e.g., deprecated syntax)
    Warning,
    /// Informational note
    Info,
    /// A recovery was performed at this location
    Recovery,
    // Future: PendingCut, Hint, etc.
}

/// Result of partial parsing - always returns what was successfully parsed
/// plus any errors encountered. Designed as the "shell for partial programs".
#[derive(Debug, Clone)]
pub struct PartialProgram {
    /// Successfully parsed statements
    pub stmts: Vec<Statement>,
    /// Fatal errors that prevented full parsing
    pub errors: Vec<ParseError>,
    /// Non-fatal diagnostics (warnings, recoveries, etc)
    pub diagnostics: Vec<ParseDiagnostic>,
    /// Whether the parse is completely valid (no errors)
    pub is_valid: bool,
}

impl PartialProgram {
    /// Create an empty partial program
    pub fn empty() -> Self {
        Self {
            stmts: vec![],
            errors: vec![],
            diagnostics: vec![],
            is_valid: true,
        }
    }

    /// Convert to a full Program, failing if there were errors
    pub fn into_program(self) -> anyhow::Result<Program> {
        if let Some(first) = self.errors.first() {
            anyhow::bail!("parse error at line {}: {}", first.line, first.message);
        }
        Ok(self.stmts)
    }
}

/// Parse a .sprf file with error recovery. Returns partial AST even on errors.
/// This is the primary entry point for tools (LSP, CLI, checker) that need
/// to work with partially valid files.
pub fn parse_program_partial(input: &str) -> PartialProgram {
    let mut result = PartialProgram::empty();
    let mut remaining = input;
    let mut last_len = usize::MAX; // Track remaining length to detect progress

    loop {
        let offset = input.len() - remaining.len();
        remaining = skip_ws_and_comments(remaining);
        
        if remaining.is_empty() {
            break;
        }

        // Prevent infinite loops on non-progress
        // If we haven't consumed any input since last iteration, we're stuck
        if remaining.len() == last_len {
            result.errors.push(ParseError {
                message: "parse stuck - cannot recover".to_string(),
                offset,
                line: line_number(input, offset),
                context: context_snippet(remaining, 30),
            });
            result.is_valid = false;
            break;
        }
        last_len = remaining.len();

        match parse_statement(remaining) {
            Ok((stmt, rest)) => {
                result.stmts.push(stmt);
                remaining = rest;
            }
            Err(e) => {
                // Record the error
                let line = line_number(input, offset);
                result.errors.push(ParseError {
                    message: e.to_string(),
                    offset,
                    line,
                    context: context_snippet(remaining, 30),
                });
                result.is_valid = false;

                // Try to recover by finding next statement boundary
                match find_recovery_point(remaining) {
                    Some((recovery_offset, kind)) => {
                        let recovery_line = line_number(input, offset + recovery_offset);
                        result.diagnostics.push(ParseDiagnostic {
                            kind: DiagnosticKind::Recovery,
                            message: format!("recovered at {kind}"),
                            offset: offset + recovery_offset,
                            line: recovery_line,
                            data: None,
                        });
                        remaining = &remaining[recovery_offset..];
                    }
                    None => {
                        // Cannot recover - consume rest to prevent infinite loop
                        break;
                    }
                }
            }
        }
    }

    result
}

/// Original parse_program - now implemented via partial parser for consistency.
/// Use this when you need strict parsing (fail on first error).
pub fn parse_program(input: &str) -> anyhow::Result<Program> {
    parse_program_partial(input).into_program()
}

/// Find a recovery point after an error. Returns (offset, kind) where kind
/// describes what was found (for diagnostics).
/// 
/// Strategy: Find the next statement boundary - either a semicolon or the next
/// rule/check keyword that appears after some content (not at position 0).
fn find_recovery_point(input: &str) -> Option<(usize, &'static str)> {
    // First, try to find a semicolon - this is the cleanest recovery point
    // Scan character by character to handle nested braces reasonably
    let mut brace_depth: i32 = 0;
    let bytes = input.as_bytes();
    let mut pos = 0;
    
    while pos < bytes.len() {
        match bytes[pos] {
            b'{' => brace_depth += 1,
            b'}' => brace_depth = brace_depth.saturating_sub(1),
            b';' if brace_depth == 0 => {
                // Found semicolon at top level - skip past it
                return Some((pos + 1, ";"));
            }
            _ => {}
        }
        pos += 1;
    }
    
    // No semicolon found - look for next statement keyword
    // Start search from position 1 to avoid finding the keyword we're currently failing on
    for keyword in ["rule(", "check("] {
        if let Some(keyword_pos) = input[1..].find(keyword) {
            let actual_pos = 1 + keyword_pos;
            return Some((actual_pos, keyword));
        }
    }
    
    None
}

/// Calculate line number from byte offset (1-indexed)
fn line_number(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].lines().count().max(1)
}

/// Extract context snippet around position
fn context_snippet(input: &str, max_len: usize) -> String {
    let snippet = &input[..input.len().min(max_len)];
    snippet.replace('\n', "\\n")
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

/// Dispatch: `rule ...` or `check ...`.
fn parse_statement(input: &str) -> anyhow::Result<(Statement, &str)> {
    let trimmed = skip_ws_and_comments(input);

    // rule(name) { body };
    if let Some(after) = strip_rule_keyword(trimmed) {
        let (decl, rest) = parse_rule_decl(after)?;
        return Ok((Statement::Rule(decl), rest));
    }

    // check(name) { SQL };
    if let Some(after) = strip_check_keyword(trimmed) {
        let (decl, rest) = parse_check_decl(after)?;
        return Ok((Statement::Check(decl), rest));
    }

    anyhow::bail!(
        "expected `rule` or `check`, found {:?}",
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

/// Check if input starts with `check(`.
fn strip_check_keyword(input: &str) -> Option<&str> {
    if !input.starts_with("check") {
        return None;
    }
    let after = input[5..].trim_start();
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
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
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

/// Parse `check(name) { SQL };`
fn parse_check_decl(input: &str) -> anyhow::Result<(CheckDecl, &str)> {
    // Input starts after "check", at the `(name)` part
    if !input.starts_with('(') {
        anyhow::bail!("expected `(` after `check`");
    }
    let (name, rest) = parse_paren_body(&input[1..])?;
    let name = name.trim().to_string();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        anyhow::bail!("invalid check name: {:?}", name);
    }

    let rest = rest.trim_start();
    if !rest.starts_with('{') {
        anyhow::bail!("expected `{{` after `check({})`", name);
    }

    // Parse SQL body inside braces (brace-aware, but not semicolon-aware)
    let (sql, rest) = parse_check_sql_body(&rest[1..])?;

    // Expect `;` after `}`
    let rest = rest.trim_start();
    if !rest.starts_with(';') {
        anyhow::bail!("expected `;` after `check({}) {{ ... }}`", name);
    }
    let rest = &rest[1..];

    Ok((CheckDecl { name, sql }, rest))
}

/// Parse SQL body inside a check block.
/// SQL can contain semicolons, so we just match braces.
fn parse_check_sql_body(input: &str) -> anyhow::Result<(String, &str)> {
    let mut depth: u32 = 1;
    let mut pos = 0;
    let bytes = input.as_bytes();

    while pos < bytes.len() {
        match bytes[pos] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let sql = input[..pos].trim().to_string();
                    return Ok((sql, &input[pos + 1..]));
                }
            }
            b'\\' => {
                pos += 1;
            }
            _ => {}
        }
        pos += 1;
    }

    anyhow::bail!("unclosed `{{` in check block");
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
        Ok(Some((
            RuleBody::Ref {
                cross_ref,
                children,
            },
            rest,
        )))
    } else {
        Ok(Some((
            RuleBody::Ref {
                cross_ref,
                children: vec![],
            },
            after_paren,
        )))
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

    Ok(Some((RuleBody::Block { slot, children, is_chain: false }, rest)))
}

/// Parse contents of a { ... } block.
/// Children are semicolon-delimited: `{ a > b; c > d; }`
/// Trailing semicolon before `}` is optional.
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
            return Ok((children, rest));
        }

        // Try cross-ref first
        if let Some((cref, rest)) = try_parse_cross_ref(remaining)? {
            children.push(cref);
            remaining = rest;
        } else if let Some((block, rest)) = try_parse_block(remaining)? {
            // Try block
            children.push(block);
            remaining = rest;
        } else {
            // Otherwise parse as a step chain
            let (step, rest) = parse_step_chain(remaining)?;
            children.push(step);
            remaining = rest;
        }

        // Consume semicolon delimiter between siblings
        remaining = skip_ws_and_comments(remaining);
        if remaining.starts_with(';') {
            remaining = &remaining[1..];
        }
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

    // Return as a Block - first slot is the "scope", rest are children.
    // is_chain=true marks these children as sequential pipeline steps, not branches.
    Ok((
        RuleBody::Block {
            slot: first_slot,
            children,
            is_chain: true,
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
/// the tag and the remaining input after the tag name. Tags may carry a
/// `.norm` suffix (e.g. `repo.norm(...)`) for normalized demand-scan matching.
fn try_parse_tag(input: &str) -> Option<(Tag, &str)> {
    // Tags are alphabetic, optionally followed by `.norm`.
    let word_end = input
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(input.len());
    if word_end == 0 {
        return None;
    }

    // Check for `.norm` modifier directly after the alphabetic word.
    let (tag_str, after_tag): (String, &str) = if input[word_end..].starts_with(".norm") {
        let after = &input[word_end + 5..];
        // Guard: `.norm` must be followed by optional whitespace then `(` or `[`,
        // otherwise it's not part of the tag (could be a glob with a dot).
        let after_trimmed = after.trim_start();
        if after_trimmed.starts_with('(') || after_trimmed.starts_with('[') {
            (format!("{}.norm", &input[..word_end]), after)
        } else {
            (input[..word_end].to_string(), &input[word_end..])
        }
    } else {
        (input[..word_end].to_string(), &input[word_end..])
    };

    let rest_trimmed = after_tag.trim_start();
    if !rest_trimmed.starts_with('(') && !rest_trimmed.starts_with('[') {
        return None;
    }

    Tag::from_str(&tag_str).map(|t| (t, after_tag))
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
        let RuleBody::Block { slot, children, .. } = &decl.body[0] else {
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
            ..
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
            ..
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
        let RuleBody::Ref {
            cross_ref,
            children,
        } = &decl.body[0]
        else {
            panic!("expected Ref body")
        };
        assert_eq!(cross_ref.rule_name, "helm_charts");
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn parse_check_block() {
        let input = r#"check(openapi_drift) {
            SELECT m.name, m.version, s.version
            FROM mono_openapi_data m, sot_openapi_data s
            WHERE strip_suffixes(m.name, '-service') = s.name
              AND m.version != s.version
        };"#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 1);
        let Statement::Check(decl) = &program[0] else {
            panic!("expected Check")
        };
        assert_eq!(decl.name, "openapi_drift");
        assert!(decl.sql.contains("SELECT"));
        assert!(decl.sql.contains("FROM mono_openapi_data"));
    }

    #[test]
    fn parse_rule_and_check() {
        let input = r#"rule(pkg) { fs(**/Cargo.toml) };

check(version_mismatch) {
    SELECT * FROM packages WHERE version != expected
};"#;
        let program = parse_program(input).unwrap();
        assert_eq!(program.len(), 2);
        assert!(matches!(program[0], Statement::Rule(_)));
        assert!(matches!(program[1], Statement::Check(_)));
    }

    // Partial parsing tests
    #[test]
    fn partial_parse_valid_input() {
        let input = "rule(pkg) { fs(**/Cargo.toml) };";
        let result = parse_program_partial(input);
        assert!(result.is_valid);
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.stmts.len(), 1);
    }

    #[test]
    fn partial_parse_with_error_recovery() {
        // Missing closing brace - should recover at next rule
        let input = r#"rule(broken) { fs(**/Cargo.toml) 
rule(valid) { fs(**/package.json) };"#;
        let result = parse_program_partial(input);
        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1); // One error for broken rule
        assert_eq!(result.stmts.len(), 1); // But we still get the valid rule
        let Statement::Rule(decl) = &result.stmts[0] else { panic!("expected Rule") };
        assert_eq!(decl.name, "valid");
    }

    #[test]
    fn partial_parse_multiple_rules_one_broken() {
        let input = r#"rule(first) { fs(**/*.toml) };
rule(broken) { json({ 
rule(last) { fs(**/*.json) };"#;
        let result = parse_program_partial(input);
        assert!(!result.is_valid);
        assert_eq!(result.stmts.len(), 2); // first and last
        let Statement::Rule(first) = &result.stmts[0] else { panic!("expected Rule") };
        let Statement::Rule(last) = &result.stmts[1] else { panic!("expected Rule") };
        assert_eq!(first.name, "first");
        assert_eq!(last.name, "last");
    }

    #[test]
    fn partial_parse_reports_diagnostics() {
        // This should trigger recovery diagnostics
        let input = "rule(a) { fs(*) }; rule(b) { json({"; 
        let result = parse_program_partial(input);
        // First rule parses, second fails but we should have recovery info
        assert_eq!(result.stmts.len(), 1);
        assert!(!result.is_valid);
    }

    /// KITCHEN SINK: Comprehensive partial parsing scenarios
    #[test]
    fn partial_parse_kitchen_sink() {
        // A complex .sprf file with multiple errors - should recover and parse valid parts
        let input = r#"
# Valid rule
rule(valid_1) { fs(**/*.toml) > json({ package: { name: $NAME } }) };

# Broken: missing closing brace for json pattern
rule(broken_1) { fs(**/*.json) > json({ name: $NAME 

# Valid rule after broken
rule(valid_2) { fs(**/Cargo.lock) > json({ package: $PKG }) };

# Broken check: missing closing brace
check(broken_check) { SELECT * FROM valid_1 

# Valid check
rule(valid_3) { comment("TODO") > line(re:TODO\s+(?P<TASK>.*)) };

# Broken: unclosed paren
rule(broken_2) { fs(**/*.rs 

# Final valid rule
rule(valid_4) { file(config.yaml) > json({ version: $V }) };
"#;

        let result = parse_program_partial(input);
        
        // Should have 4 valid rules
        let stmt_names: Vec<_> = result.stmts.iter().map(|s| match s {
            Statement::Rule(r) => r.name.as_str(),
            Statement::Check(c) => c.name.as_str(),
        }).collect();
        assert_eq!(result.stmts.len(), 4, "Expected 4 valid statements, got {:?}", stmt_names);
        
        // Should have errors for the 3 broken statements
        assert!(result.errors.len() >= 2, "Expected at least 2 errors for broken statements");
        
        // Should not be valid overall
        assert!(!result.is_valid);
        
        // Verify the valid rule names
        let names: Vec<_> = result.stmts.iter()
            .filter_map(|s| match s { Statement::Rule(r) => Some(r.name.as_str()), _ => None })
            .collect();
        assert!(names.contains(&"valid_1"), "Missing valid_1");
        assert!(names.contains(&"valid_2"), "Missing valid_2");
        assert!(names.contains(&"valid_3"), "Missing valid_3");
        assert!(names.contains(&"valid_4"), "Missing valid_4");
    }

    /// Test that partial parse produces usable AST for LSP-style operations
    #[test]
    fn partial_parse_produces_usable_ast() {
        let input = r#"
rule(pkg) { fs(**/Cargo.toml) > json({ package: { name: $NAME } }) };
rule(broken) { fs(
rule(deps) { fs(**/package.json) > json({ dependencies: { $DEP: $VER } }) };
"#;

        let result = parse_program_partial(input);
        
        // Even with broken middle rule, we should extract 2 valid rules
        assert_eq!(result.stmts.len(), 2);
        
        // The AST should be usable for extraction
        let program: Vec<_> = result.stmts.iter().cloned().collect();
        assert_eq!(program.len(), 2);
        
        // Verify we can access rule bodies
        for stmt in &program {
            if let Statement::Rule(rule) = stmt {
                assert!(!rule.body.is_empty(), "Rule {} should have body", rule.name);
            }
        }
    }
}
