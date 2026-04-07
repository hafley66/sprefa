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
use sprefa_rules::types::{AstSelector, LineMatcher, MatchDef, Rule, RuleSet, SelectStep};
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
                let rule = lower_rule_decl(decl)?;
                collect_dep_edges(&decl.body, &decl.name, &mut edges);
                rules.push(rule);
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

/// Walk rule bodies to find cross-rule references and emit dependency edges.
fn collect_dep_edges(bodies: &[RuleBody], consumer: &str, edges: &mut Vec<DepEdge>) {
    for body in bodies {
        match body {
            RuleBody::Step(_) => {}
            RuleBody::Block { children, .. } => {
                collect_dep_edges(children, consumer, edges);
            }
            RuleBody::Ref { cross_ref, children } => {
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
}

fn lower_rule_decl(decl: &RuleDecl) -> Result<Rule> {
    // Flatten all body items into scoped steps, collecting json annotations.
    let mut all_scoped = vec![];
    let mut json_annotations = vec![];
    for body in &decl.body {
        let flattened = flatten_body(body, 0, &HashSet::new(), &mut json_annotations)?;
        all_scoped.extend(flattened);
    }

    // Sort by scope depth - outer scopes must execute before inner
    let mut scoped_steps = all_scoped;
    scoped_steps.sort_by_key(|s| s.depth);

    // Collect all select steps in order
    let mut select: Vec<SelectStep> = vec![];
    let mut select_ast: Option<AstSelector> = None;
    let mut line_matcher: Option<LineMatcher> = None;

    for scoped in &scoped_steps {
        select.extend(scoped.select_steps.clone());
        if let Some(ast) = scoped.ast_selector.clone() {
            select_ast = Some(ast);
        }
        if let Some(lm) = scoped.line_matcher.clone() {
            line_matcher = Some(lm);
        }
    }

    // Infer create_matches from all $VARs in body.
    // Detect repo()/rev() tags and inline json annotations to set scan annotations.
    let mut scan_vars: HashMap<String, String> = HashMap::new();
    collect_scan_annotations(&decl.body, &mut scan_vars);
    for annot in &json_annotations {
        scan_vars.insert(annot.var.clone(), annot.kind.clone());
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
            scan: scan_vars.get(var).cloned(),
        })
        .collect();

    Ok(Rule {
        name: decl.name.clone(),
        description: None,
        select,
        select_ast,
        value: line_matcher,
        create_matches,
        confidence: None,
    })
}

/// Walk rule bodies to find repo($VAR)/rev($VAR) tags and record their
/// captured variables as scan-driving columns.
fn collect_scan_annotations(bodies: &[RuleBody], scan_vars: &mut HashMap<String, String>) {
    for body in bodies {
        let (slot, children) = match body {
            RuleBody::Step(slot) => (Some(slot), &[][..]),
            RuleBody::Block { slot, children } => (Some(slot), children.as_slice()),
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
        RuleBody::Block { slot, children } => {
            let block_vars = extract_slot_vars(slot);
            let mut available_vars = parent_vars.clone();
            available_vars.extend(block_vars.iter().cloned());

            let block_scoped =
                slot_to_scoped_step(slot, depth, parent_vars, json_annotations)?;
            result.push(block_scoped);

            for child in children {
                let child_scoped =
                    flatten_body(child, depth + 1, &available_vars, json_annotations)?;
                result.extend(child_scoped);
            }
        }
        RuleBody::Ref { cross_ref, children } => {
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

    let (select_steps, ast_selector, line_matcher, annots) = convert_slot(slot)?;
    json_annotations.extend(annots);

    Ok(ScopedStep {
        depth,
        scope_vars,
        select_steps,
        ast_selector,
        line_matcher,
    })
}

/// Convert a slot to SelectSteps.
fn convert_slot(
    slot: &Slot,
) -> Result<(
    Vec<SelectStep>,
    Option<AstSelector>,
    Option<LineMatcher>,
    Vec<crate::_2_pattern::ScanAnnotation>,
)> {
    let mut select = vec![];
    let mut ast_selector: Option<AstSelector> = None;
    let mut line_matcher: Option<LineMatcher> = None;
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
            Tag::Repo => {
                select.push(SelectStep::Repo {
                    pattern: body.clone(),
                    capture: None,
                });
            }
            Tag::Rev => {
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
                let (pattern, constraints, segment_captures) =
                    rewrite_ast_braced_captures(body);
                ast_selector = Some(AstSelector {
                    pattern: Some(pattern),
                    rule: None,
                    constraints,
                    rule_file: None,
                    language: arg.clone(),
                    capture: "$NAME".to_string(),
                    captures: None,
                    segment_captures,
                });
            }
            Tag::Line => {
                if line_matcher.is_some() {
                    bail!("multiple line() slots not supported");
                }
                line_matcher = if let Some(re_pat) = body.strip_prefix("re:") {
                    Some(LineMatcher::Regex {
                        source: String::new(),
                        pattern: re_pat.to_string(),
                        full_match: true,
                    })
                } else {
                    Some(LineMatcher::Segments {
                        source: String::new(),
                        pattern: body.clone(),
                    })
                };
            }
        },
    }

    Ok((select, ast_selector, line_matcher, json_annotations))
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
        let rules =
            lower("rule(pkg) { fs(**/Cargo.toml) > json({ package: { name: $NAME } }) };");
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
        let repo_match = r.create_matches.iter().find(|m| m.capture == "REPO").unwrap();
        let tag_match = r.create_matches.iter().find(|m| m.capture == "TAG").unwrap();
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
        let repo_match = r.create_matches.iter().find(|m| m.capture == "REPO").unwrap();
        let tag_match = r.create_matches.iter().find(|m| m.capture == "TAG").unwrap();
        assert_eq!(repo_match.scan.as_deref(), Some("repo"));
        assert_eq!(tag_match.scan.as_deref(), Some("rev"));
    }

    #[test]
    fn lower_ast_with_lang() {
        let rules = lower("rule(imports) { fs(**/*.config) > ast[typescript](import $NAME from '$PATH') };");
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
        assert_eq!(
            c["SPREFA0"]["regex"].as_str().unwrap(),
            "^use.+Query$"
        );
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
        let rules = lower(
            r"rule(hooks) { fs(**/*.ts) > ast[typescript](use${ENTITY}Query($$$ARGS)) };",
        );
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
        assert_eq!(edges[0].bindings, vec![
            ("repo".to_string(), "REPO".to_string()),
            ("pin".to_string(), "PIN".to_string()),
        ]);
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
}
