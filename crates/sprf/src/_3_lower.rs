/// Lower .sprf parse tree to Rule types.
///
/// Scoped RuleBody -> flattened Rule with dependency-based ordering
///
/// The lowering process:
/// 1. Flatten nested blocks into linear sequence with scope depths
/// 2. Sort steps based on variable dependencies (if B references var from A, A comes first)
/// 3. Convert slots to SelectStep
///
/// Example:
/// ```sprf
/// rule(deploy) {
///   repo($REPO) {
///     rev(main) {
///       fs(**/values.yaml) > json({ svc: $SVC })
///     }
///   }
/// };
/// ```
/// Becomes steps: [repo@depth0, rev@depth1, fs@depth2, json@depth2]
/// Execution order respects scope: repo runs first, then rev, then fs/json
use anyhow::{bail, Result};
use sprefa_rules::graph::DepEdge;
use sprefa_rules::pattern::{parse_segment_pattern, Segment};
use sprefa_rules::types::{AstSelector, LineMatcher, MarkerScope, MdPattern, MatchDef, Rule, RuleSet, SelectStep};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::_0_ast::{Program, RuleBody, RuleDecl, Slot, Statement, Tag};
use crate::_2_pattern::parse_json_body;

/// Lower a parsed program into a RuleSet and dependency edges.
pub fn lower_program(program: &Program) -> Result<(RuleSet, Vec<DepEdge>)> {
    let mut rules = vec![];
    let mut edges = vec![];

    for stmt in program {
        match stmt {
            Statement::Rule(decl) => {
                let decl_rules = lower_rule_decl(decl)?;
                collect_dep_edges(&decl.body, &decl.name, &mut edges);
                rules.extend(decl_rules);
            }
            Statement::Check(_) => {
                // Check blocks are handled separately by the invariant checker
            }
        }
    }

    Ok((
        RuleSet {
            schema: None,
            rules,
        },
        edges,
    ))
}

/// Extract check declarations from a parsed program.
pub fn extract_checks(program: &Program) -> Vec<crate::_0_ast::CheckDecl> {
    program
        .iter()
        .filter_map(|stmt| match stmt {
            Statement::Check(decl) => Some(decl.clone()),
            _ => None,
        })
        .collect()
}

/// Walk rule bodies to find cross-rule references and emit dependency edges.
fn collect_dep_edges(bodies: &[RuleBody], consumer: &str, edges: &mut Vec<DepEdge>) {
    for body in bodies {
        match body {
            RuleBody::Step(_) => {}
            RuleBody::Block { children, .. } => {
                collect_dep_edges(children, consumer, edges);
            }
            RuleBody::Ref {
                cross_ref,
                children,
            } => {
                edges.push(DepEdge {
                    producer: cross_ref.rule_name.clone(),
                    consumer: consumer.to_string(),
                    bindings: cross_ref
                        .bindings
                        .iter()
                        .map(|b| (b.column.clone(), b.var.clone()))
                        .collect(),
                });
                collect_dep_edges(children, consumer, edges);
            }
        }
    }
}

/// One step in the flattened rule with scope information.
#[derive(Debug, Clone)]
struct ScopedStep {
    depth: usize,
    scope_vars: HashSet<String>, // Variables captured at this scope level
    select_steps: Vec<SelectStep>,
    ast_selector: Option<AstSelector>,
    line_matcher: Option<LineMatcher>,
    marker_scope: Option<MarkerScope>,
    md_scope: Option<MdPattern>,
    md_matcher: Option<MdPattern>,
}

/// Expand one RuleBody into all its monomorphized variants.
///
/// Forking only happens at brace-block levels (`is_chain: false`). Chain-blocks
/// (`is_chain: true`) have sequential pipeline children that must stay together.
///
/// A Block (or Ref) with N brace children becomes N variants, each with one child path.
/// A leaf (Step, chain-block, or empty Block/Ref) produces one variant unchanged.
fn monomorphize_one(body: &RuleBody) -> Vec<RuleBody> {
    match body {
        RuleBody::Step(_) => vec![body.clone()],
        RuleBody::Block { is_chain: true, .. } => vec![body.clone()],
        RuleBody::Block { slot, children, is_chain: false } if children.is_empty() => {
            vec![body.clone()]
        }
        RuleBody::Block { slot, children, is_chain: false } => {
            let mut result = vec![];
            for child in children {
                for variant in monomorphize_one(child) {
                    result.push(RuleBody::Block {
                        slot: slot.clone(),
                        children: vec![variant],
                        is_chain: false,
                    });
                }
            }
            result
        }
        RuleBody::Ref { cross_ref, children } if children.is_empty() => vec![body.clone()],
        RuleBody::Ref { cross_ref, children } => {
            let mut result = vec![];
            for child in children {
                for variant in monomorphize_one(child) {
                    result.push(RuleBody::Ref {
                        cross_ref: cross_ref.clone(),
                        children: vec![variant],
                    });
                }
            }
            result
        }
    }
}

/// Expand a rule body list into all monomorphized paths.
///
/// Top-level items in `bodies` are sequential (AND conditions); they are NOT branches.
/// Branching happens only inside brace-block children. For each top-level item that
/// expands to N variants, the Cartesian product with other top-level variants is taken.
///
/// In practice, branching only occurs at one brace level, so a rule with N branches
/// in one block produces exactly N paths. A rule with no branches produces 1 path.
fn monomorphize_bodies(bodies: &[RuleBody]) -> Vec<Vec<RuleBody>> {
    if bodies.is_empty() {
        return vec![vec![]];
    }
    // Start with one empty path, then extend by each body item's variants.
    let mut all_paths: Vec<Vec<RuleBody>> = vec![vec![]];
    for body in bodies {
        let variants = monomorphize_one(body);
        if variants.len() == 1 {
            // Common case: no branching -- append to all existing paths.
            for path in &mut all_paths {
                path.push(variants[0].clone());
            }
        } else {
            // Branching: each existing path multiplies by N variants.
            let prev_paths = std::mem::take(&mut all_paths);
            for path in prev_paths {
                for variant in &variants {
                    let mut new_path = path.clone();
                    new_path.push(variant.clone());
                    all_paths.push(new_path);
                }
            }
        }
    }
    all_paths
}

fn lower_rule_decl(decl: &RuleDecl) -> Result<Vec<Rule>> {
    // Compute create_matches from ALL captures across ALL branches so that every
    // monomorphized Rule shares the same table schema.
    let mut scan_vars_all: HashMap<String, String> = HashMap::new();
    collect_scan_annotations(&decl.body, &mut scan_vars_all);

    // Collect all json annotations from a full flatten of the original body to seed scan_vars.
    let mut probe_annots = vec![];
    for body in &decl.body {
        let _ = flatten_body(body, 0, &HashSet::new(), &mut probe_annots);
    }
    for annot in &probe_annots {
        let kind = if annot.norm {
            format!("{}.norm", annot.kind)
        } else {
            annot.kind.clone()
        };
        scan_vars_all.insert(annot.var.clone(), kind);
    }

    let mut all_vars: Vec<String> = vec![];
    for body in &decl.body {
        for cap in body.all_captures() {
            if !all_vars.contains(&cap) {
                all_vars.push(cap);
            }
        }
    }

    let create_matches: Vec<MatchDef> = all_vars
        .iter()
        .map(|var| MatchDef {
            capture: var.clone(),
            kind: var.clone(),
            parent: None,
            scan: scan_vars_all.get(var).cloned(),
        })
        .collect();

    // Monomorphize: expand body tree into all root-to-leaf paths.
    let paths = monomorphize_bodies(&decl.body);

    let mut rules = vec![];
    for path_bodies in &paths {
        let mut all_scoped = vec![];
        let mut json_annotations = vec![];
        for body in path_bodies {
            let flattened = flatten_body(body, 0, &HashSet::new(), &mut json_annotations)?;
            all_scoped.extend(flattened);
        }

        all_scoped.sort_by_key(|s| s.depth);

        let mut select: Vec<SelectStep> = vec![];
        let mut select_ast: Option<AstSelector> = None;
        let mut line_matcher: Option<LineMatcher> = None;
        let mut marker_scope: Option<MarkerScope> = None;
        let mut md_scope: Option<MdPattern> = None;
        let mut md_matcher: Option<MdPattern> = None;

        for scoped in &all_scoped {
            select.extend(scoped.select_steps.clone());
            if let Some(ast) = scoped.ast_selector.clone() {
                select_ast = Some(ast);
            }
            if let Some(lm) = scoped.line_matcher.clone() {
                line_matcher = Some(lm);
            }
            if let Some(ms) = scoped.marker_scope.clone() {
                marker_scope = Some(ms);
            }
            if let Some(ms) = scoped.md_scope.clone() {
                md_scope = Some(ms);
            }
            if let Some(mm) = scoped.md_matcher.clone() {
                md_matcher = Some(mm);
            }
        }

        rules.push(Rule {
            name: decl.name.clone(),
            description: None,
            select,
            select_ast,
            value: line_matcher,
            marker_scope,
            md_scope,
            md_matcher,
            create_matches: create_matches.clone(),
            confidence: None,
        });
    }

    Ok(rules)
}

/// Walk rule bodies to find repo($VAR)/rev($VAR) tags and record their
/// captured variables as scan-driving columns.
fn collect_scan_annotations(bodies: &[RuleBody], scan_vars: &mut HashMap<String, String>) {
    for body in bodies {
        let (slot, children) = match body {
            RuleBody::Step(slot) => (Some(slot), &[][..]),
            RuleBody::Block { slot, children, .. } => (Some(slot), children.as_slice()),
            RuleBody::Ref { children, .. } => (None, children.as_slice()),
        };
        if let Some(Slot::Tagged { tag, body, .. }) = slot {
            match tag {
                Tag::Repo => {
                    for var in slot.unwrap().captures() {
                        scan_vars.insert(var, "repo".to_string());
                    }
                }
                Tag::Rev => {
                    for var in slot.unwrap().captures() {
                        scan_vars.insert(var, "rev".to_string());
                    }
                }
                Tag::RepoNorm => {
                    for var in slot.unwrap().captures() {
                        scan_vars.insert(var, "repo.norm".to_string());
                    }
                }
                Tag::RevNorm => {
                    for var in slot.unwrap().captures() {
                        scan_vars.insert(var, "rev.norm".to_string());
                    }
                }
                _ => {}
            }
        }
        collect_scan_annotations(children, scan_vars);
    }
}

/// Flatten RuleBody into a list of scoped steps.
///
/// `depth` is the current nesting level (0 = top, 1 = first block, etc.)
/// `parent_vars` are variables captured by outer scopes (available to inner)
fn flatten_body(
    body: &RuleBody,
    depth: usize,
    parent_vars: &HashSet<String>,
    json_annotations: &mut Vec<crate::_2_pattern::ScanAnnotation>,
) -> Result<Vec<ScopedStep>> {
    let mut result = vec![];

    match body {
        RuleBody::Step(slot) => {
            let scoped = slot_to_scoped_step(slot, depth, parent_vars, json_annotations)?;
            result.push(scoped);
        }
        RuleBody::Block { slot, children, .. } => {
            let block_vars = extract_slot_vars(slot);
            let mut available_vars = parent_vars.clone();
            available_vars.extend(block_vars.iter().cloned());

            let block_scoped = slot_to_scoped_step(slot, depth, parent_vars, json_annotations)?;
            result.push(block_scoped);

            for child in children {
                let child_scoped =
                    flatten_body(child, depth + 1, &available_vars, json_annotations)?;
                result.extend(child_scoped);
            }
        }
        RuleBody::Ref {
            cross_ref,
            children,
        } => {
            let mut available_vars = parent_vars.clone();
            for binding in &cross_ref.bindings {
                available_vars.insert(binding.var.clone());
            }
            for child in children {
                let child_scoped =
                    flatten_body(child, depth + 1, &available_vars, json_annotations)?;
                result.extend(child_scoped);
            }
        }
    }

    Ok(result)
}

/// Extract variable names from a slot body.
fn extract_slot_vars(slot: &Slot) -> HashSet<String> {
    let mut vars = HashSet::new();
    let body = match slot {
        Slot::Bare(s) => s.as_str(),
        Slot::Tagged { body, .. } => body.as_str(),
    };

    // Find $SCREAMING variables
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'_' {
                i += 1;
                continue; // $_ is wildcard, not a capture
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > start {
                let var = &body[start..i];
                if var
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
                {
                    vars.insert(var.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    vars
}

/// Convert a slot to a scoped step.
fn slot_to_scoped_step(
    slot: &Slot,
    depth: usize,
    available_vars: &HashSet<String>,
    json_annotations: &mut Vec<crate::_2_pattern::ScanAnnotation>,
) -> Result<ScopedStep> {
    let scope_vars = extract_slot_vars(slot);

    let converted = convert_slot(slot)?;
    json_annotations.extend(converted.json_annotations);

    Ok(ScopedStep {
        depth,
        scope_vars,
        select_steps: converted.select,
        ast_selector: converted.ast_selector,
        line_matcher: converted.line_matcher,
        marker_scope: converted.marker_scope,
        md_scope: converted.md_scope,
        md_matcher: converted.md_matcher,
    })
}

/// Result of converting a slot.
struct ConvertedSlot {
    select: Vec<SelectStep>,
    ast_selector: Option<AstSelector>,
    line_matcher: Option<LineMatcher>,
    marker_scope: Option<MarkerScope>,
    md_scope: Option<MdPattern>,
    md_matcher: Option<MdPattern>,
    json_annotations: Vec<crate::_2_pattern::ScanAnnotation>,
}

/// Convert a slot to SelectSteps.
fn convert_slot(slot: &Slot) -> Result<ConvertedSlot> {
    let mut select = vec![];
    let mut ast_selector: Option<AstSelector> = None;
    let mut line_matcher: Option<LineMatcher> = None;
    let mut marker_scope: Option<MarkerScope> = None;
    let mut md_scope: Option<MdPattern> = None;
    let mut md_matcher: Option<MdPattern> = None;
    let mut json_annotations = Vec::new();

    match slot {
        Slot::Bare(pattern) => {
            // Bare glob - infer context
            // For now, treat as file pattern
            select.push(SelectStep::File {
                pattern: pattern.clone(),
                capture: None,
            });
        }
        Slot::Tagged { tag, arg, body } => match tag {
            Tag::Fs => {
                select.push(SelectStep::File {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::Repo | Tag::RepoNorm => {
                select.push(SelectStep::Repo {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::Rev | Tag::RevNorm => {
                select.push(SelectStep::Rev {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::Folder => {
                select.push(SelectStep::Folder {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::File => {
                select.push(SelectStep::File {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::Json => {
                let (steps, annots) = parse_json_body(body)?;
                json_annotations.extend(annots);
                select.extend(steps);
            }
            Tag::Ast => {
                if ast_selector.is_some() {
                    bail!("multiple ast() slots not supported");
                }
                let (pattern, constraints, segment_captures) = rewrite_ast_braced_captures(body);
                let ast_captures = build_ast_captures_map(body);
                ast_selector = Some(AstSelector {
                    pattern: Some(pattern),
                    rule: None,
                    constraints,
                    rule_file: None,
                    language: arg.clone(),
                    capture: "$NAME".to_string(),
                    captures: if ast_captures.is_empty() { None } else { Some(ast_captures) },
                    segment_captures,
                });
            }
            Tag::Line => {
                if line_matcher.is_some() {
                    bail!("multiple line() slots not supported");
                }
                line_matcher = if let Some(re_pat) = body.strip_prefix("re:") {
                    let has_dollar = re_pat.contains('$');
                    let pattern = if has_dollar {
                        rewrite_re_dollar_captures(re_pat)
                    } else {
                        re_pat.to_string()
                    };
                    Some(LineMatcher::Regex {
                        source: String::new(),
                        pattern,
                        // Raw (?P<>) patterns: full-match anchored.
                        // $-sugar patterns: search mode (captures provide boundaries).
                        full_match: !has_dollar,
                    })
                } else {
                    Some(LineMatcher::Segments {
                        source: String::new(),
                        pattern: body.clone(),
                    })
                };
            }
            Tag::Marker => {
                if marker_scope.is_some() {
                    bail!("multiple marker() slots not supported");
                }
                marker_scope = Some(parse_marker_body(body)?);
            }
            Tag::Md => {
                let parsed = parse_md_body(body)?;
                // Heading patterns can be scopers (intermediate in chain) or matchers (terminal).
                // For now, we store the first heading pattern as md_scope and any subsequent
                // as md_matcher. The extractor handles the distinction.
                match &parsed {
                    MdPattern::Heading { .. } => {
                        if md_scope.is_some() {
                            // Second md() heading in same rule -> this one is the matcher
                            md_matcher = Some(parsed);
                        } else {
                            md_scope = Some(parsed);
                        }
                    }
                    _ => {
                        md_matcher = Some(parsed);
                    }
                }
            }
        },
    }

    Ok(ConvertedSlot {
        select,
        ast_selector,
        line_matcher,
        marker_scope,
        md_scope,
        md_matcher,
        json_annotations,
    })
}

/// Parse marker() body into a MarkerScope.
///
/// One arg: `marker("SECTION:")` -> flat sequential regions.
/// Two args: `marker("BEGIN:", "END:")` -> paired open/close.
///
/// The body string is the raw content inside the parens. Quotes are part of the
/// body because the parser doesn't strip them (they're inside the tag body).
/// We strip quotes here and split on `, ` for the two-arg form.
fn parse_marker_body(body: &str) -> Result<MarkerScope> {
    let body = body.trim();

    // Try splitting on comma for two-arg form
    // Each arg may be quoted: "open", "close"
    let args = split_marker_args(body)?;
    match args.len() {
        1 => Ok(MarkerScope {
            open: args[0].clone(),
            close: None,
            capture: None,
        }),
        2 => Ok(MarkerScope {
            open: args[0].clone(),
            close: Some(args[1].clone()),
            capture: None,
        }),
        _ => bail!("marker() takes 1 or 2 arguments, got {}", args.len()),
    }
}

/// Split marker body into 1 or 2 quoted string args.
fn split_marker_args(body: &str) -> Result<Vec<String>> {
    let mut args = vec![];
    let mut rest = body.trim();

    loop {
        if rest.is_empty() {
            break;
        }
        // Expect a quoted string
        if rest.starts_with('"') {
            let end = rest[1..].find('"').ok_or_else(|| anyhow::anyhow!("unterminated string in marker()"))?;
            args.push(rest[1..=end].to_string());
            rest = rest[end + 2..].trim();
            if rest.starts_with(',') {
                rest = rest[1..].trim();
            }
        } else {
            // Unquoted: take until comma or end
            let end = rest.find(',').unwrap_or(rest.len());
            let arg = rest[..end].trim();
            if !arg.is_empty() {
                args.push(arg.to_string());
            }
            rest = if end < rest.len() { rest[end + 1..].trim() } else { "" };
        }
    }

    if args.is_empty() {
        bail!("marker() requires at least 1 argument");
    }
    Ok(args)
}

/// Parse md() body into an MdPattern.
///
/// Syntax detection by leading characters:
///   `# heading`     -> Heading (level by # count)
///   `- item`        -> ListItem
///   `[$TEXT]($URL)`  -> Link
///   `` ```lang ``    -> CodeBlock
///   `| row |`        -> TableRow
///   `> quote`        -> Blockquote
fn parse_md_body(body: &str) -> Result<MdPattern> {
    let body = body.trim();

    // Heading: starts with one or more #
    if body.starts_with('#') {
        let hashes = body.bytes().take_while(|&b| b == b'#').count();
        if hashes > 6 {
            bail!("md() heading level must be 1-6, got {}", hashes);
        }
        let rest = body[hashes..].trim();
        let (text, capture) = parse_md_text_and_capture(rest);
        return Ok(MdPattern::Heading {
            level: hashes as u8,
            text,
            capture,
        });
    }

    // List item: starts with - or * or + or digit.
    if body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ") {
        let rest = body[2..].trim();
        let capture = extract_sole_capture(rest);
        return Ok(MdPattern::ListItem { capture });
    }
    if body.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        if let Some(idx) = body.find(". ").or_else(|| body.find(") ")) {
            let rest = body[idx + 2..].trim();
            let capture = extract_sole_capture(rest);
            return Ok(MdPattern::ListItem { capture });
        }
    }

    // Link: [text](url)
    if body.starts_with('[') && body.contains("](") && body.ends_with(')') {
        let bracket_end = body.find("](").unwrap();
        let text_part = &body[1..bracket_end];
        let url_part = &body[bracket_end + 2..body.len() - 1];
        let text_capture = extract_sole_capture(text_part);
        let url_capture = extract_sole_capture(url_part);
        return Ok(MdPattern::Link {
            text_capture,
            url_capture,
        });
    }

    // Code block: ```lang
    if body.starts_with("```") {
        let rest = body[3..].trim();
        let lang_capture = extract_sole_capture(rest);
        return Ok(MdPattern::CodeBlock {
            lang_capture,
            body_capture: None,
        });
    }

    // Table row: | ... |
    if body.starts_with('|') && body.ends_with('|') {
        let capture = extract_sole_capture(body);
        return Ok(MdPattern::TableRow { capture });
    }

    // Blockquote: > text
    if body.starts_with("> ") || body == ">" {
        let rest = if body.len() > 2 { body[2..].trim() } else { "" };
        let capture = extract_sole_capture(rest);
        return Ok(MdPattern::Blockquote { capture });
    }

    bail!(
        "md() pattern not recognized: {:?}. Expected heading (#), list (- ), link ([]()), \
         code block (```), table (| |), or blockquote (>)",
        body
    )
}

/// Parse heading text into optional text filter and capture name.
///
/// `$TITLE` -> (None, Some("TITLE"))  -- capture-only
/// `Installation` -> (Some("Installation"), None) -- literal only
/// `$ORG/$REPO` -> (Some("$ORG/$REPO"), None) -- regex extraction handles captures
fn parse_md_text_and_capture(text: &str) -> (Option<String>, Option<String>) {
    if text.is_empty() {
        return (None, None);
    }

    // If it's a single $VAR with no surrounding text, use as capture name
    if text.starts_with('$')
        && !text.starts_with("$$$")
        && !text.contains('/')
        && !text.contains(' ')
    {
        let var = &text[1..];
        if !var.is_empty()
            && var
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        {
            return (Some(text.to_string()), Some(var.to_string()));
        }
    }

    // If it contains $VAR patterns, it's a text filter with embedded captures
    if text.contains('$') {
        return (Some(text.to_string()), None);
    }

    // Literal text
    (Some(text.to_string()), None)
}

/// If `text` is exactly `$VAR`, return Some(var_name). Otherwise None.
fn extract_sole_capture(text: &str) -> Option<String> {
    let text = text.trim();
    if text.starts_with('$') && !text.starts_with("$$$") && !text.starts_with("$_") {
        let var = &text[1..];
        if !var.is_empty()
            && !var.contains(' ')
            && var
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        {
            return Some(var.to_string());
        }
    }
    None
}

/// Rewrite an ast pattern body that contains `${VAR}` braced captures.
///
/// Scans for identifier-like tokens containing `${...}`. Each such token
/// is replaced with a synthetic metavar `$SPREFAN`, a regex constraint
/// is generated for ast-grep filtering, and the original token is stored
/// as a segment pattern for post-match extraction.
///
/// Returns `(rewritten_pattern, constraints, segment_captures)`.
/// If no `${` is found, returns the original pattern with None for both maps.
/// Build a captures map from $VAR and $$$VAR in an ast pattern body.
/// Maps metavar name (with $) to column name (without $).
/// e.g. "fn $NAME($$$ARGS)" -> {"$NAME": "NAME", "$$$ARGS": "ARGS"}
fn build_ast_captures_map(body: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let dollar_start = i;
            i += 1;
            // Count consecutive $ for multi-capture ($$$)
            while i < bytes.len() && bytes[i] == b'$' {
                i += 1;
            }
            // Skip $_ wildcard
            if i < bytes.len() && bytes[i] == b'_' && (i + 1 >= bytes.len() || !bytes[i + 1].is_ascii_alphanumeric()) {
                i += 1;
                continue;
            }
            // Skip ${BRACED} -- handled by segment_captures
            if i < bytes.len() && bytes[i] == b'{' {
                while i < bytes.len() && bytes[i] != b'}' {
                    i += 1;
                }
                if i < bytes.len() { i += 1; }
                continue;
            }
            let name_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > name_start {
                let name = &body[name_start..i];
                if name.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
                    let metavar = body[dollar_start..i].to_string();
                    map.insert(metavar, name.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    map
}

fn rewrite_ast_braced_captures(
    body: &str,
) -> (
    String,
    Option<serde_json::Value>,
    Option<BTreeMap<String, String>>,
) {
    if !body.contains("${") {
        return (body.to_string(), None, None);
    }

    let mut constraints = serde_json::Map::new();
    let mut seg_caps = BTreeMap::new();
    let mut result = String::new();
    let mut cap_idx = 0u32;

    let bytes = body.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Check if we're at the start of a token that might contain ${
        if is_ast_ident_char(bytes[i]) || (bytes[i] == b'$' && peek_brace(bytes, i)) {
            // Scan the full token: identifier chars + ${...} sequences
            let token_start = i;
            let mut has_brace_cap = false;

            while i < bytes.len() {
                if bytes[i] == b'$' && peek_brace(bytes, i) {
                    has_brace_cap = true;
                    i += 2; // skip ${
                    while i < bytes.len() && bytes[i] != b'}' {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1; // skip }
                    }
                } else if is_ast_ident_char(bytes[i]) || bytes[i] == b'$' {
                    i += 1;
                } else {
                    break;
                }
            }

            let token = &body[token_start..i];

            if has_brace_cap {
                let metavar_name = format!("SPREFA{cap_idx}");
                cap_idx += 1;

                let segments = parse_segment_pattern(token);
                let regex_str = segments_to_constraint_regex(&segments);

                constraints.insert(
                    metavar_name.clone(),
                    serde_json::json!({ "regex": regex_str }),
                );
                seg_caps.insert(metavar_name.clone(), token.to_string());

                result.push('$');
                result.push_str(&metavar_name);
            } else {
                result.push_str(token);
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    (
        result,
        Some(serde_json::Value::Object(constraints)),
        Some(seg_caps),
    )
}

fn is_ast_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Check if position `i` is `$` followed by `{`.
fn peek_brace(bytes: &[u8], i: usize) -> bool {
    bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{'
}

/// Rewrite a `re:` line pattern that contains `$NAME` captures into a proper regex.
///
/// `$NAME`  -> `(?P<NAME>[^X\s]+)` where X is the first char of the next literal,
///             or `(?P<NAME>\S+)` at end of pattern / before whitespace literal.
/// `$$$NAME` -> `(?P<NAME>.+)` (greedy, crosses whitespace).
/// `$_`     -> `\S+` (unnamed).
/// `$$$_`   -> `.+` (unnamed).
/// Literal segments pass through as-is (they're already regex).
fn rewrite_re_dollar_captures(pattern: &str) -> String {
    let segments = parse_segment_pattern(pattern);
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        match seg {
            Segment::Literal(s) => out.push_str(s),
            Segment::Capture(name) => {
                out.push_str(&format!("(?P<{}>[a-zA-Z0-9._/-]+)", name));
            }
            Segment::MultiCapture(name) => {
                out.push_str(&format!("(?P<{}>.+)", name));
            }
            Segment::Wild => out.push_str("\\S+"),
            Segment::MultiWild => out.push_str(".+"),
        }
    }
    out
}

/// Find the first raw character of the next Literal segment (skipping captures).
fn next_literal_first_char(segments: &[Segment]) -> Option<char> {
    for seg in segments {
        if let Segment::Literal(s) = seg {
            return s.chars().next();
        }
    }
    None
}

/// Convert parsed segments into a regex string for ast-grep constraint filtering.
/// Wraps in `^...$` anchors.
fn segments_to_constraint_regex(segments: &[Segment]) -> String {
    let mut regex = String::from("^");
    for seg in segments {
        match seg {
            Segment::Literal(s) => regex.push_str(&regex::escape(s)),
            Segment::Capture(_) | Segment::Wild => regex.push_str(".+"),
            Segment::MultiCapture(_) | Segment::MultiWild => regex.push_str(".*"),
        }
    }
    regex.push('$');
    regex
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_1_parse::parse_program;

    fn lower(input: &str) -> Vec<Rule> {
        let program = parse_program(input).unwrap();
        let (ruleset, _edges) = lower_program(&program).unwrap();
        ruleset.rules
    }

    #[test]
    fn lower_flat_rule() {
        let rules = lower("rule(pkg) { fs(**/Cargo.toml) > json({ package: { name: $NAME } }) };");
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert_eq!(r.name, "pkg");

        assert!(
            matches!(&r.select[0], SelectStep::File { pattern, .. } if pattern == "**/Cargo.toml")
        );
        // select[1] is the Object from json body
        assert!(matches!(&r.select[1], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_scoped_rule() {
        let rules = lower(
            r#"rule(deploy) {
            repo($REPO) {
                fs(**/values.yaml) > json({ svc: $SVC })
            }
        };"#,
        );
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        assert_eq!(r.name, "deploy");

        // Should have: repo, file, object
        assert!(matches!(&r.select[0], SelectStep::Repo { .. }));
        assert!(matches!(&r.select[1], SelectStep::File { .. }));
        assert!(matches!(&r.select[2], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_nested_scopes() {
        let rules = lower(
            r#"rule(img) {
            repo($REPO) {
                rev(main) {
                    folder(packages/$PKG) {
                        fs(values.yaml) > json({ image: { repo: $REPO, tag: $TAG } })
                    }
                }
            }
        };"#,
        );

        assert_eq!(rules.len(), 1);
        let r = &rules[0];

        // Order: repo, rev, folder, file, object
        assert!(matches!(&r.select[0], SelectStep::Repo { .. }));
        assert!(matches!(&r.select[1], SelectStep::Rev { .. }));
        assert!(matches!(&r.select[2], SelectStep::Folder { .. }));
        assert!(matches!(&r.select[3], SelectStep::File { .. }));
        assert!(matches!(&r.select[4], SelectStep::Object { .. }));
    }

    #[test]
    fn lower_with_scan_annotations() {
        // repo() and rev() tags in body infer scan annotations
        let rules = lower(
            r#"rule(deploy) {
            repo($REPO) {
                rev($TAG) {
                    fs(**/values.yaml) > json({ image: { repo: $REPO, tag: $TAG } })
                }
            }
        };"#,
        );
        let r = &rules[0];
        let repo_match = r
            .create_matches
            .iter()
            .find(|m| m.capture == "REPO")
            .unwrap();
        let tag_match = r
            .create_matches
            .iter()
            .find(|m| m.capture == "TAG")
            .unwrap();
        assert_eq!(repo_match.scan.as_deref(), Some("repo"));
        assert_eq!(tag_match.scan.as_deref(), Some("rev"));
    }

    #[test]
    fn lower_with_inline_scan_annotations() {
        // repo()/rev() wrappers inside json() body set scan annotations
        let rules = lower(
            r#"rule(deploy) {
            fs(**/values.yaml) > json({ image: { repository: repo($REPO), tag: rev($TAG) } })
        };"#,
        );
        let r = &rules[0];
        let repo_match = r
            .create_matches
            .iter()
            .find(|m| m.capture == "REPO")
            .unwrap();
        let tag_match = r
            .create_matches
            .iter()
            .find(|m| m.capture == "TAG")
            .unwrap();
        assert_eq!(repo_match.scan.as_deref(), Some("repo"));
        assert_eq!(tag_match.scan.as_deref(), Some("rev"));
    }

    #[test]
    fn lower_with_norm_scan_annotations_tag_form() {
        // repo.norm() / rev.norm() as top-level tags.
        let rules = lower(
            r#"rule(deploy) {
            repo.norm($REPO) {
                rev.norm($TAG) {
                    fs(**/values.yaml) > json({ image: { repo: $REPO, tag: $TAG } })
                }
            }
        };"#,
        );
        let r = &rules[0];
        let repo_match = r
            .create_matches
            .iter()
            .find(|m| m.capture == "REPO")
            .unwrap();
        let tag_match = r
            .create_matches
            .iter()
            .find(|m| m.capture == "TAG")
            .unwrap();
        assert_eq!(repo_match.scan.as_deref(), Some("repo.norm"));
        assert_eq!(tag_match.scan.as_deref(), Some("rev.norm"));
    }

    #[test]
    fn lower_with_norm_scan_annotations_inline_form() {
        // repo.norm() / rev.norm() inside json() body.
        let rules = lower(
            r#"rule(deploy) {
            fs(**/values.yaml) > json({ image: { repository: repo.norm($REPO), tag: rev.norm($TAG) } })
        };"#,
        );
        let r = &rules[0];
        let repo_match = r
            .create_matches
            .iter()
            .find(|m| m.capture == "REPO")
            .unwrap();
        let tag_match = r
            .create_matches
            .iter()
            .find(|m| m.capture == "TAG")
            .unwrap();
        assert_eq!(repo_match.scan.as_deref(), Some("repo.norm"));
        assert_eq!(tag_match.scan.as_deref(), Some("rev.norm"));
    }

    #[test]
    fn lower_ast_with_lang() {
        let rules = lower(
            "rule(imports) { fs(**/*.config) > ast[typescript](import $NAME from '$PATH') };",
        );
        let r = &rules[0];
        let ast = r.select_ast.as_ref().unwrap();
        assert_eq!(ast.language.as_deref(), Some("typescript"));
        assert_eq!(ast.pattern.as_deref(), Some("import $NAME from '$PATH'"));
    }

    #[test]
    fn lower_line_segments() {
        let rules = lower(r"rule(img) { fs(deploy/**/*.yaml) > line($REPO:$TAG) };");
        let r = &rules[0];
        assert!(matches!(
            r.value.as_ref().unwrap(),
            LineMatcher::Segments { pattern, .. } if pattern == "$REPO:$TAG"
        ));
    }

    #[test]
    fn lower_line_regex() {
        let rules = lower(
            r"rule(img) { fs(deploy/**/*.yaml) > line(re:image:\s+(?P<REPO>[^:]+):(?P<TAG>.+)) };",
        );
        let r = &rules[0];
        assert!(matches!(
            r.value.as_ref().unwrap(),
            LineMatcher::Regex { pattern, .. } if pattern == r"image:\s+(?P<REPO>[^:]+):(?P<TAG>.+)"
        ));
    }

    #[test]
    fn lower_line_regex_dollar_sugar() {
        let rules = lower(r"rule(img) { fs(**/Dockerfile) > line(re:FROM\s+$IMAGE:$TAG) };");
        let r = &rules[0];
        match r.value.as_ref().unwrap() {
            LineMatcher::Regex { pattern, .. } => {
                // $IMAGE and $TAG -> [a-zA-Z0-9._/-]+
                assert_eq!(pattern, r"FROM\s+(?P<IMAGE>[a-zA-Z0-9._/-]+):(?P<TAG>[a-zA-Z0-9._/-]+)");
            }
            other => panic!("expected Regex, got {:?}", other),
        }
        // create_matches should include IMAGE and TAG
        let caps: Vec<&str> = r.create_matches.iter().map(|m| m.capture.as_str()).collect();
        assert!(caps.contains(&"IMAGE"), "got {:?}", caps);
        assert!(caps.contains(&"TAG"), "got {:?}", caps);
    }

    // ── ${VAR} braced capture lowering ──────────────

    #[test]
    fn rewrite_braced_no_captures() {
        let (pat, constraints, seg) = rewrite_ast_braced_captures("import $NAME from $PATH");
        assert_eq!(pat, "import $NAME from $PATH");
        assert!(constraints.is_none());
        assert!(seg.is_none());
    }

    #[test]
    fn rewrite_braced_single_capture() {
        let (pat, constraints, seg) = rewrite_ast_braced_captures("use${ENTITY}Query($$$ARGS)");
        assert_eq!(pat, "$SPREFA0($$$ARGS)");
        let c = constraints.unwrap();
        assert_eq!(c["SPREFA0"]["regex"].as_str().unwrap(), "^use.+Query$");
        let s = seg.unwrap();
        assert_eq!(s["SPREFA0"], "use${ENTITY}Query");
    }

    #[test]
    fn rewrite_braced_multiple_captures() {
        let (pat, constraints, seg) =
            rewrite_ast_braced_captures("${PREFIX}Service.${METHOD}($$$ARGS)");
        // Two tokens with braces: ${PREFIX}Service and ${METHOD}
        // ${PREFIX}Service -> $SPREFA0, ${METHOD} -> $SPREFA1
        assert_eq!(pat, "$SPREFA0.$SPREFA1($$$ARGS)");
        let c = constraints.unwrap();
        assert!(c["SPREFA0"]["regex"].as_str().is_some());
        assert!(c["SPREFA1"]["regex"].as_str().is_some());
        let s = seg.unwrap();
        assert_eq!(s["SPREFA0"], "${PREFIX}Service");
        assert_eq!(s["SPREFA1"], "${METHOD}");
    }

    #[test]
    fn lower_ast_braced_capture() {
        let rules =
            lower(r"rule(hooks) { fs(**/*.ts) > ast[typescript](use${ENTITY}Query($$$ARGS)) };");
        let r = &rules[0];
        let ast = r.select_ast.as_ref().unwrap();
        assert_eq!(ast.pattern.as_deref(), Some("$SPREFA0($$$ARGS)"));
        assert!(ast.constraints.is_some());
        assert!(ast.segment_captures.is_some());
        let seg = ast.segment_captures.as_ref().unwrap();
        assert_eq!(seg["SPREFA0"], "use${ENTITY}Query");
    }

    // ── Dependency edge extraction ────────────────────

    fn lower_with_edges(input: &str) -> (Vec<Rule>, Vec<DepEdge>) {
        let program = parse_program(input).unwrap();
        let (ruleset, edges) = lower_program(&program).unwrap();
        (ruleset.rules, edges)
    }

    #[test]
    fn cross_ref_produces_dep_edge() {
        let (rules, edges) = lower_with_edges(
            r#"
            rule(deploy_config) {
                fs(**/services.yaml) > json({ services: { $SVC: { repo: repo($REPO), pin: rev($PIN) } } })
            };
            rule(svc_version) {
                deploy_config(repo: $REPO, pin: $PIN)
                repo($REPO) > rev($PIN) > fs(**/package.json) > json({ version: $VERSION })
            };
            "#,
        );
        assert_eq!(rules.len(), 2);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].producer, "deploy_config");
        assert_eq!(edges[0].consumer, "svc_version");
        assert_eq!(
            edges[0].bindings,
            vec![
                ("repo".to_string(), "REPO".to_string()),
                ("pin".to_string(), "PIN".to_string()),
            ]
        );
    }

    #[test]
    fn no_cross_refs_no_edges() {
        let (_rules, edges) = lower_with_edges(
            r#"
            rule(a) { fs(**/*.yaml) > json({ name: $NAME }) };
            rule(b) { fs(**/*.json) > json({ version: $VER }) };
            "#,
        );
        assert!(edges.is_empty());
    }

    // ── Monomorphization ─────────────────────────────

    #[test]
    fn monomorphize_single_path_unchanged() {
        use crate::_0_ast::{Slot, Tag};
        let body = vec![RuleBody::Block {
            slot: Slot::Tagged { tag: Tag::Repo, arg: None, body: "$REPO".into() },
            is_chain: false,
            children: vec![RuleBody::Block {
                slot: Slot::Tagged { tag: Tag::Rev, arg: None, body: "main".into() },
                is_chain: false,
                children: vec![RuleBody::Step(Slot::Tagged {
                    tag: Tag::Fs,
                    arg: None,
                    body: "a".into(),
                })],
            }],
        }];
        let paths = monomorphize_bodies(&body);
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn monomorphize_two_siblings() {
        use crate::_0_ast::{Slot, Tag};
        let body = vec![RuleBody::Block {
            slot: Slot::Tagged { tag: Tag::Repo, arg: None, body: "$REPO".into() },
            is_chain: false,
            children: vec![
                RuleBody::Block {
                    slot: Slot::Tagged { tag: Tag::Rev, arg: None, body: "main".into() },
                    is_chain: false,
                    children: vec![RuleBody::Step(Slot::Tagged {
                        tag: Tag::Fs,
                        arg: None,
                        body: "a".into(),
                    })],
                },
                RuleBody::Block {
                    slot: Slot::Tagged { tag: Tag::Rev, arg: None, body: "staging".into() },
                    is_chain: false,
                    children: vec![RuleBody::Step(Slot::Tagged {
                        tag: Tag::Fs,
                        arg: None,
                        body: "b".into(),
                    })],
                },
            ],
        }];
        let paths = monomorphize_bodies(&body);
        assert_eq!(paths.len(), 2);
        // Each path is one top-level Block(repo) with exactly one child.
        for path in &paths {
            assert_eq!(path.len(), 1);
            match &path[0] {
                RuleBody::Block { slot: Slot::Tagged { tag: Tag::Repo, .. }, children, .. } => {
                    assert_eq!(children.len(), 1);
                }
                _ => panic!("expected repo block"),
            }
        }
    }

    #[test]
    fn monomorphize_deep_fork() {
        use crate::_0_ast::{Slot, Tag};
        // repo($R) { rev(main) { fs(a); fs(b) } } -> 2 paths
        // is_chain: false because children are brace-level siblings
        let body = vec![RuleBody::Block {
            slot: Slot::Tagged { tag: Tag::Repo, arg: None, body: "$R".into() },
            is_chain: false,
            children: vec![RuleBody::Block {
                slot: Slot::Tagged { tag: Tag::Rev, arg: None, body: "main".into() },
                is_chain: false,
                children: vec![
                    RuleBody::Step(Slot::Tagged { tag: Tag::Fs, arg: None, body: "a".into() }),
                    RuleBody::Step(Slot::Tagged { tag: Tag::Fs, arg: None, body: "b".into() }),
                ],
            }],
        }];
        let paths = monomorphize_bodies(&body);
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn lower_sibling_revs_produces_two_rules() {
        let rules = lower(
            r#"rule(drift) {
                repo($REPO) {
                    rev(main) > fs(**/values.yaml) > json({ image: $PROD });
                    rev(staging) > fs(**/values.yaml) > json({ image: $STAGE })
                }
            };"#,
        );
        let drift: Vec<_> = rules.iter().filter(|r| r.name == "drift").collect();
        assert_eq!(drift.len(), 2);

        // Both must have identical create_matches (all captures from all branches).
        assert_eq!(drift[0].create_matches.len(), drift[1].create_matches.len());
        let caps_0: Vec<&str> = drift[0].create_matches.iter().map(|m| m.capture.as_str()).collect();
        let caps_1: Vec<&str> = drift[1].create_matches.iter().map(|m| m.capture.as_str()).collect();
        assert_eq!(caps_0, caps_1);
        assert!(caps_0.contains(&"REPO"));
        assert!(caps_0.contains(&"PROD"));
        assert!(caps_0.contains(&"STAGE"));

        // Each branch selects a different rev pattern.
        let has_main = drift[0].select.iter().any(|s| matches!(s, SelectStep::Rev { pattern, .. } if pattern == "main"));
        let has_staging = drift[1].select.iter().any(|s| matches!(s, SelectStep::Rev { pattern, .. } if pattern == "staging"));
        assert!(has_main, "branch 0 should select rev(main)");
        assert!(has_staging, "branch 1 should select rev(staging)");
    }

    #[test]
    fn lower_single_branch_still_one_rule() {
        let rules = lower(
            r#"rule(pkg) {
                repo($REPO) {
                    rev(main) > fs(**/Cargo.toml) > json({ package: { name: $NAME } })
                }
            };"#,
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "pkg");
    }
}
